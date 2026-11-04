//! Codex's live activity reducer: turns, calls and outcomes from its rollout lines.
use crate::transcript::live::{
    last_paragraph, newer, timestamp, Activity, Call, LiveLog, Turn, Work,
};
use serde_json::Value;

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
                let said = match event {
                    "item_completed" => matches!(
                        payload["item"]["type"].as_str(),
                        Some("UserMessage" | "AgentMessage")
                    ),
                    _ => payload["message"]
                        .as_str()
                        .is_some_and(|words| !words.trim().is_empty()),
                };
                if said {
                    newer(&mut self.last_message_at, at);
                }
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
