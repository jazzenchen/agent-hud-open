//! The demo's transcripts: the lines of a session's header, its turns and its
//! sub-agents, as Claude Code, Codex and DeepSeek Harness write them.
use super::data::{DemoAgent, DemoTurn, Kinds, Plan};
use super::{folder, Ending};
use crate::iso;
use crate::l10n::tr;
use serde_json::{json, Value};

/// What a session's later prompts ask; the first is its title.
const PROMPTS: [&str; 5] = [
    "Keep going.",
    "Fix the failing check and run it again.",
    "Tidy this up before we commit.",
    "Use the existing helper instead.",
    "Show me what changed.",
];

fn prompt(plan: &Plan, index: usize) -> &'static str {
    if index == 0 {
        plan.title
    } else {
        PROMPTS[index % PROMPTS.len()]
    }
}

pub(super) fn reply(plan: &Plan) -> String {
    tr!("Done: {{task}}.", task = plan.title)
}

pub(super) fn jsonl(lines: Vec<Value>) -> String {
    lines
        .into_iter()
        .map(|line| line.to_string() + "\n")
        .collect()
}

/// A tool call's input, as the tool takes it, in the project at `dir`.
fn tool_input(tool: &str, dir: &str) -> Value {
    match tool {
        "Bash" => json!({"command":"swift test"}),
        "Edit" => {
            json!({"file_path":format!("{dir}/Sources/App/ContentView.swift"),"old_string":"","new_string":""})
        }
        "Glob" => json!({"pattern":"**/*.swift"}),
        "Grep" => json!({"pattern":"TODO|FIXME","path":dir}),
        _ => json!({"file_path":format!("{dir}/Sources/App/ContentView.swift")}),
    }
}

/// The tools a session's own calls ask for, in turn.
const OWN_TOOLS: [&str; 3] = ["Read", "Edit", "Bash"];

/// What Claude Code's Agent tool hands a sub-agent: its description, then
/// what is wanted back.
fn agent_prompt(agent: &DemoAgent) -> String {
    let wanted = match agent.task.kind {
        "Explore" => "Search the codebase and report the files, types and call sites involved. Change nothing.",
        "Plan" => "Lay out the steps, the files each one touches and the risks. Change nothing.",
        "test-runner" => "Run it, and for each failure give the test, the error and the likely cause.",
        "code-reviewer" => "Review the current diff for bugs and regressions, with file and line for each finding.",
        _ => "Make the change, build it and run the affected tests. Report what you changed.",
    };
    format!("{}\n\n{wanted}", agent.task.description)
}

/// A sub-agent's transcript: the task it was handed, then each call, the
/// last tool's result before the next, and its report unless it still works.
pub(super) fn agent_lines(plan: &Plan, agent: &DemoAgent) -> Vec<Value> {
    let dir = folder(plan.project);
    let id = &agent.id;
    let line = |kind: &str, at: i64, message: Value| json!({"type":kind,"isSidechain":true,"agentId":id,"sessionId":plan.id,"cwd":dir,"uuid":format!("{id}-{kind}-{at}"),"timestamp":iso(at),"message":message});
    let mut lines = vec![line(
        "user",
        agent.start,
        json!({"role":"user","content":agent_prompt(agent)}),
    )];
    let mut before = agent.start;
    for (call, step) in agent.calls.iter().enumerate() {
        if call > 0 {
            let back = before + (step.at - before) * 2 / 5;
            lines.push(line("user", back, json!({"role":"user","content":[{"type":"tool_result","tool_use_id":format!("{id}-t{}",call-1),"content":"ok"}]})));
        }
        let content = match step.tool {
            Some(tool) => {
                json!([{"type":"tool_use","id":format!("{id}-t{call}"),"name":tool,"input":tool_input(tool,&dir)}])
            }
            None => {
                json!([{"type":"text","text":tr!("Done: {{task}}.", task = agent.task.description)}])
            }
        };
        let usage = step.usage;
        lines.push(line("assistant", step.at, json!({"id":format!("msg-{id}-{call}"),"model":agent.model,"role":"assistant","content":content,"stop_reason":if step.tool.is_some() {"tool_use"} else {"end_turn"},"usage":{"input_tokens":usage.input,"output_tokens":usage.output+usage.reasoning,"cache_read_input_tokens":usage.cache_read,"cache_creation_input_tokens":usage.cache_write}})));
        before = step.at;
    }
    lines
}

/// One turn of `plan` as its client writes it: its prompt at `turn.start`,
/// each call at its time, the sub-agents it starts as Agent calls and their
/// results as they report back.
pub(super) fn turn_lines(
    plan: &Plan,
    index: usize,
    turn: &DemoTurn,
    ending: Ending,
    totals: &mut Kinds,
    seq: &mut u64,
) -> Vec<Value> {
    let (start, end, calls, own) = (turn.start, turn.end, turn.calls, turn.own);
    let at = |call: usize| turn.times[call];
    let text = match ending {
        Ending::Reply(Some(message)) => message.to_owned(),
        _ => reply(plan),
    };
    let dir = folder(plan.project);
    let mut lines = Vec::new();
    match plan.source {
        "claude" => {
            // Typed in the Claude desktop app, where most real Claude Code prompts come from.
            lines.push(json!({"type":"user","uuid":format!("{}-u{index}",plan.id),"sessionId":plan.id,"cwd":dir,"entrypoint":"claude-desktop","timestamp":iso(start),"message":{"role":"user","content":prompt(plan,index)}}));
            // Claude Code marks a compaction with a boundary line.
            if turn.compacted {
                lines.push(json!({"type":"system","subtype":"compact_boundary","uuid":format!("{}-c{index}",plan.id),"sessionId":plan.id,"cwd":dir,"timestamp":iso(start + 500),"compactMetadata":{"trigger":"manual"}}));
            }
            // Each finished sub-agent's result comes back before the first
            // call after its report.
            let mut reported = vec![false; turn.agents.len()];
            let mut results = |until: i64, lines: &mut Vec<Value>| {
                for (agent, done) in turn.agents.iter().zip(reported.iter_mut()) {
                    let back = agent.calls.last().map_or(agent.start, |call| call.at) + 400;
                    if !*done && !agent.open && back < until {
                        *done = true;
                        lines.push(json!({"type":"user","uuid":format!("{}-r{}",plan.id,agent.id),"sessionId":plan.id,"cwd":dir,"timestamp":iso(back),"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":agent.call,"content":[{"type":"text","text":format!("Done: {}.",agent.task.description)}]}]},"toolUseResult":{"status":"completed","agentId":agent.id}}));
                    }
                }
            };
            for call in 0..calls {
                results(at(call), &mut lines);
                let share = own.share(call, calls);
                let last = call + 1 == calls;
                let done = last && matches!(ending, Ending::Reply(_));
                let started = turn
                    .agents
                    .iter()
                    .filter(|agent| agent.launch == call)
                    .map(|agent| json!({"type":"tool_use","id":agent.call,"name":"Agent","input":{"description":agent.task.description,"prompt":agent_prompt(agent),"subagent_type":agent.task.kind}}))
                    .collect::<Vec<_>>();
                let content = if done {
                    json!([{"type":"text","text":text}])
                } else if !started.is_empty() {
                    Value::Array(started)
                } else {
                    let tool = OWN_TOOLS[(index + call) % OWN_TOOLS.len()];
                    json!([{"type":"tool_use","id":format!("{}-t{index}-{call}",plan.id),"name":tool,"input":tool_input(tool,&dir)}])
                };
                lines.push(json!({"type":"assistant","sessionId":plan.id,"cwd":dir,"timestamp":iso(at(call)),"message":{"id":format!("msg-{}-{index}-{call}",plan.id),"model":plan.model,"role":"assistant","content":content,"stop_reason":if done {"end_turn"} else {"tool_use"},"usage":{"input_tokens":share.input,"output_tokens":share.output+share.reasoning,"cache_read_input_tokens":share.cache_read,"cache_creation_input_tokens":share.cache_write}}}));
            }
            // A turn still waiting has the results back so far.
            if ending == Ending::Open {
                results(i64::MAX, &mut lines);
            }
            if ending == Ending::Interrupted {
                lines.push(json!({"type":"user","uuid":format!("{}-x{index}",plan.id),"sessionId":plan.id,"cwd":dir,"timestamp":iso(end),"message":{"role":"user","content":"[Request interrupted by user]"}}));
            }
        }
        "codex" => {
            let turn = format!("{}-t{index}", plan.id);
            lines.push(json!({"timestamp":iso(start),"type":"event_msg","payload":{"type":"user_message","message":prompt(plan,index)}}));
            lines.push(json!({"timestamp":iso(start),"type":"event_msg","payload":{"type":"task_started","turn_id":turn}}));
            for call in 0..calls {
                let share = own.share(call, calls);
                totals.input += share.input;
                totals.cache_write += share.cache_write;
                totals.cache_read += share.cache_read;
                totals.output += share.output;
                totals.reasoning += share.reasoning;
                let usage = |k: &Kinds| json!({"input_tokens":k.input+k.cache_read+k.cache_write,"cached_input_tokens":k.cache_read,"cache_write_input_tokens":k.cache_write,"output_tokens":k.output+k.reasoning,"reasoning_output_tokens":k.reasoning});
                lines.push(json!({"timestamp":iso(at(call)),"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":usage(totals),"last_token_usage":usage(&share)}}}));
            }
            match ending {
                Ending::Reply(_) => lines.push(json!({"timestamp":iso(end),"type":"event_msg","payload":{"type":"task_complete","turn_id":turn,"last_agent_message":text}})),
                Ending::Interrupted => lines.push(json!({"timestamp":iso(end),"type":"event_msg","payload":{"type":"turn_aborted","turn_id":turn,"reason":"interrupted"}})),
                Ending::Open => {}
            }
        }
        _ => {
            let turn = index as u64 + 1;
            let mut push = |kind: &str, time: i64, data: Value| {
                *seq += 1;
                lines.push(json!({"type":kind,"time":iso(time),"seq":*seq,"data":data}));
            };
            push("turn/start", start, json!({"turn":turn}));
            push(
                "user/message",
                start,
                json!({"turn":turn,"source":{"kind":"user"},"content":prompt(plan,index)}),
            );
            for call in 0..calls {
                let share = own.share(call, calls);
                push(
                    "assistant/message",
                    at(call),
                    json!({"turn":turn,"step":call,"usage":{"inputTokens":share.input,"outputTokens":share.output+share.reasoning,"reasoningTokens":share.reasoning,"cacheReadTokens":share.cache_read,"cacheWriteTokens":share.cache_write}}),
                );
            }
            match ending {
                Ending::Reply(_) => push(
                    "turn/end",
                    end,
                    json!({"turn":turn,"reason":{"kind":"completed"}}),
                ),
                Ending::Interrupted => push(
                    "turn/end",
                    end,
                    json!({"turn":turn,"reason":{"kind":"interrupted"}}),
                ),
                Ending::Open => {}
            }
        }
    }
    lines
}

/// A session's opening lines: the client's own header before its first turn.
pub(super) fn header(plan: &Plan, start: i64) -> Vec<Value> {
    let dir = folder(plan.project);
    match plan.source {
        "codex" => vec![
            json!({"timestamp":iso(start - 1_000),"type":"session_meta","payload":{"id":plan.id,"cwd":dir,"originator":"codex_cli_rs"}}),
            json!({"timestamp":iso(start - 1_000),"type":"turn_context","payload":{"model":plan.model,"cwd":dir}}),
        ],
        "deepseek" => vec![
            json!({"type":"session","version":4,"id":plan.id,"cwd":dir,"createdAt":iso(start - 1_000)}),
            json!({"type":"request/header","time":iso(start - 1_000),"seq":0,"data":{"header":{"config":{"model":plan.model,"provider":"deepseek"}}}}),
        ],
        _ => Vec::new(),
    }
}
