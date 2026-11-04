//! Claude Code transcript usage: the responses a log meters and the calls they make.
use super::project_origin::is_managed_project_prompt;
use super::transcript::claude_prompt;
use crate::{
    number,
    transcript::{live::timestamp, metering},
    Log, RecentCall, Usage,
};
use serde_json::{json, Value};

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
            let at = timestamp(&value["timestamp"]);
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
        let line_at = timestamp(&value["timestamp"]);
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
            Some("user") if !is_managed_project_prompt(value) => {
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
        let next = crate::qoder::transcript::counters(
            &self.source,
            Usage {
                input: number(usage, "input_tokens"),
                output: number(usage, "output_tokens"),
                read: number(usage, "cache_read_input_tokens"),
                write: number(usage, "cache_creation_input_tokens"),
                reasoning: number(&usage["output_tokens_details"], "thinking_tokens"),
            },
        );
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
