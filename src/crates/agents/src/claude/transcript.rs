//! Claude Code transcript usage and live activity reducer.
use crate::{
    number,
    transcript::items::{self, Items, LineItem},
    transcript::live::{
        last_paragraph, newer, timestamp, Activity, Call, DesktopAsk, DesktopAskOption, LiveLog,
        Turn, Work,
    },
    transcript::metering,
    transcript::turns::{has, TurnStart, Turns},
    Log, RecentCall, Usage,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

/// A copy of what Claude Code sent to the model (2.1.299 and later write
/// `api-request-blob` lines, and `api-request…` lines about them). A copy
/// repeats the conversation's own lines with no id or time, so it is no
/// prompt and no message.
pub fn request_copy(value: &Value) -> bool {
    value["type"]
        .as_str()
        .is_some_and(|kind| kind.starts_with("api-request"))
}

/// The turn a Claude prompt line starts: the prompt id Claude Code writes on
/// the line and reports to hooks as `prompt_id`, so a hook's turn and the
/// log's turn are one; the line's own uuid where no prompt id is written.
pub fn claude_turn(value: &Value) -> Option<&str> {
    value["promptId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .or_else(|| value["uuid"].as_str())
}

/// The same prompt rule drives source phase and explicit transcript pages.
pub fn claude_prompt(value: &Value) -> bool {
    if request_copy(value) {
        return false;
    }
    let message = &value["message"];
    let role = message["role"].as_str().or_else(|| value["type"].as_str());
    let content = &message["content"];
    let parts = content.as_array().map(Vec::as_slice).unwrap_or_default();
    let text = content.as_str().or_else(|| {
        parts
            .iter()
            .find(|part| part["type"] == "text")
            .and_then(|part| part["text"].as_str())
    });
    role == Some("user")
        && value["isMeta"] != true
        && value["isCompactSummary"] != true
        && !parts.iter().any(|part| part["type"] == "tool_result")
        && !text
            .is_some_and(|s| s.starts_with("<command-name>") || s.starts_with("<local-command-"))
}

/// Claude Code's note that the user stopped a turn: a user message that starts
/// no turn of its own and is not the user's words.
pub fn interrupted(value: &Value) -> bool {
    let content = &value["message"]["content"];
    content
        .as_str()
        .is_some_and(|s| s.starts_with("[Request interrupted"))
        || content.as_array().is_some_and(|parts| {
            parts.iter().any(|part| {
                part["text"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("[Request interrupted"))
            })
        })
}

/// A Claude turn starts at the user's prompt; replies, tool calls and their
/// results name no turn, so a page that begins inside a turn looks back to it.
pub static TURNS: Turns = Turns {
    start: |value| {
        if !claude_prompt(value) || interrupted(value) {
            return None;
        }
        let turn = claude_turn(value)
            .or_else(|| value["id"].as_str())
            .map(str::to_owned);
        Some(TurnStart::whole(
            turn,
            value.get("timestamp").or_else(|| value.get("time")),
        ))
    },
    // A tool result is a user line too, and never a prompt.
    may_name: |line| has(line, b"\"user\"") && !has(line, b"\"tool_use_id\""),
};

/// The rows of a Claude message line: its prompt, reply, tool calls and
/// tool results. Lines with no message are not Claude's to show.
pub fn message_items(value: &Value, offset: u64) -> Option<Vec<LineItem>> {
    let item = value.get("message").filter(|item| item.is_object())?;
    let prompt = claude_prompt(value) && !interrupted(value);
    let mut items = items::message(value, item, offset, prompt);
    items.extend(narrations(value, item, offset));
    Some(items)
}

/// The agent's words the Claude desktop app logs as narration blocks (see
/// `narration`): text the user saw, kept in a thinking-shaped block.
fn narrations(value: &Value, item: &Value, offset: u64) -> Vec<LineItem> {
    let Some(blocks) = item["content"].as_array() else {
        return Vec::new();
    };
    let id = value["uuid"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("o{offset}"));
    let line = items::Line::of(value, false);
    blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| {
            let words = narration(block).filter(|words| !words.trim().is_empty())?;
            Some(line.item(
                format!("{id}:n{index}"),
                "assistant",
                json!(words),
                None,
                &Value::Null,
            ))
        })
        .collect()
}

/// Words the user typed while a turn ran, which Claude Code hands the model
/// at its next step: an attachment in that turn, not a prompt of its own.
pub fn queued_words(value: &Value, offset: u64) -> Option<LineItem> {
    let attachment = &value["attachment"];
    if value["type"] != "attachment" || attachment["type"] != "queued_command" {
        return None;
    }
    let words = attachment["prompt"]
        .as_str()
        .filter(|s| !s.trim().is_empty())?;
    let id = value["uuid"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("o{offset}"));
    Some(items::Line::of(value, true).item(
        id,
        "user",
        serde_json::json!(words),
        None,
        &Value::Null,
    ))
}

/// A Claude page shows its message lines and the words typed into a running
/// turn; copies of a request show nothing.
pub static ITEMS: Items = Items {
    read: |value, offset| {
        if request_copy(value) {
            return Vec::new();
        }
        if let Some(words) = queued_words(value, offset) {
            return vec![words];
        }
        message_items(value, offset).unwrap_or_default()
    },
};

pub fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

impl Log {
    pub fn parse_claude(&mut self, value: &Value) {
        if let Some(id) = value["sessionId"].as_str() {
            self.observed = true;
            if !self.live.is_subagent {
                let owned = self.live.session_id.as_deref().unwrap_or(id);
                self.id = owned.into();
            }
        }
        if let Some(cwd) = value["cwd"].as_str() {
            self.cwd = Some(cwd.into());
        }
        if let Some(entrypoint) = value["entrypoint"]
            .as_str()
            .filter(|entrypoint| !entrypoint.is_empty())
        {
            self.client = Some(entrypoint.into());
        }
        // Only the main log marks its turns: a prompt starts one (an
        // interruption, recorded as a user line, does not), and a compact
        // boundary marks a compaction (agent-hud-native-open TranscriptAccumulator).
        if !self.live.is_subagent && value["isSidechain"] != true {
            let at = value["timestamp"].as_str().and_then(metering::date_ms);
            if value["type"] == "system" && value["subtype"] == "compact_boundary" {
                self.mark(at, metering::MarkKind::Compaction);
            } else if claude_prompt(value) && !interruption(value) {
                let plugin = (value["origin"]["kind"] == "plugin")
                    .then(|| value["origin"]["name"].as_str())
                    .flatten();
                self.mark_prompt(at, plugin);
            }
        }
        // A response is timed from the newest user line of its chain (a
        // prompt or a tool result) to its own last line. Attachment lines
        // are no request time: Claude Code often writes them as the
        // response arrives.
        let chain = usize::from(value["isSidechain"] == true);
        let line_at = value["timestamp"].as_str().and_then(metering::date_ms);
        if value["type"] == "user" {
            if let Some(at) = line_at {
                self.call_timing.request_at[chain] = Some(at);
            }
        }
        // Claude Code's own order: the name given to the session, then the
        // title it generated, then the first prompt.
        match value["type"].as_str() {
            Some("custom-title") => self.naming.name(0, value["customTitle"].as_str()),
            Some("ai-title") => self.naming.name(1, value["aiTitle"].as_str()),
            Some("user") => {
                if let Some(text) = value["message"]["content"].as_str() {
                    self.title_from(text);
                }
            }
            _ => {}
        }
        if value["type"] != "assistant" {
            return;
        }
        let message = &value["message"];
        // The model a call ran on. A line Claude Code wrote in the model's
        // place (`<synthetic>`, after an API error) names none, which reads as
        // Unknown (agent-hud-native-open ClaudeModelMapper).
        let model = message["model"]
            .as_str()
            .filter(|model| !model.is_empty() && *model != "<synthetic>")
            .unwrap_or("")
            .to_owned();
        // Claude Code writes a response one content block per line, all under
        // the message's id; its tool requests join that call.
        let tools = message["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|block| block["type"] == "tool_use")
                    .filter_map(|block| block["name"].as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        // A response is keyed by its message id, else its request id, else the
        // line's place in the log, so a line without an id still counts once
        // (agent-hud-native-open TranscriptEvent.usageKey and the `o:` offset key).
        let id = match (message["id"].as_str(), value["requestId"].as_str()) {
            (Some(id), _) => id.to_owned(),
            (None, Some(request)) => format!("r:{request}"),
            (None, None) => format!("o:{}", self.meter_line_start),
        };
        let id = id.as_str();
        let usage = &message["usage"];
        if !usage.is_object() {
            self.extend_call(id, tools, line_at);
            return;
        }
        // The counters as the line writes them: output includes the thinking
        // tokens, kept apart as reasoning (`claude_counted`).
        let next = Usage {
            input: number(usage, "input_tokens"),
            output: number(usage, "output_tokens"),
            read: number(usage, "cache_read_input_tokens"),
            write: number(usage, "cache_creation_input_tokens"),
            reasoning: number(&usage["output_tokens_details"], "thinking_tokens"),
        };
        // Each counter keeps its largest value over the response's lines:
        // builds before about 2026-09 wrote the first block's line with the
        // output counted so far and only the last line with the whole count.
        let previous = self.messages.get(id).copied().unwrap_or_default();
        let next = previous.max(next);
        if previous == next {
            self.extend_call(id, tools, line_at);
        } else {
            let (counted, before) = (claude_counted(next), claude_counted(previous));
            // The session's model is the one that wrote the most output
            // (agent-hud-native-open TranscriptAccumulator.build, dominantAgentId).
            *self.claude_output.entry(model.clone()).or_default() +=
                next.output.saturating_sub(previous.output);
            self.model = self
                .claude_output
                .iter()
                .filter(|(model, _)| !model.is_empty())
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                .filter(|(_, output)| **output > 0)
                .map(|(model, _)| model.clone())
                .unwrap_or_else(|| "Unknown".into());
            if let Some(at) = line_at {
                let started = self.call_timing.request_at[chain];
                let recent = match self.recent_calls.iter().position(|call| call.id == id) {
                    Some(index) => &mut self.recent_calls[index],
                    None => {
                        if self.recent_calls.len() == RECENT_CALLS {
                            self.recent_calls.pop_front();
                        }
                        self.recent_calls.push_back(RecentCall {
                            id: id.into(),
                            at,
                            model: model.clone(),
                            tools: Vec::new(),
                            started,
                            ended: None,
                        });
                        self.recent_calls.back_mut().unwrap()
                    }
                };
                recent.at = at;
                recent.ended = Some(recent.ended.map_or(at, |ended| ended.max(at)));
                recent.model = model.clone();
                recent.tools.extend(tools);
                let tools = recent.tools.clone();
                let duration_ms = recent.took();
                let key = metering::event_key(&claude_event_key(id));
                self.meter_events.insert(
                    key.clone(),
                    metering::Event {
                        key,
                        model,
                        at,
                        usage: counted,
                        tools,
                        duration_ms,
                        ..Default::default()
                    },
                );
            }
            self.usage.add(Usage {
                input: counted.input - before.input,
                output: 0,
                read: counted.read - before.read,
                write: counted.write - before.write,
                reasoning: counted.reasoning - before.reasoning,
            });
            // Output without thinking can fall when a later line reports the
            // thinking apart; the response's whole output never does.
            self.usage.output = (self.usage.output + counted.output).saturating_sub(before.output);
            self.messages.insert(id.into(), next);
            if let Some(turn) = self.turns.iter_mut().find(|turn| turn["id"] == id) {
                let next_turn = json!({"id":id,"index":turn["index"],"timestamp":self.updated,"input":next.input,"output":next.output,"cache":next.read+next.write,"context":null,"message":null});
                let previous = std::mem::replace(turn, next_turn.clone());
                self.note_bucket(&previous, false);
                self.note_bucket(&next_turn, true);
            } else {
                self.push_turn(json!({"id":id,"index":self.messages.len()-1,"timestamp":self.updated,"input":next.input,"output":next.output,"cache":next.read+next.write,"context":null,"message":null}));
            }
        }
    }
}

/// A response's usage as the ledger counts it: Claude Code's output includes
/// the thinking tokens (`output_tokens_details.thinking_tokens`), which are
/// kept apart as reasoning, as Codex's are (agent-hud-native-open records them as the
/// event's reasoning tokens, part of its output).
fn claude_counted(raw: Usage) -> Usage {
    let reasoning = raw.reasoning.min(raw.output);
    Usage {
        output: raw.output - reasoning,
        reasoning,
        ..raw
    }
}

/// The ledger key of a response: `m:` and its message id, or the request-id
/// or offset key it already carries.
fn claude_event_key(id: &str) -> String {
    if id.starts_with("r:") || id.starts_with("o:") {
        id.to_owned()
    } else {
        format!("m:{id}")
    }
}

/// Responses whose later lines can still add tools to their call; Claude Code
/// writes one response's lines back to back.
const RECENT_CALLS: usize = 16;

/// An interruption, which Claude Code records as a user line of its own.
fn interruption(value: &Value) -> bool {
    let content = &value["message"]["content"];
    content
        .as_str()
        .or_else(|| {
            content.as_array().and_then(|parts| {
                parts
                    .iter()
                    .find(|part| part["type"] == "text")
                    .and_then(|part| part["text"].as_str())
            })
        })
        .is_some_and(|text| text.starts_with("[Request interrupted"))
}

impl Log {
    /// A later line of a recent response that leaves its counts alone: the
    /// tools it asks for join the call, and its time moves the call's end.
    fn extend_call(&mut self, id: &str, tools: Vec<String>, at: Option<i64>) {
        let Some(recent) = self.recent_calls.iter_mut().find(|call| call.id == id) else {
            return;
        };
        let ended = match (recent.ended, at) {
            (Some(ended), Some(at)) => Some(ended.max(at)),
            (ended, at) => ended.or(at),
        };
        if tools.is_empty() && ended == recent.ended {
            return;
        }
        recent.tools.extend(tools);
        recent.ended = ended;
        let Some(usage) = self.messages.get(id).copied().map(claude_counted) else {
            return;
        };
        let key = metering::event_key(&claude_event_key(id));
        self.meter_events.insert(
            key.clone(),
            metering::Event {
                key,
                model: recent.model.clone(),
                at: recent.at,
                usage,
                tools: recent.tools.clone(),
                duration_ms: recent.took(),
                ..Default::default()
            },
        );
    }
}

impl LiveLog {
    pub fn claude(&mut self, value: &Value) {
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
        let (id, started) = self
            .turns
            .back()
            .map(|turn| (turn.id.clone(), turn.started))
            .unwrap_or_default();
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
                id: id.or(fallback),
                started,
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

/// A thinking block that is a narration: what the agent says between its
/// calls when the Claude desktop app asks its session for updates
/// (`thinkingDisplay: "updates"`). The block's signature, base64, holds a
/// message whose field 2 holds a message whose field 1 holds a message whose
/// field 8 is the text `narration` (agent-hud-native-open ClaudeNarration, as Claude
/// desktop 2.42304 tells it). Any other thinking block is the agent's
/// reasoning.
pub fn narration(block: &Value) -> Option<&str> {
    if block["type"] != "thinking" {
        return None;
    }
    let signature = block["signature"].as_str()?;
    (signature_kind(signature).as_deref() == Some("narration"))
        .then(|| block["thinking"].as_str())
        .flatten()
}

/// The kind a thinking block's signature names, such as `narration`.
pub fn signature_kind(signature: &str) -> Option<String> {
    use base64::Engine;
    let padding = (4 - signature.len() % 4) % 4;
    let padded = format!("{signature}{}", "=".repeat(padding));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(padded)
        .ok()?;
    // The last length-delimited field of that number, in a message that reads
    // to its end.
    fn field(number: u32, message: &[u8]) -> Option<&[u8]> {
        let mut found = None;
        crate::protobuf::fields(message, |field| {
            if field.number == number && field.wire == 2 {
                found = Some(field.bytes()?);
            }
            Ok(())
        })
        .ok()?;
        found
    }
    let kind = field(8, field(1, field(2, &bytes)?)?)?;
    String::from_utf8(kind.to_vec()).ok()
}

fn call_input<'a>(value: &'a Value, id: &str) -> &'a Value {
    value["message"]["content"]
        .as_array()
        .and_then(|blocks| blocks.iter().find(|block| block["id"] == id))
        .map(|block| &block["input"])
        .unwrap_or(&Value::Null)
}

fn trimmed_str(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn is_default_ignorable(c: char) -> bool {
    matches!(
        c as u32,
        0x00AD
            | 0x034F
            | 0x061C
            | 0x115F..=0x1160
            | 0x17B4..=0x17B5
            | 0x180B..=0x180F
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x206F
            | 0x3164
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xFFA0
            | 0xFFF0..=0xFFF8
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0000..=0xE0FFF
    )
}

/// The words on the button of an app action the next-step box offers; None
/// for one this does not know.
fn app_action(action: &str) -> Option<&'static str> {
    Some(match action {
        "commit" => "Commit",
        "create_pr" => "Create PR",
        "merge" => "Merge",
        "fix_ci" => "Fix CI",
        "address_comments" => "Address comments",
        "resolve_conflicts" => "Resolve conflicts",
        "verify" => "Verify",
        "archive" => "Archive",
        _ => return None,
    })
}

/// Text as the app reads a button's: without control, format or line and
/// paragraph separator characters other than the joiners, nor the filler
/// characters it drops, its spaces collapsed (agent-hud-native-open
/// ClaudeDesktopAsks.shown; this app does not compose the text first).
fn shown(value: &Value) -> Option<String> {
    let text = value.as_str()?;
    let kept = text
        .chars()
        .filter(|&c| {
            if c == '\u{200C}' || c == '\u{200D}' {
                return true;
            }
            !(c.is_control()
                || is_format(c)
                || c == '\u{2028}'
                || c == '\u{2029}'
                || matches!(
                    c,
                    '\u{115F}'
                        | '\u{1160}'
                        | '\u{3164}'
                        | '\u{FFA0}'
                        | '\u{2800}'
                        | '\u{FFFC}'
                        | '\u{1D159}'
                        | '\u{13441}'
                        | '\u{13442}'
                        | '\u{16910}'
                ))
        })
        .collect::<String>();
    Some(kept.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Unicode's format characters (general category Cf).
fn is_format(c: char) -> bool {
    matches!(
        c as u32,
        0x00AD
            | 0x0600..=0x0605
            | 0x061C
            | 0x06DD
            | 0x070F
            | 0x0890..=0x0891
            | 0x08E2
            | 0x180E
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0x110BD
            | 0x110CD
            | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0001
            | 0xE0020..=0xE007F
    )
}

/// Words the app will not send as a reply: a mention, a command, or an
/// invisible character.
fn sends_otherwise(words: &str) -> bool {
    words.contains('@') || words.starts_with(['/', '!']) || words.chars().any(is_default_ignorable)
}

/// The button the box draws for one of its actions: an app action with the
/// app's own words, or a reply the app sends as it is. None for one it
/// drops: an unknown action, a session offered as an action, a reply
/// without words, or one whose words would mention a file, run a command or
/// hide what they send (agent-hud-native-open ClaudeDesktopAsks.button).
fn claude_desktop_button(action: &Value) -> Option<DesktopAskOption> {
    if let Some(name) = action.get("action").and_then(Value::as_str) {
        return app_action(name).map(|label| DesktopAskOption {
            label: label.into(),
            message: None,
        });
    }
    let label =
        shown(&action["label"]).filter(|label| !label.is_empty() && !sends_otherwise(label))?;
    let message = shown(&action["message"]).filter(|message| !message.is_empty());
    if message.as_deref().is_some_and(sends_otherwise) {
        return None;
    }
    Some(DesktopAskOption { label, message })
}

pub fn claude_desktop_ask(
    name: &str,
    input: &Value,
    id: &str,
    response: Option<String>,
    at: i64,
) -> Option<DesktopAsk> {
    match name {
        "mcp__ccd_turn__wrap_up" => {
            let recap = trimmed_str(&input["recap"]).or_else(|| trimmed_str(&input["outcome"]))?;
            let question = trimmed_str(&input["ask"])
                .or_else(|| trimmed_str(&input["next_ask"]))
                .or_else(|| trimmed_str(&input["next_brief"]))?;
            let raw_actions = input
                .get("actions")
                .or_else(|| input.get("next_actions"))
                .and_then(Value::as_array);
            let needs_user = input["blocked"] == true
                || input["blocked"] == "true"
                || trimmed_str(&input["needs_user"]).is_some();
            let first_valid = raw_actions
                .and_then(|actions| actions.first())
                .and_then(claude_desktop_button)
                .is_some();
            if !needs_user && !first_valid {
                return None;
            }
            // The buttons the app keeps, the first two; a reply it would not
            // send is left out after they are taken.
            let options = raw_actions
                .into_iter()
                .flatten()
                .filter(
                    |action| match action.get("action").and_then(Value::as_str) {
                        Some(name) => name == "new_session" || app_action(name).is_some(),
                        None => action.is_object() && trimmed_str(&action["label"]).is_some(),
                    },
                )
                .take(2)
                .filter_map(claude_desktop_button)
                .collect::<Vec<_>>();
            Some(DesktopAsk {
                id: id.into(),
                response,
                kind: "next_step".into(),
                question,
                recap: Some(recap),
                options,
                at,
                offset: None,
            })
        }
        "mcp__ccd_session__start_session" if input["initiation"] == "offer" => {
            let title = trimmed_str(&input["title"])?;
            // The offer card shows its title and the app's two buttons only.
            let recap = None;
            let options = vec![
                DesktopAskOption {
                    label: "New session".into(),
                    message: None,
                },
                DesktopAskOption {
                    label: "Do it here".into(),
                    message: None,
                },
            ];
            Some(DesktopAsk {
                id: id.into(),
                response,
                kind: "offer".into(),
                question: title,
                recap,
                options,
                at,
                offset: None,
            })
        }
        _ => None,
    }
}

/// The ask `ask` with its words, read again from the one line its call is on
/// in the log at `path`, after a checkpoint gave it back without them; at
/// most 64 KiB of the log are read. None when that line is gone, longer, or
/// holds no such call (agent-hud-native-open ClaudeTranscriptStore.reworded).
pub fn reworded_desktop_ask(path: &Path, ask: &DesktopAsk) -> Option<DesktopAsk> {
    use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
    let mut file = fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(ask.offset?)).ok()?;
    let mut line = Vec::new();
    BufReader::new(file.take(64 << 10))
        .read_until(b'\n', &mut line)
        .ok()?;
    if line.last() != Some(&b'\n') {
        return None;
    }
    let value = serde_json::from_slice::<Value>(&line).ok()?;
    let call = value["message"]["content"]
        .as_array()?
        .iter()
        .find(|block| block["type"] == "tool_use" && block["id"] == ask.id.as_str())?;
    let response = value["message"]["id"].as_str().map(str::to_owned);
    let worded = claude_desktop_ask(
        call["name"].as_str()?,
        &call["input"],
        &ask.id,
        response,
        ask.at,
    )?;
    (worded.kind == ask.kind).then_some(DesktopAsk {
        offset: ask.offset,
        ..worded
    })
}

/// The card of a question the Claude desktop app asks. `unanswerable` is
/// why it can only be answered in the app, and the step that would let the
/// host answer it (`answerFix`); None for a next-step box the host can
/// answer, whose answer then goes into its session as words.
pub fn desktop_ask_view(
    req_id: &str,
    session_id: &str,
    cwd: Option<&str>,
    ask: &DesktopAsk,
    unanswerable: Option<crate::replies::Unreachable>,
) -> Value {
    let tool = if ask.kind == "offer" {
        "mcp__ccd_session__start_session"
    } else {
        "mcp__ccd_turn__wrap_up"
    };
    let header = if ask.kind == "offer" {
        crate::l10n::text("新任务", "New task")
    } else {
        crate::l10n::text("下一步", "Next step")
    };
    let options = ask
        .options
        .iter()
        .map(|opt| json!({"label": opt.label, "description": opt.message}))
        .collect::<Vec<_>>();
    let question = json!({
        "id": ask.id,
        "question": ask.question,
        "header": header,
        "options": options,
        "multiSelect": false,
        "answerKey": ask.id,
    });
    let created_at = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ask.at)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default();
    json!({
        "id": req_id,
        "title": ask.question,
        "source": "claude",
        "vendor": "Claude",
        "toolName": tool,
        "input": ask.recap.as_deref().unwrap_or(""),
        "detail": ask.recap,
        "summary": ask.question,
        "sessionId": session_id,
        "createdAt": created_at,
        "allowsAlways": false,
        "kind": "question",
        "cwd": cwd,
        "path": null,
        "removed": null,
        "added": null,
        "suggestions": [],
        "alwaysAllowRule": null,
        "questions": [question],
        "plan": null,
        "answersQuestion": unanswerable.is_none(),
        "answerNote": unanswerable.as_ref().map(|why| why.note.as_str()),
        "answerFix": unanswerable.and_then(|why| why.fix).map(|fix| fix.as_str()),
        "answersPlan": false,
    })
}

/// The words an answer to a next-step box stands for, as a click on the box
/// sends them: the user's own, else the chosen button's message, else its
/// label. None for a box put away or left unanswered
/// (agent-hud-native-open ClaudeDesktopAsks.words).
pub fn desktop_ask_words(view: &Value, decision: &Value) -> Option<String> {
    if decision["kind"] != "answer" {
        return None;
    }
    let question = view["questions"].get(0)?;
    let answer = decision["answers"].get(question["answerKey"].as_str()?)?;
    if let Some(own) = answer["custom"]
        .as_str()
        .map(str::trim)
        .filter(|own| !own.is_empty())
    {
        return Some(own.to_owned());
    }
    let label = answer["selected"].get(0)?.as_str()?;
    let option = question["options"]
        .as_array()?
        .iter()
        .find(|option| option["label"] == label)?;
    Some(option["description"].as_str().unwrap_or(label).to_owned())
}

#[derive(Clone, Default)]
pub struct ClaudeDesktopIndex {
    pub archived: HashSet<String>,
    pub targets: HashMap<String, Value>,
    /// Each metadata file as last read, by its modification time and size, so
    /// a pass re-reads only the files that changed (agent-hud-native-open
    /// ClaudeDesktopSessions keeps the same cache).
    files: HashMap<PathBuf, DesktopFile>,
}

#[derive(Clone)]
struct DesktopFile {
    stamp: (Option<std::time::SystemTime>, u64),
    /// The tab's kind, its own id, the Claude Code session it holds and
    /// whether it is archived; None for a file that is not such a record.
    tab: Option<(&'static str, String, String, bool)>,
}

/// Where the Claude desktop app keeps its data under a home folder, on any
/// system: Application Support on macOS, roaming AppData on Windows (or the
/// Microsoft Store package's copy of it), the config folder on Linux. Only
/// the folders that exist, each once.
pub fn desktop_bases(home: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![
        home.join("Library/Application Support/Claude"),
        home.join("AppData/Roaming/Claude"),
        home.join(".config/Claude"),
    ];
    if let Some(roaming) = std::env::var_os("APPDATA").map(PathBuf::from) {
        if roaming.starts_with(home) {
            candidates.push(roaming.join("Claude"));
        }
    }
    let packages = home.join("AppData/Local/Packages");
    if let Ok(entries) = std::fs::read_dir(&packages) {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("Claude_"))
            {
                candidates.push(entry.path().join("LocalCache/Roaming/Claude"));
            }
        }
    }
    let mut found: Vec<PathBuf> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        if !candidate.is_dir() {
            continue;
        }
        let real = std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
        if !seen.contains(&real) {
            seen.push(real);
            found.push(candidate);
        }
    }
    found
}

impl ClaudeDesktopIndex {
    /// Reads the desktop app's session metadata again, every collection pass
    /// as Open does, and says whether the archive or the tabs changed.
    pub fn refresh(&mut self, provider_roots: &[(PathBuf, &'static str)]) -> bool {
        let mut homes = Vec::new();
        for (root, source) in provider_roots {
            if *source == "claude" {
                if let Some(home) = root.parent().and_then(Path::parent) {
                    if !homes.contains(&home.to_path_buf()) {
                        homes.push(home.to_path_buf());
                    }
                }
            }
        }
        for variable in ["HOME", "USERPROFILE"] {
            if let Some(env_home) = std::env::var_os(variable).map(PathBuf::from) {
                if !homes.contains(&env_home) {
                    homes.push(env_home);
                }
            }
        }
        let mut bases: Vec<PathBuf> = Vec::new();
        for home in &homes {
            for base in desktop_bases(home) {
                if !bases.contains(&base) {
                    bases.push(base);
                }
            }
        }
        let mut files = HashMap::new();
        for base in bases {
            for (folder, kind) in [
                ("claude-code-sessions", "claude-desktop"),
                ("local-agent-mode-sessions", "claude-cowork"),
            ] {
                for path in desktop_metadata_files(&base.join(folder)) {
                    let Ok(meta) = fs::symlink_metadata(&path) else {
                        continue;
                    };
                    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 1024 * 1024
                    {
                        continue;
                    }
                    let stamp = (meta.modified().ok(), meta.len());
                    let file = match self.files.get(&path).filter(|file| file.stamp == stamp) {
                        Some(file) => file.clone(),
                        None => DesktopFile {
                            stamp,
                            tab: desktop_tab(&path, kind),
                        },
                    };
                    files.insert(path, file);
                }
            }
        }
        self.files = files;
        let mut tabs: HashMap<&str, (usize, usize)> = HashMap::new();
        let mut targets: HashMap<String, Value> = HashMap::new();
        let mut ambiguous: HashSet<String> = HashSet::new();
        let mut paths = self.files.keys().collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            let Some((kind, stem, cli_id, archived)) = &self.files[path].tab else {
                continue;
            };
            let counts = tabs.entry(cli_id).or_insert((0, 0));
            counts.0 += 1;
            if *archived {
                counts.1 += 1;
            } else {
                let target = json!({"kind": kind, "id": stem});
                if targets
                    .get(cli_id)
                    .is_some_and(|existing| existing != &target)
                {
                    ambiguous.insert(cli_id.clone());
                }
                targets.insert(cli_id.clone(), target);
            }
        }
        targets.retain(|id, _| !ambiguous.contains(id));
        let archived = tabs
            .into_iter()
            .filter(|(_, (all, arch))| *all > 0 && all == arch)
            .map(|(id, _)| id.to_owned())
            .collect::<HashSet<_>>();
        let changed = archived != self.archived || targets != self.targets;
        self.archived = archived;
        self.targets = targets;
        changed
    }
}

/// Exactly `<root>/<organization>/<account>/local_*.json`; symlinked
/// folders are not followed.
fn desktop_metadata_files(root: &Path) -> Vec<PathBuf> {
    let folders = |path: &Path| {
        fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                fs::symlink_metadata(path)
                    .is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
            })
            .collect::<Vec<_>>()
    };
    let mut found = Vec::new();
    for organization in folders(root) {
        for account in folders(&organization) {
            for file in fs::read_dir(&account).into_iter().flatten().flatten() {
                let path = file.path();
                let stem = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("");
                if path.extension().and_then(|ext| ext.to_str()) == Some("json")
                    && stem.strip_prefix("local_").is_some_and(uuid)
                {
                    found.push(path);
                }
            }
        }
    }
    found
}

/// The tab a metadata file records: only its ids and archive flag are read.
fn desktop_tab(path: &Path, kind: &'static str) -> Option<(&'static str, String, String, bool)> {
    let stem = path.file_stem()?.to_str()?.to_owned();
    let value = serde_json::from_slice::<Value>(&fs::read(path).ok()?).ok()?;
    if value["sessionId"].as_str() != Some(stem.as_str()) {
        return None;
    }
    let cli_id = value["cliSessionId"]
        .as_str()
        .filter(|id| uuid(id))?
        .to_owned();
    Some((kind, stem, cli_id, value["isArchived"] == true))
}

/// The desktop tab kind a Claude Code session's client opens in: the
/// desktop app's Code tab, or Cowork (agent-hud-native-open
/// ClaudeSessionOrigins.desktopHost). `client` is the session's client as
/// `naming::clients::client_name` names it.
pub fn desktop_kind(client: Option<&str>) -> Option<&'static str> {
    match client {
        Some("Claude Code Desktop" | "claude-desktop-3p") => Some("claude-desktop"),
        Some("local-agent") => Some("claude-cowork"),
        _ => None,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn assistant(id: Option<&str>, request: Option<&str>, model: &str, usage: Value) -> Value {
        let mut message = json!({"role":"assistant","model":model,"usage":usage,"content":[{"type":"text","text":"ok"}]});
        if let Some(id) = id {
            message["id"] = json!(id);
        }
        let mut line = json!({"type":"assistant","sessionId":"s1","timestamp":"2026-10-21T08:00:00Z","message":message});
        if let Some(request) = request {
            line["requestId"] = json!(request);
        }
        line
    }

    #[test]
    fn thinking_tokens_count_as_reasoning_within_the_output() {
        let mut log = Log::new("s1.jsonl".into(), "claude");
        log.parse(&assistant(Some("m1"), None, "claude-fable-5", json!({"input_tokens":2,"output_tokens":448,"cache_read_input_tokens":100,"output_tokens_details":{"thinking_tokens":295}})));
        assert_eq!(log.usage.output, 153);
        assert_eq!(log.usage.reasoning, 295);
        assert_eq!(log.usage.total(), 2 + 448 + 100);
        let event = log.meter_events.values().next().unwrap();
        assert_eq!((event.usage.output, event.usage.reasoning), (153, 295));
        assert_eq!(
            log.turns[0]["output"], 448,
            "a turn's output keeps its thinking"
        );
    }

    #[test]
    fn a_response_counts_its_whole_output_when_earlier_lines_had_part_of_it() {
        // Builds before about 2026-09 wrote the first blocks' lines with the
        // output counted so far; only the last line holds the whole count.
        let mut log = Log::new("s1.jsonl".into(), "claude");
        let line = |output, thinking: Option<u64>| {
            let mut usage = json!({"input_tokens":2,"output_tokens":output,"cache_read_input_tokens":57680,"cache_creation_input_tokens":4412});
            if let Some(thinking) = thinking {
                usage["output_tokens_details"] = json!({"thinking_tokens":thinking});
            }
            assistant(Some("m1"), Some("req_1"), "claude-fable-5", usage)
        };
        log.parse(&line(9, None));
        log.parse(&line(9, None));
        log.parse(&line(448, Some(295)));
        assert_eq!(log.usage.output + log.usage.reasoning, 448);
        assert_eq!(log.usage.total(), 2 + 448 + 57680 + 4412);
        // A first line that reported more than the final non-thinking share
        // still counts the response's output once.
        let mut log = Log::new("s1.jsonl".into(), "claude");
        log.parse(&line(300, None));
        log.parse(&line(448, Some(295)));
        assert_eq!((log.usage.output, log.usage.reasoning), (153, 295));
    }

    #[test]
    fn a_response_without_a_message_id_counts_once_by_request_or_by_line() {
        let mut log = Log::new("s1.jsonl".into(), "claude");
        let usage = json!({"input_tokens":10,"output_tokens":3});
        log.parse(&assistant(
            None,
            Some("req_1"),
            "claude-fable-5",
            usage.clone(),
        ));
        log.parse(&assistant(
            None,
            Some("req_1"),
            "claude-fable-5",
            usage.clone(),
        ));
        assert_eq!(log.usage.total(), 13, "lines of one request count once");
        log.meter_line_start = 900;
        log.parse(&assistant(None, None, "claude-fable-5", usage.clone()));
        log.meter_line_start = 1200;
        log.parse(&assistant(None, None, "claude-fable-5", usage));
        assert_eq!(
            log.usage.total(),
            39,
            "a line with neither id counts by its place in the log"
        );
        let mut keys = log.meter_events.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        let mut expected = ["r:req_1", "o:900", "o:1200"]
            .map(metering::event_key)
            .to_vec();
        expected.sort();
        assert_eq!(keys, expected);
    }

    #[test]
    fn the_session_model_is_the_one_that_wrote_the_most_output() {
        let mut log = Log::new("s1.jsonl".into(), "claude");
        log.parse(&assistant(
            Some("m1"),
            None,
            "claude-opus-5",
            json!({"input_tokens":1,"output_tokens":900}),
        ));
        log.parse(&assistant(
            Some("m2"),
            None,
            "claude-fable-5",
            json!({"input_tokens":1,"output_tokens":100}),
        ));
        log.parse(&assistant(
            Some("m3"),
            None,
            "<synthetic>",
            json!({"input_tokens":1,"output_tokens":5000}),
        ));
        assert_eq!(
            log.model, "claude-opus-5",
            "the latest line does not name the session's model; a synthetic line never does"
        );
        let synthetic = log.meter_events.get(&metering::event_key("m:m3")).unwrap();
        assert_eq!(
            synthetic.model, "",
            "a synthetic line's call has no model (Unknown)"
        );
        let mut only = Log::new("s2.jsonl".into(), "claude");
        only.parse(&assistant(
            Some("m1"),
            None,
            "<synthetic>",
            json!({"input_tokens":0,"output_tokens":0}),
        ));
        assert_eq!(only.model, "Unknown");
    }

    #[test]
    fn a_limit_reset_reads_seconds_or_milliseconds() {
        let reset = |at: Value| {
            let mut live = LiveLog::default();
            live.claude(&json!({"type":"user","timestamp":"2026-10-21T08:00:00Z","message":{"role":"user","content":"go"}}));
            live.claude(&json!({"type":"assistant","timestamp":"2026-10-21T08:00:05Z","isApiErrorMessage":true,"error":"rate_limit","quotaLimits":{"resetsAt":at},"message":{"role":"assistant","model":"<synthetic>","content":[]}}));
            live.turns.back().unwrap().limited_until
        };
        assert_eq!(reset(json!(1_792_000_000)), Some(1_792_000_000_000));
        assert_eq!(reset(json!(1_792_000_000_000_i64)), Some(1_792_000_000_000));
    }

    #[test]
    fn a_narration_is_an_item_of_the_agents_text() {
        use base64::Engine;
        let message = |kind: &str| {
            let inner = [&[0x42, kind.len() as u8][..], kind.as_bytes()].concat();
            let middle = [&[0x0a, inner.len() as u8][..], &inner].concat();
            [&[0x12, middle.len() as u8][..], &middle].concat()
        };
        let signature =
            |kind: &str| base64::engine::general_purpose::STANDARD_NO_PAD.encode(message(kind));
        let line = |kind: &str, words: &str| json!({"type":"assistant","uuid":"a1","timestamp":"2026-10-25T14:48:37Z","message":{"role":"assistant","id":"m1","content":[{"type":"thinking","thinking":words,"signature":signature(kind)}]}});
        let said = (ITEMS.read)(
            &line("narration", "I've pulled up the last seven turns."),
            0,
        );
        assert_eq!(said.len(), 1, "{said:?}");
        assert_eq!(said[0].role, items::LineRole::Assistant);
        assert_eq!(said[0].kind, items::LineKind::Text);
        assert_eq!(
            said[0].text.as_deref(),
            Some("I've pulled up the last seven turns.")
        );
        assert!(
            (ITEMS.read)(&line("thinking", "private reasoning"), 0).is_empty(),
            "reasoning is no text"
        );
    }

    #[test]
    fn a_narration_is_the_agents_words_not_its_thinking() {
        use base64::Engine;
        // field 2 { field 1 { field 8 "narration" } }
        let message = |kind: &str| {
            let inner = [&[0x42, kind.len() as u8][..], kind.as_bytes()].concat();
            let middle = [&[0x0a, inner.len() as u8][..], &inner].concat();
            [&[0x12, middle.len() as u8][..], &middle].concat()
        };
        let signature =
            |kind: &str| base64::engine::general_purpose::STANDARD_NO_PAD.encode(message(kind));
        assert_eq!(
            signature_kind(&signature("narration")).as_deref(),
            Some("narration")
        );
        assert_eq!(signature_kind("not base64!"), None);
        let mut live = LiveLog::new(Path::new("s1.jsonl"), "claude");
        live.claude(&json!({"type":"user","timestamp":"2026-10-21T08:00:00Z","message":{"role":"user","content":"go"}}));
        live.claude(&json!({"type":"assistant","timestamp":"2026-10-21T08:00:05Z","message":{"role":"assistant","model":"claude-fable-5","id":"m1","content":[{"type":"thinking","thinking":"Reading the build log now.","signature":signature("narration")}]}}));
        assert_eq!(
            live.activity.as_ref().unwrap().line.as_deref(),
            Some("text")
        );
        assert_eq!(
            live.turns.back().unwrap().message.as_deref(),
            Some("Reading the build log now.")
        );
        live.claude(&json!({"type":"assistant","timestamp":"2026-10-21T08:00:06Z","message":{"role":"assistant","model":"claude-fable-5","id":"m2","content":[{"type":"thinking","thinking":"private reasoning","signature":signature("thinking")}]}}));
        assert_eq!(
            live.activity.as_ref().unwrap().line.as_deref(),
            Some("thinking")
        );
        assert_eq!(
            live.turns.back().unwrap().message.as_deref(),
            Some("Reading the build log now.")
        );
    }

    #[test]
    fn the_next_step_box_keeps_the_buttons_the_app_draws() {
        let ask = |actions: Value| {
            let input = json!({"recap":"Done.","ask":"Commit it?","actions":actions});
            claude_desktop_ask("mcp__ccd_turn__wrap_up", &input, "t1", None, 0).map(|ask| {
                ask.options
                    .into_iter()
                    .map(|option| (option.label, option.message))
                    .collect::<Vec<_>>()
            })
        };
        let commit = ("Commit".to_owned(), None);
        // Unknown actions are left out before the first two are taken.
        assert_eq!(
            ask(
                json!([{"action":"commit","label":"Ship"},{"action":"bogus"},{"label":"Run tests","message":"Run the tests"}])
            ),
            Some(vec![
                commit.clone(),
                ("Run tests".into(), Some("Run the tests".into()))
            ]),
        );
        // A session offered as an action takes a place but draws no button.
        assert_eq!(
            ask(json!([{"action":"commit"},{"action":"new_session"},{"label":"Run"}])),
            Some(vec![commit.clone()])
        );
        // A reply that would mention, run a command or hide its words is dropped.
        assert_eq!(
            ask(
                json!([{"action":"commit"},{"label":"Ping @bob"},{"label":"Go","message":"/deploy"}])
            ),
            Some(vec![commit.clone()]),
        );
        // Control characters and runs of spaces are cleaned from what is shown.
        assert_eq!(
            ask(json!([{"label":"Run\u{0007}   now"}])),
            Some(vec![("Run now".into(), None)]),
        );
        // A box whose first button cannot be drawn shows only when blocked.
        assert_eq!(ask(json!([{"action":"bogus"},{"action":"commit"}])), None);
        let offer = claude_desktop_ask(
            "mcp__ccd_session__start_session",
            &json!({"initiation":"offer","title":"Split the parser","initial_message":"Work on it"}),
            "t2",
            None,
            0,
        )
        .unwrap();
        assert!(
            offer.recap.is_none(),
            "an offer card shows its title and buttons only"
        );
    }
}
