//! Codex transcript usage: the calls a rollout meters and the provider they run on.
use crate::{
    number,
    transcript::{live::timestamp, metering},
    Log, Usage,
};
use serde_json::{json, Value};

impl Log {
    /// Keeps the model provider a rollout's own `session_meta` names, so its
    /// calls carry it as their route (`codex_model`): `openai` and none are
    /// OpenAI's own service; `ollama`, `lmstudio` and `oss` run on this
    /// computer; any other id is a provider configured in `config.toml`.
    pub fn note_codex_provider(&mut self, value: &Value) {
        if value["type"] != "session_meta" {
            return;
        }
        let payload = &value["payload"];
        let own = match self.live.session_id.as_deref() {
            Some(session) => payload["id"].as_str() == Some(session),
            None => true,
        };
        if own {
            self.source_state["provider"] =
                json!(payload["model_provider"].as_str().unwrap_or("openai"));
        }
    }

    /// Whether the rollout's provider is known, or a read resuming past its
    /// first line has to look at it (`note_codex_provider`).
    pub fn codex_provider_known(&self) -> bool {
        self.source_state["provider"].is_string()
    }

    /// The model a call is recorded under: `<model>#<provider>` when the
    /// provider is not OpenAI's own service, so pricing can tell a relay or a
    /// local server from OpenAI (`pricing::model_for`).
    fn codex_model(&self) -> String {
        if self.model == "Unknown" {
            return String::new();
        }
        match self.source_state["provider"].as_str() {
            Some(provider) if !provider.is_empty() && provider != "openai" => {
                format!("{}#{provider}", self.model)
            }
            _ => self.model.clone(),
        }
    }

    pub fn meter_codex_call(&mut self, value: &Value) {
        self.note_codex_provider(value);
        // A reply is timed from the newest request-side line (a prompt, a tool
        // result) to its `token_usage_record`, written as the reply finishes.
        // The token count that closes the call comes after the tool results,
        // so its own time holds the tool runs. A fork's copied history times
        // nothing.
        if !self.live.inherited(value) {
            if let Some(at) = timestamp(&value["timestamp"]) {
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
        let Some(at) = timestamp(&value["timestamp"]) else {
            return;
        };
        let event = metering::Event {
            key: metering::event_key(&format!("o{}", self.meter_line_start)),
            model: self.codex_model(),
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
        let at = || timestamp(&value["timestamp"]);
        if !self.live.inherited(value) {
            if value["type"] == "compacted" {
                self.mark(at(), metering::MarkKind::Compaction);
            } else if value["type"] == "event_msg" && payload["type"] == "task_started" {
                self.mark_prompt(at(), None);
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
                    let path = crate::transcript::text(&spawn["agent_path"]).map(|path| {
                        path.trim_end_matches('/')
                            .rsplit('/')
                            .next()
                            .unwrap_or(path)
                            .to_owned()
                    });
                    let nickname = crate::transcript::text(&spawn["agent_nickname"])
                        .or_else(|| crate::transcript::text(&payload["agent_nickname"]))
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
