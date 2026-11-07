//! The demo's sessions working: a turn starts, replies, rests and ends.
use super::data::{self, DemoTurn, Kinds, Plan};
use super::{
    agent_path, folder, jsonl, path, plan, reply, source_of, turn_lines, DemoSessions, Ending,
};
use crate::{iso, AgentHudCore};
use serde_json::{json, Value};

impl AgentHudCore {
    /// A demo session starts a turn.
    pub fn demo_work(
        &mut self,
        id: &str,
        now: i64,
        sessions: &mut DemoSessions,
    ) -> Result<(), String> {
        let plan = plan(id)?;
        if sessions.open.get(plan.id).copied().unwrap_or(false) {
            return Ok(());
        }
        self.demo_turn(plan, now, Ending::Open, sessions)
    }

    /// A demo session's turn ends with a reply, which raises the reminder.
    pub fn demo_reply(
        &mut self,
        id: &str,
        message: Option<&str>,
        now: i64,
        sessions: &mut DemoSessions,
    ) -> Result<(), String> {
        let plan = plan(id)?;
        self.demo_end(plan, now, Ending::Reply(message), sessions)
    }

    /// A demo session stops with nothing to say.
    pub fn demo_rest(
        &mut self,
        id: &str,
        now: i64,
        sessions: &mut DemoSessions,
    ) -> Result<(), String> {
        let plan = plan(id)?;
        self.demo_end(plan, now, Ending::Interrupted, sessions)
    }

    fn demo_turn(
        &mut self,
        plan: &'static Plan,
        now: i64,
        ending: Ending,
        sessions: &mut DemoSessions,
    ) -> Result<(), String> {
        let index = sessions.turns.get(plan.id).copied().unwrap_or(0);
        sessions.turns.insert(plan.id, index + 1);
        sessions.open.insert(plan.id, ending == Ending::Open);
        let own = Kinds {
            cache_write: 6_000,
            input: 900,
            reasoning: 800,
            output: 1_200,
            cache_read: 180_000,
        };
        let mut totals = sessions.totals.get(plan.id).copied().unwrap_or_default();
        let mut seq = sessions.seq.get(plan.id).copied().unwrap_or(0);
        let turn = DemoTurn {
            start: now,
            end: now + 1_000,
            calls: 2,
            times: data::spread(now, now + 1_000, 2),
            own,
            agents: Vec::new(),
            compacted: false,
        };
        let lines = turn_lines(plan, index, &turn, ending, &mut totals, &mut seq);
        sessions.totals.insert(plan.id, totals);
        sessions.seq.insert(plan.id, seq);
        self.read_demo(
            path(plan.source, plan.id),
            source_of(plan),
            &jsonl(lines),
            false,
        )
    }

    fn demo_end(
        &mut self,
        plan: &'static Plan,
        now: i64,
        ending: Ending,
        sessions: &mut DemoSessions,
    ) -> Result<(), String> {
        if !sessions.open.get(plan.id).copied().unwrap_or(false) {
            // A session that was not working first starts a turn to end.
            self.demo_turn(plan, now - 2_000, Ending::Open, sessions)?;
        }
        sessions.open.insert(plan.id, false);
        let index = sessions
            .turns
            .get(plan.id)
            .copied()
            .unwrap_or(1)
            .saturating_sub(1);
        let dir = folder(plan.project);
        let text = match ending {
            Ending::Reply(Some(message)) => message.to_owned(),
            _ => reply(plan),
        };
        let mut seq = sessions.seq.get(plan.id).copied().unwrap_or(0);
        // A turn ends once the sub-agents it waits on report back, or are
        // stopped with it.
        let mut lines = Vec::new();
        for agent in sessions.agents.remove(plan.id).unwrap_or_default() {
            let line = |kind: &str, at: i64, message: Value| json!({"type":kind,"isSidechain":true,"agentId":agent.id,"sessionId":plan.id,"cwd":dir,"uuid":format!("{}-{kind}-{at}",agent.id),"timestamp":iso(at),"message":message});
            let last = agent.calls.len().saturating_sub(1);
            let tail = if matches!(ending, Ending::Reply(_)) {
                vec![
                    line(
                        "user",
                        now - 1_600,
                        json!({"role":"user","content":[{"type":"tool_result","tool_use_id":format!("{}-t{last}",agent.id),"content":"ok"}]}),
                    ),
                    line(
                        "assistant",
                        now - 1_200,
                        json!({"id":format!("msg-{}-end-{now}",agent.id),"model":agent.model,"role":"assistant","content":[{"type":"text","text":format!("Done: {}.",agent.task.description)}],"stop_reason":"end_turn","usage":{"input_tokens":4,"output_tokens":640,"cache_read_input_tokens":42_000,"cache_creation_input_tokens":1_500}}),
                    ),
                ]
            } else {
                vec![line(
                    "user",
                    now - 500,
                    json!({"role":"user","content":"[Request interrupted by user]"}),
                )]
            };
            self.read_demo(
                agent_path(plan.id, &agent.id),
                "claude",
                &jsonl(tail),
                false,
            )?;
            if matches!(ending, Ending::Reply(_)) {
                lines.push(json!({"type":"user","uuid":format!("{}-r{}",plan.id,agent.id),"sessionId":plan.id,"cwd":dir,"timestamp":iso(now - 800),"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":agent.call,"content":[{"type":"text","text":format!("Done: {}.",agent.task.description)}]}]},"toolUseResult":{"status":"completed","agentId":agent.id}}));
            }
        }
        let line = match (plan.source, ending) {
            ("claude", Ending::Reply(_)) => {
                json!({"type":"assistant","sessionId":plan.id,"cwd":dir,"timestamp":iso(now),"message":{"id":format!("msg-{}-{index}-end-{now}",plan.id),"model":plan.model,"role":"assistant","content":[{"type":"text","text":text}],"stop_reason":"end_turn","usage":{"input_tokens":300,"output_tokens":420,"cache_read_input_tokens":90_000,"cache_creation_input_tokens":2_000}}})
            }
            ("claude", _) => {
                json!({"type":"user","uuid":format!("{}-x{index}-{now}",plan.id),"sessionId":plan.id,"cwd":dir,"timestamp":iso(now),"message":{"role":"user","content":"[Request interrupted by user]"}})
            }
            ("codex", Ending::Reply(_)) => {
                json!({"timestamp":iso(now),"type":"event_msg","payload":{"type":"task_complete","turn_id":format!("{}-t{index}",plan.id),"last_agent_message":text}})
            }
            ("codex", _) => {
                json!({"timestamp":iso(now),"type":"event_msg","payload":{"type":"turn_aborted","turn_id":format!("{}-t{index}",plan.id),"reason":"interrupted"}})
            }
            (_, ending) => {
                seq += 1;
                let kind = if matches!(ending, Ending::Reply(_)) {
                    "completed"
                } else {
                    "interrupted"
                };
                json!({"type":"turn/end","time":iso(now),"seq":seq,"data":{"turn":index+1,"reason":{"kind":kind}}})
            }
        };
        sessions.seq.insert(plan.id, seq);
        lines.push(line);
        self.read_demo(
            path(plan.source, plan.id),
            source_of(plan),
            &jsonl(lines),
            false,
        )
    }
}
