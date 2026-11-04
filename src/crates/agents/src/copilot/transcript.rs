//! GitHub Copilot JSONL session-state reducer (`events.jsonl`).
//! OTel multi-session spans are reduced by `source_batches` and mapped onto the same session.
//!
//! A session's tokens come from one place, the best it has: the session
//! store's rows, one per call (`store`); else the OTel export's chat spans,
//! one per call; else the cumulative `session.shutdown` totals here, each
//! counted by what it adds to the last. A better source, once read for a
//! session, replaces what a lesser one counted.
use crate::transcript::{at, n, text};
use crate::{Log, Usage};
use serde_json::{json, Value};
use std::path::Path;

impl Log {
    pub fn copilot(&mut self, v: &Value) {
        if self.path.file_name().is_none_or(|s| s != "events.jsonl") {
            return;
        }
        let raw = self
            .path
            .parent()
            .and_then(Path::file_name)
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        self.own_id(&raw);
        let Some(at) = at(&v["timestamp"]) else {
            return;
        };
        self.clock(at);
        let d = &v["data"];
        let kind = text(&v["type"]).unwrap_or("");
        if kind == "session.start" {
            self.source_state["started"] = json!(true);
            // workspace.yaml's folder, read first, wins.
            if self.cwd.is_none() {
                self.cwd = text(&d["context"]["cwd"]).map(str::to_owned);
            }
        }
        if let Some(model) = text(&d["selectedModel"])
            .or_else(|| text(&d["newModel"]))
            .filter(|m| *m != "auto")
        {
            self.model = model.into();
        }
        let turn = text(&v["id"])
            .map(str::to_owned)
            .unwrap_or_else(|| format!("at-{at}"));
        let main = v["agentId"].is_null();
        if main && matches!(kind, "user.message" | "assistant.turn_start") {
            self.source_state["turn"] = json!(turn);
            self.live.observe_source(&turn, true, Some(at), at, None);
        }
        if main
            && (kind == "abort"
                || kind == "session.shutdown"
                || (kind == "hook.start" && d["hookType"] == "agentStop"))
        {
            let turn = self.source_state["turn"].as_str().unwrap_or(&turn);
            let reason = text(&d["input"]["stopReason"]);
            self.live.observe_source(
                turn,
                false,
                None,
                at,
                if kind == "abort" {
                    Some("interrupted")
                } else if reason == Some("end_turn") {
                    Some("completed")
                } else if reason == Some("error") {
                    Some("failed")
                } else {
                    None
                },
            );
        }
        if kind != "session.shutdown"
            || self.source_state["hasOtel"] == true
            || self.source_state["hasInference"] == true
        {
            return;
        }
        let Some(metrics) = d["modelMetrics"].as_object() else {
            return;
        };
        for (raw, value) in metrics {
            // The same shutdown written twice is one: it adds nothing.
            let key = format!("{}:shutdown:{turn}:{raw}", self.id);
            if self.messages.contains_key(&key) {
                continue;
            }
            let u = &value["usage"];
            let next = Usage {
                input: n(&u["inputTokens"]),
                output: n(&u["outputTokens"]),
                read: n(&u["cacheReadTokens"]),
                ..Usage::default()
            };
            let previous: Option<Usage> =
                serde_json::from_value(self.source_state["peaks"][raw].clone()).ok();
            self.source_state["peaks"][raw] =
                serde_json::to_value(previous.unwrap_or(next).max(next)).unwrap();
            if previous.is_none() && self.source_state["started"] != true {
                continue;
            }
            let previous = previous.unwrap_or_default();
            let input = next.input.saturating_sub(previous.input);
            let output = next.output.saturating_sub(previous.output);
            let read = next.read.saturating_sub(previous.read).min(input);
            let model = if raw == "auto" || raw.is_empty() {
                text(&d["currentModel"]).unwrap_or(&self.model)
            } else {
                raw
            }
            .to_owned();
            self.count_source(
                key,
                &model,
                at,
                Usage {
                    input: input - read,
                    output,
                    read,
                    ..Usage::default()
                },
                false,
            );
        }
    }
}

/// A Copilot turn starts at the user's message or the assistant's turn start.
pub static TURNS: crate::transcript::turns::Turns = crate::transcript::turns::Turns {
    start: |value| {
        if !matches!(
            value["type"].as_str(),
            Some("user.message" | "assistant.turn_start")
        ) {
            return None;
        }
        let id = value["id"].as_str()?;
        Some(crate::transcript::turns::TurnStart::whole(
            Some(id.to_owned()),
            value.get("timestamp"),
        ))
    },
    may_name: |line| {
        crate::transcript::turns::has(line, b"user.message")
            || crate::transcript::turns::has(line, b"assistant.turn_start")
    },
};
