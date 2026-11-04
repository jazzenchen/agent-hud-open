//! Claude Code's live activity reducer: turns, calls and questions from its log lines.
use super::desktop_ask::claude_desktop_ask;
use super::project_origin::{managed_origin, ClaudeProjectOrigin};
use super::transcript::{claude_prompt, claude_turn, narration};
use crate::transcript::live::{
    last_paragraph, newer, timestamp, Activity, Call, LiveLog, Turn, Work,
};
use serde_json::Value;

impl LiveLog {
    pub fn claude(&mut self, value: &Value) {
        if let Some(origin) = managed_origin(value) {
            let next = origin.unwrap_or(ClaudeProjectOrigin::Unknown);
            self.claude_project_origin = Some(match self.claude_project_origin.take() {
                None => next,
                Some(ClaudeProjectOrigin::Unknown) => ClaudeProjectOrigin::Unknown,
                Some(existing) if existing == next => existing,
                Some(_) => ClaudeProjectOrigin::Unknown,
            });
        }
        if let Some(id) = value["sessionId"].as_str() {
            if self.is_subagent {
                self.parent_id.get_or_insert_with(|| id.into());
            } else {
                self.session_id.get_or_insert_with(|| id.into());
            }
        }
        let Some(at) = timestamp(&value["timestamp"]) else {
            return;
        };
        self.started_at = Some(self.started_at.unwrap_or(at).min(at));
        newer(&mut self.last_event, at);
        if !self.is_subagent && value["isSidechain"] == true {
            return;
        }
        let message = &value["message"];
        let role = message["role"]
            .as_str()
            .or(value["type"].as_str())
            .unwrap_or("");
        let content = &message["content"];
        let blocks = content.as_array().map(Vec::as_slice).unwrap_or_default();
        let text = content.as_str().or_else(|| {
            blocks
                .iter()
                .find(|block| block["type"] == "text")
                .and_then(|block| block["text"].as_str())
        });
        let results = blocks
            .iter()
            .filter(|block| block["type"] == "tool_result")
            .collect::<Vec<_>>();
        let prompt = claude_prompt(value);
        let interrupted = text.is_some_and(|text| text.starts_with("[Request interrupted"));
        let api_error = (role == "assistant" && value["isApiErrorMessage"] == true)
            .then(|| value["error"].as_str())
            .flatten();
        let assistant = role == "assistant" && message["model"] != "<synthetic>";
        if (prompt && !interrupted)
            || (assistant && !assistant_words(content, blocks).trim().is_empty())
        {
            newer(&mut self.last_message_at, at);
        }
        let completed = assistant
            && matches!(
                message["stop_reason"].as_str(),
                Some("end_turn" | "stop_sequence")
            );
        let ends_turn = !results.is_empty() && value["mcpMeta"]["_meta"]["claude/endTurn"] == true;
        let stop = value["type"] == "system"
            && matches!(
                value["subtype"].as_str(),
                Some("stop_hook_summary" | "turn_duration")
            );
        let is_newer = at
            >= self
                .turns
                .back()
                .map(|turn| turn.observed)
                .unwrap_or(i64::MIN);
        if prompt && is_newer {
            let start = if interrupted {
                self.turns.back().and_then(|turn| turn.started)
            } else {
                Some(at)
            };
            let id = if interrupted {
                self.turns.back().and_then(|turn| turn.id.clone())
            } else {
                claude_turn(value)
                    .map(str::to_owned)
                    .or_else(|| Some(format!("completed-{at}")))
            };
            self.push(Turn {
                id,
                started: start,
                observed: at,
                running: !interrupted,
                outcome: interrupted.then(|| "interrupted".into()),
                limited_until: None,
                message: None,
            });
            self.activity.get_or_insert_with(Activity::default).start();
        } else if let Some(error) =
            api_error.filter(|_| is_newer && self.turns.back().is_none_or(|turn| turn.running))
        {
            let outcome = if matches!(error, "rate_limit" | "billing_error") {
                "limited"
            } else {
                "failed"
            };
            // Seconds, as Claude Code writes them; a value this large can only
            // be milliseconds (agent-hud-native-open FastTranscriptParser).
            let until = value["quotaLimits"]["resetsAt"]
                .as_f64()
                .filter(|at| at.is_finite())
                .map(|at| if at > 100_000_000_000.0 { at } else { at * 1000.0 } as i64);
            self.claude_terminal(at, Some(outcome), until);
        } else if completed && is_newer {
            let assistant_text = assistant_words(content, blocks);
            if let Some(para) = last_paragraph(&assistant_text) {
                if let Some(turn) = self.turns.back_mut() {
                    turn.message = Some(para);
                }
            }
            let completion = message["id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| at.to_string());
            if self.is_subagent || !self.completed_responses.contains(&completion) {
                if self.completed_responses.len() == 32 {
                    self.completed_responses.pop_front();
                }
                self.completed_responses.push_back(completion);
                self.claude_terminal(at, Some("completed"), None);
            }
        } else if assistant && is_newer {
            let assistant_text = assistant_words(content, blocks);
            let para = last_paragraph(&assistant_text);
            let structured = self.is_subagent
                && blocks.iter().any(|block| {
                    block["type"] == "tool_use" && block["name"] == "StructuredOutput"
                });
            if structured {
                if let Some(para) = para {
                    if let Some(turn) = self.turns.back_mut() {
                        turn.message = Some(para);
                    }
                }
                self.claude_terminal(at, Some("completed"), None);
            } else if self.turns.is_empty() {
                self.push(Turn {
                    id: Some(format!("completed-{at}")),
                    started: None,
                    observed: at,
                    running: true,
                    outcome: None,
                    limited_until: None,
                    message: para,
                });
            } else if let Some(turn) = self
                .turns
                .back_mut()
                .filter(|turn| !turn.running && at > turn.observed)
            {
                turn.id = Some(match turn.id.as_deref() {
                    Some(turn) => format!("{turn}-{at}"),
                    None => format!("completed-{at}"),
                });
                turn.running = true;
                turn.outcome = None;
                turn.limited_until = None;
                if para.is_some() {
                    turn.message = para;
                }
            } else if let Some(para) = para {
                if let Some(turn) = self.turns.back_mut() {
                    turn.message = Some(para);
                }
            }
        } else if (ends_turn || stop)
            && is_newer
            && self.turns.back().is_some_and(|turn| turn.running)
        {
            self.claude_terminal(at, None, None);
        }
        if role == "user" || role == "assistant" {
            self.touch_running(at);
        }
        if self.is_subagent {
            return;
        }
        let activity = self.activity.get_or_insert_with(Activity::default);
        if prompt && is_newer {
            activity.line = Some(if interrupted { "context" } else { "prompt" }.into());
        } else if !results.is_empty() {
            for result in results {
                if let Some(id) = result["tool_use_id"].as_str() {
                    activity.close(id, at, result["is_error"] == true);
                }
            }
            if ends_turn {
                activity.line = Some("answer".into());
            }
        } else if assistant && blocks.iter().any(|block| block["type"] == "tool_use") {
            let response_id = message["id"].as_str().map(str::to_owned);
            for call in blocks.iter().filter(|block| block["type"] == "tool_use") {
                let Some(id) = call["id"].as_str() else {
                    continue;
                };
                let name = call["name"].as_str().unwrap_or("");
                let request = match name {
                    "AskUserQuestion" => Some("question"),
                    _ => None,
                };
                let call_item = Call {
                    id: id.into(),
                    work: claude_work(name),
                    at,
                    request: request.map(str::to_owned),
                };
                let input = call_input(value, id);
                if let Some(ask) = claude_desktop_ask(name, input, id, response_id.clone(), at) {
                    if ask.kind == "next_step" {
                        activity.standing_question = Some(Call {
                            request: Some("question".into()),
                            ..call_item.clone()
                        });
                        activity.open(call_item);
                    } else {
                        activity.open(Call {
                            request: Some("question".into()),
                            ..call_item
                        });
                    }
                    activity.desktop_ask = Some(ask);
                    continue;
                }
                if activity
                    .desktop_ask
                    .as_ref()
                    .is_some_and(|asked| asked.response.is_none() || asked.response != response_id)
                {
                    activity.desktop_ask = None;
                    activity.standing_question = None;
                }
                activity.open(call_item);
            }
        } else if api_error.is_some() {
            activity.line = Some("error".into());
        } else if assistant {
            activity.line = Some(
                if completed {
                    "answer"
                } else if blocks.iter().any(|block| {
                    matches!(
                        block["type"].as_str(),
                        Some("thinking" | "redacted_thinking")
                    ) && narration(block).is_none()
                }) {
                    "thinking"
                } else {
                    "text"
                }
                .into(),
            );
        } else if role == "user" {
            activity.line = Some("context".into());
        }
        activity.retrying = value["type"] == "system"
            && value["subtype"] == "api_error"
            && value["retryAttempt"].as_u64().is_some();
    }

    fn claude_terminal(&mut self, at: i64, outcome: Option<&str>, until: Option<i64>) {
        let fallback = (outcome == Some("completed")).then(|| format!("completed-{at}"));
        if let Some(turn) = self.turns.back_mut() {
            if turn.id.is_none() {
                turn.id = fallback;
            }
            turn.running = false;
            turn.observed = at;
            turn.outcome = outcome.map(str::to_owned);
            turn.limited_until = until;
        } else {
            self.push(Turn {
                id: fallback,
                started: None,
                observed: at,
                running: false,
                outcome: outcome.map(str::to_owned),
                limited_until: until,
                message: None,
            });
        }
        if let Some(activity) = &mut self.activity {
            activity.end();
        }
    }
}

/// What the agent says on an assistant line: its text blocks, and the
/// narrations the desktop app shows as the agent's own words.
fn assistant_words(content: &Value, blocks: &[Value]) -> String {
    content.as_str().map(str::to_owned).unwrap_or_else(|| {
        blocks
            .iter()
            .filter_map(|block| {
                if block["type"] == "text" {
                    block["text"].as_str()
                } else {
                    narration(block)
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    })
}

fn call_input<'a>(value: &'a Value, id: &str) -> &'a Value {
    value["message"]["content"]
        .as_array()
        .and_then(|blocks| blocks.iter().find(|block| block["id"] == id))
        .map(|block| &block["input"])
        .unwrap_or(&Value::Null)
}

fn claude_work(name: &str) -> Option<Work> {
    match name {
        "Bash" | "BashOutput" | "KillShell" | "run_command" | "run_shell_command" | "exec" => {
            Some(Work::Shell)
        }
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" | "apply_patch" | "edit" | "replace"
        | "write_file" | "write" | "notebook_edit" => Some(Work::Edit),
        "Read"
        | "Glob"
        | "Grep"
        | "WebFetch"
        | "WebSearch"
        | "read_file"
        | "read"
        | "glob"
        | "grep_search"
        | "search_file_content"
        | "grep"
        | "web_fetch"
        | "webfetch"
        | "web_search" => Some(Work::Read),
        "Task" | "Agent" | "task" | "agent" => Some(Work::Agents),
        _ if name.starts_with("mcp__") => Some(Work::Command),
        _ => None,
    }
}
