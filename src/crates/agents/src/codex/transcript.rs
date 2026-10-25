//! Codex transcript usage and live activity reducer.
use crate::{
    number,
    transcript::items::{self, Items, Line, LineItem, ToolCall},
    transcript::live::{last_paragraph, newer, timestamp, Activity, Call, LiveLog, Turn, Work},
    transcript::metering,
    Log, Usage,
};
use serde_json::{json, Value};

impl Log {
    pub fn meter_codex_call(&mut self, value: &Value) {
        // A reply is timed from the newest request-side line (a prompt, a tool
        // result) to its `token_usage_record`, written as the reply finishes.
        // The token count that closes the call comes after the tool results,
        // so its own time holds the tool runs. A fork's copied history times
        // nothing.
        if !self.live.inherited(value) {
            if let Some(at) = value["timestamp"].as_str().and_then(metering::date_ms) {
                if value["type"] == "token_usage_record" {
                    let took = self.call_timing.since_request(0, at);
                    self.call_timing.replies.push(took);
                    self.call_timing.request_at[0] = Some(at);
                    return;
                }
                if codex_request_side(value) {
                    self.call_timing.request_at[0] = Some(at);
                }
            }
        }
        // Codex writes a call's tool requests before the token count that
        // closes it (agent-hud-native-open CallTools.codex); a fork's copied history
        // asks for nothing.
        if value["type"] == "response_item" && !self.live.inherited(value) {
            let payload = &value["payload"];
            let tool = match payload["type"].as_str() {
                Some("function_call" | "custom_tool_call") => payload["name"].as_str(),
                Some("local_shell_call") => Some("shell"),
                Some("web_search_call") => Some("web_search"),
                _ => None,
            };
            if let Some(tool) = tool {
                self.pending_tools.push(tool.to_owned());
            }
            return;
        }
        if value["type"] != "event_msg" || value["payload"]["type"] != "token_count" {
            return;
        }
        let info = &value["payload"]["info"];
        let total = &info["total_token_usage"];
        if !total["input_tokens"].is_u64() || !total["output_tokens"].is_u64() {
            return;
        }
        let read = |value: &Value| Usage {
            input: number(value, "input_tokens"),
            output: number(value, "output_tokens"),
            read: number(value, "cached_input_tokens"),
            write: number(value, "cache_write_input_tokens"),
            reasoning: number(value, "reasoning_output_tokens"),
        };
        let next = read(total);
        let previous = self.meter_codex.replace(next);
        let reset = previous
            .is_some_and(|p| next.input < p.input || next.output < p.output || next.read < p.read);
        let delta = if let Some(previous) = previous.filter(|_| !reset) {
            Usage {
                input: next.input.saturating_sub(previous.input),
                output: next.output.saturating_sub(previous.output),
                read: next.read.saturating_sub(previous.read),
                write: next.write.saturating_sub(previous.write),
                reasoning: next.reasoning.saturating_sub(previous.reasoning),
            }
        } else if info["last_token_usage"].is_object() {
            read(&info["last_token_usage"])
        } else {
            next
        };
        // A count that does not move the totals closes no call.
        if delta.input == 0 && delta.output == 0 {
            return;
        }
        let tools = std::mem::take(&mut self.pending_tools);
        // One reply per count. A count that closes two replies (under 1 % of
        // counts) is not timed: which one it counts cannot be told.
        let replies = std::mem::take(&mut self.call_timing.replies);
        let duration_ms = match replies.as_slice() {
            [took] => *took,
            _ => None,
        };
        if self.live.inherited(value) {
            return;
        }
        let Some(at) = value["timestamp"].as_str().and_then(metering::date_ms) else {
            return;
        };
        let event = metering::Event {
            key: metering::event_key(&format!("o{}", self.meter_line_start)),
            model: if self.model == "Unknown" {
                String::new()
            } else {
                self.model.clone()
            },
            at,
            usage: Usage {
                input: delta.input.saturating_sub(delta.read + delta.write),
                output: delta.output.saturating_sub(delta.reasoning),
                read: delta.read,
                write: delta.write,
                reasoning: delta.reasoning,
            },
            context_window: info["model_context_window"]
                .as_u64()
                .filter(|window| *window > 0),
            tools,
            duration_ms,
        };
        let counted = event.usage;
        self.usage.add(counted);
        let index = self
            .turns
            .back()
            .and_then(|entry| entry["index"].as_u64())
            .unwrap_or(0)
            + 1;
        self.push_turn(json!({"id":format!("{}:{index}",self.id),"index":index,"timestamp":value["timestamp"],"input":counted.input,"output":counted.output+counted.reasoning,"cache":counted.read+counted.write,"context":null,"message":null}));
        self.live.note_usage(value);
        self.meter_events.insert(event.key.clone(), event);
    }

    pub fn parse_codex(&mut self, value: &Value) {
        let payload = &value["payload"];
        // A turn starts at its task_started event and a compacted record marks a
        // compaction; a fork's copied history marks neither.
        let at = || value["timestamp"].as_str().and_then(metering::date_ms);
        if !self.live.inherited(value) {
            if value["type"] == "compacted" {
                self.mark(at(), metering::MarkKind::Compaction);
            } else if value["type"] == "event_msg" && payload["type"] == "task_started" {
                self.mark(at(), metering::MarkKind::Prompt);
            }
        }
        match value["type"].as_str() {
            Some("session_meta") => {
                if let Some(id) = payload["id"].as_str() {
                    self.observed = true;
                    let owned = self.live.session_id.as_deref().unwrap_or(id);
                    self.id = owned.into();
                }
                if payload["id"].as_str() == self.live.session_id.as_deref() {
                    self.cwd = payload["cwd"].as_str().map(str::to_owned);
                    self.client = crate::naming::clients::codex_client(
                        payload["source"].as_str(),
                        payload["originator"].as_str(),
                    );
                    // A spawned agent is named by its path, else its nickname
                    // (agent-hud-native-open CodexTranscript.agentPath, agentName).
                    let spawn = &payload["source"]["subagent"]["thread_spawn"];
                    fn nonempty(value: &Value) -> Option<&str> {
                        value.as_str().filter(|text| !text.trim().is_empty())
                    }
                    let path = nonempty(&spawn["agent_path"]).map(|path| {
                        path.trim_end_matches('/')
                            .rsplit('/')
                            .next()
                            .unwrap_or(path)
                            .to_owned()
                    });
                    let nickname = nonempty(&spawn["agent_nickname"])
                        .or_else(|| nonempty(&payload["agent_nickname"]))
                        .map(str::to_owned);
                    if let Some(name) = path.or(nickname) {
                        self.naming.agent(&name);
                    }
                }
            }
            Some("turn_context") => {
                if let Some(model) = payload["model"].as_str() {
                    self.model = model.into();
                }
            }
            Some("event_msg") if payload["type"] == "user_message" => {
                if let Some(text) = payload["message"].as_str() {
                    self.title_from(text);
                }
            }
            // Current rollouts record a prompt only as a completed UserMessage
            // item of text and image parts; its first text part that gives a
            // title names the session.
            Some("event_msg")
                if payload["type"] == "item_completed"
                    && payload["item"]["type"] == "UserMessage" =>
            {
                for part in payload["item"]["content"].as_array().into_iter().flatten() {
                    if part["type"] == "text" {
                        if let Some(text) = part["text"].as_str() {
                            self.title_from(text);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl LiveLog {
    pub fn codex(&mut self, value: &Value) {
        let payload = &value["payload"];
        let kind = value["type"].as_str().unwrap_or("");
        let Some(at) = timestamp(&value["timestamp"]) else {
            return;
        };
        if kind == "session_meta" {
            if self.session_id.is_none() {
                self.session_id = payload["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned);
                self.started_at = timestamp(&payload["timestamp"]).or(Some(at));
                self.inherited_through = payload["subagent_history_start_ordinal"].as_u64();
                let subagent = &payload["source"]["subagent"];
                self.is_subagent = !subagent.is_null();
                self.is_internal = subagent["other"] == "guardian";
                self.parent_id = payload["parent_thread_id"]
                    .as_str()
                    .or(subagent["thread_spawn"]["parent_thread_id"].as_str())
                    .map(str::to_owned);
            }
            return;
        }
        if kind == "turn_context" || self.inherited(value) {
            return;
        }
        if kind == "response_item" {
            if !self
                .turns
                .back()
                .is_some_and(|turn| turn.running && at >= turn.started.unwrap_or(i64::MIN))
            {
                return;
            }
            let item = payload["type"].as_str().unwrap_or("");
            let is_call = matches!(
                item,
                "function_call" | "custom_tool_call" | "local_shell_call"
            );
            let is_output = matches!(item, "function_call_output" | "custom_tool_call_output");
            if !(is_call || is_output) {
                return;
            }
            let Some(id) = payload["call_id"].as_str() else {
                return;
            };
            self.touch_running(at);
            let activity = self.activity.get_or_insert_with(Activity::default);
            if is_call {
                let name = if item == "local_shell_call" {
                    "shell"
                } else {
                    payload["name"].as_str().unwrap_or("")
                };
                let work = codex_work(name, payload["namespace"].as_str());
                let request = (work == Some(Work::Question)).then(|| "question".into());
                activity.open(Call {
                    id: id.into(),
                    work,
                    at,
                    request,
                });
            } else {
                activity.close(id, at, false);
            }
            newer(&mut self.last_event, at);
            return;
        }
        if kind != "event_msg" {
            return;
        }
        let event = payload["type"].as_str().unwrap_or("");
        match event {
            "task_started" => {
                let id = payload["turn_id"].as_str().map(str::to_owned);
                let duplicate = self.turns.iter().any(|turn| {
                    turn.id == id && (id.is_some() || turn.running && turn.started == Some(at))
                });
                if !duplicate
                    && at
                        >= self
                            .turns
                            .back()
                            .and_then(|turn| turn.started)
                            .unwrap_or(i64::MIN)
                {
                    self.push(Turn {
                        id,
                        started: Some(at),
                        observed: at,
                        running: true,
                        outcome: None,
                        limited_until: None,
                        message: None,
                    });
                    self.final_answer = None;
                    self.activity.get_or_insert_with(Activity::default).start();
                }
                newer(&mut self.last_event, at);
            }
            "task_complete" => {
                let error = &payload["error"];
                let carries_error = !error.is_null() && error.as_str() != Some("");
                let outcome = if carries_error {
                    codex_outcome(error)
                } else {
                    "completed"
                };
                let turn_id = payload["turn_id"].as_str();
                self.terminal(turn_id, at, Some(outcome), None);
                // The answer is the one Codex reports with the completion,
                // else the turn's explicitly final message (agent-hud-native-open
                // CodexTranscript); a turn with neither keeps its last words.
                if let Some(msg) = payload["last_agent_message"]
                    .as_str()
                    .and_then(last_paragraph)
                    .or_else(|| self.final_answer.take())
                {
                    if let Some(turn) = self
                        .turns
                        .iter_mut()
                        .rev()
                        .find(|turn| turn_id.is_none() || turn.id.as_deref() == turn_id)
                    {
                        turn.message = Some(msg);
                    }
                }
                newer(&mut self.last_event, at);
            }
            "turn_aborted" => {
                self.terminal(
                    payload["turn_id"].as_str(),
                    at,
                    (payload["reason"] == "interrupted").then_some("interrupted"),
                    None,
                );
                newer(&mut self.last_event, at);
            }
            "token_count" => {}
            "user_message" | "agent_message" | "item_completed" => {
                // Every message the agent writes, its progress commentary too,
                // is the running turn's message (agent-hud-native-open
                // CodexTranscript); a final answer is also kept for its
                // completion.
                if event == "agent_message" {
                    if let Some(msg) = payload["message"].as_str().and_then(last_paragraph) {
                        if let Some(turn) = self.turns.back_mut().filter(|turn| turn.running) {
                            turn.message = Some(msg.clone());
                        }
                        if matches!(payload["phase"].as_str(), None | Some("final_answer"))
                            || payload["channel"] == "final"
                        {
                            self.final_answer = Some(msg);
                        }
                    }
                }
                if event == "item_completed" {
                    if payload["item"]["type"] == "AgentMessage"
                        && matches!(
                            payload["item"]["phase"].as_str(),
                            None | Some("final_answer")
                        )
                    {
                        let text = payload["item"]["content"]
                            .as_array()
                            .map(|items| {
                                items
                                    .iter()
                                    .filter_map(|item| item["text"].as_str())
                                    .collect::<Vec<_>>()
                                    .join("\n\n")
                            })
                            .unwrap_or_default();
                        if let Some(msg) = last_paragraph(&text) {
                            if let Some(turn) = self.turns.back_mut().filter(|turn| turn.running) {
                                turn.message = Some(msg.clone());
                            }
                            self.final_answer = Some(msg);
                        }
                    }
                    let work = match payload["item"]["type"].as_str() {
                        Some("FileChange") => Some(Work::Edit),
                        Some("ImageView") => Some(Work::Read),
                        _ => None,
                    };
                    if let Some(work) =
                        work.filter(|_| self.turns.back().is_some_and(|turn| turn.running))
                    {
                        let activity = self.activity.get_or_insert_with(Activity::default);
                        activity.latest = Some(work);
                        newer(&mut activity.tool_at, at);
                    }
                }
                self.touch_running(at);
            }
            _ => self.touch_running(at),
        }
    }
}

/// A line written before a reply's request goes out: a turn's start, the
/// person's message, or a tool's result.
fn codex_request_side(value: &Value) -> bool {
    let payload = &value["payload"];
    match (value["type"].as_str(), payload["type"].as_str()) {
        (Some("event_msg"), Some("task_started" | "user_message")) => true,
        (Some("event_msg"), Some("item_completed")) => payload["item"]["type"] == "UserMessage",
        (
            Some("response_item"),
            Some("function_call_output" | "custom_tool_call_output" | "local_shell_call_output"),
        ) => true,
        (Some("response_item"), Some("message")) => payload["role"] == "user",
        _ => false,
    }
}

fn codex_outcome(error: &Value) -> &'static str {
    let info = &error["codex_error_info"];
    if info == "usage_limit_exceeded"
        || info
            .as_object()
            .is_some_and(|info| info.contains_key("usage_limit_exceeded"))
    {
        "limited"
    } else {
        "failed"
    }
}

fn codex_work(name: &str, namespace: Option<&str>) -> Option<Work> {
    let name = name.strip_prefix("functions.").unwrap_or(name);
    match name {
        "exec" | "shell" | "shell_command" | "exec_command" | "write_stdin" => Some(Work::Shell),
        "js" | "js_reset" => Some(Work::Command),
        "apply_patch" => Some(Work::Edit),
        "view_image" => Some(Work::Read),
        "request_user_input_async" | "request_user_input" => Some(Work::Question),
        "spawn_agent" | "wait_agent" | "send_message" | "followup_task" | "list_agents"
        | "wait" | "interrupt_agent" => Some(Work::Agents),
        _ if name.starts_with("mcp__") || namespace.is_some_and(|ns| ns.starts_with("mcp__")) => {
            Some(Work::Command)
        }
        _ => None,
    }
}

/// Whether a Codex user item holds the user's own words rather than context
/// the client hands the model. Desktop rollouts name each part's role
/// (`user.text`, `agents_md.instructions`, `environments.environment_context`,
/// `additional_content.codex_apps_open_page`, `generic.turn_aborted`, …), and no
/// item mixes the two; older rollouts name none, and their context blocks are
/// told by how they open. Any other line is not Codex's and is left as it is.
pub fn user_words(value: &Value) -> bool {
    let payload = &value["payload"];
    if value["type"] != "response_item" || payload["type"] != "message" {
        return true;
    }
    let kinds = &payload["internal_chat_message_metadata_passthrough"]["content_item_kinds"];
    if let Some(kinds) = kinds.as_array().filter(|kinds| !kinds.is_empty()) {
        return kinds
            .iter()
            .filter_map(Value::as_str)
            .any(|kind| kind.starts_with("user."));
    }
    let text = payload["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|part| part["type"] == "input_text")
        .filter_map(|part| part["text"].as_str())
        .collect::<String>();
    let text = text.trim_start();
    !CONTEXT_OPENINGS
        .iter()
        .any(|opening| text.starts_with(opening))
}

/// How the context blocks of rollouts without part roles begin.
const CONTEXT_OPENINGS: [&str; 7] = [
    "<environment_context>",
    "# AGENTS.md instructions",
    "<user_instructions>",
    "<turn_aborted>",
    "<external_codex_apps_open_page>",
    "<recommended_plugins>",
    "<skill>",
];

/// The rows of a Codex `response_item` line: a message's text, a tool call
/// or what it returned. Other lines are not response items.
pub fn response_items(value: &Value, offset: u64) -> Option<Vec<LineItem>> {
    if value["type"] != "response_item" {
        return None;
    }
    let payload = &value["payload"];
    let line = Line::of(value, user_words(value));
    let id = payload["id"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("o{offset}"));
    let mut rows = Vec::new();
    match payload["type"].as_str() {
        Some("message") => {
            let text = payload["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|v| matches!(v["type"].as_str(), Some("input_text" | "output_text")))
                .filter_map(|v| v["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if !text.is_empty() {
                let role = payload["role"].as_str().unwrap_or("assistant");
                rows.push(line.item(id, role, json!(text), None, &Value::Null));
            }
        }
        Some("function_call" | "custom_tool_call") => rows.push(line.item(
            id,
            "assistant",
            Value::Null,
            Some(ToolCall::new(
                &payload["call_id"],
                &payload["name"],
                payload.get("arguments").or_else(|| payload.get("input")),
            )),
            &payload["call_id"],
        )),
        Some("function_call_output" | "custom_tool_call_output") => rows.push(line.item(
            id,
            "tool",
            payload["output"].clone(),
            None,
            &payload["call_id"],
        )),
        _ => {}
    }
    Some(rows)
}

/// A Codex page shows its response items, and the bare messages of rollouts
/// written before items were wrapped; `event_msg` lines and the rest show
/// nothing.
pub static ITEMS: Items = Items {
    read: |value, offset| {
        response_items(value, offset)
            .or_else(|| {
                (value["type"] == "message" && !value["message"].is_object())
                    .then(|| items::message(value, value, offset, user_words(value)))
            })
            .unwrap_or_default()
    },
};

/// Codex names its turn on most lines (`turn_id`) and says once when it began
/// (`task_started`).
pub static TURNS: crate::transcript::turns::Turns = crate::transcript::turns::Turns {
    start: |value| {
        let payload = &value["payload"];
        let id = payload["turn_id"].as_str()?;
        let begins = value["type"] == "event_msg" && payload["type"] == "task_started";
        Some(crate::transcript::turns::TurnStart {
            turn: Some(id.into()),
            started: begins.then(|| value.get("timestamp").cloned()).flatten(),
            whole: false,
        })
    },
    may_name: |line| crate::transcript::turns::has(line, b"\"turn_id\""),
};
