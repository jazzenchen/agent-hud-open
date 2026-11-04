//! The JSONL session reducer of Pi, and of Oh My Pi, which adds a title slot
//! before the header, titles in the header and in `title_change` lines,
//! `provider/model` in `model_change` lines and `model_usage` lines for calls
//! outside the conversation (can1357/oh-my-pi 33ea58a,
//! packages/coding-agent/src/session/session-entries.ts).
use crate::transcript::{at, n, parts, text};
use crate::{Log, Usage};
use serde_json::Value;

/// An entry at least this much older than its log's header was copied in
/// from the session the log continues or forks (`super::copies_history`).
const COPIED: i64 = 1000;

impl Log {
    pub fn pi(&mut self, v: &Value) {
        let kind = text(&v["type"]).unwrap_or("");
        let time = at(&v["timestamp"]);
        if kind == "session" {
            if let Some(id) = text(&v["id"]) {
                self.own_id(id);
            }
            self.cwd = text(&v["cwd"]).map(str::to_owned);
            if let Some(title) = text(&v["title"]) {
                self.naming.name(0, Some(title));
            }
            if let Some(at) = time {
                self.clock(at);
                if text(&v["parentSession"]).is_some() && super::copies_history(&self.source) {
                    self.source_state["copiedBefore"] = (at - COPIED).into();
                }
            }
            return;
        }
        if !self.observed {
            return;
        }
        if kind == "session_info" {
            // An empty name gives the session back its first prompt's title.
            match text(&v["name"]) {
                Some(name) => self.naming.name(0, Some(name)),
                None => self.naming.clear_name(),
            }
            return;
        }
        if kind == "title_change" {
            self.naming.name(0, text(&v["title"]));
            return;
        }
        if kind == "model_change" {
            // Oh My Pi names a model for a role ("smol", "slow") beside the
            // session's own, and writes it as provider/model.
            let model = text(&v["modelId"]).or_else(|| {
                text(&v["role"])
                    .is_none_or(|role| role == "default")
                    .then(|| text(&v["model"]))
                    .flatten()
                    .map(|model| model.split_once('/').map_or(model, |(_, id)| id))
            });
            if let Some(model) = model {
                self.model = model.into();
            }
            return;
        }
        let message = &v["message"];
        if kind == "message" && message["role"] == "user" {
            if let Some(prompt) = parts(&message["content"]) {
                self.title_from(prompt);
            }
            return;
        }
        let call = match kind {
            "message" if message["role"] == "assistant" => message,
            "model_usage" => v,
            _ => return,
        };
        if !call["usage"].is_object() {
            return;
        }
        let Some(at) = time.or_else(|| at(&call["timestamp"])) else {
            return;
        };
        if self.source_state["copiedBefore"]
            .as_i64()
            .is_some_and(|copied| at < copied)
        {
            return;
        }
        let u = &call["usage"];
        let model = text(&call["model"]).unwrap_or("Unknown");
        let provider = text(&call["provider"]).unwrap_or("Unknown");
        let source = &self.source;
        let key = if let Some(response) = text(&call["responseId"]) {
            format!("{source}:response:{provider}:{response}")
        } else if let Some(id) = text(&v["id"]) {
            format!("{source}:entry:{id}:{at}:{provider}:{model}")
        } else {
            format!("{}:o{}", self.id, self.meter_line_start)
        };
        // A call outside the conversation (a title, a compaction) leaves the
        // session's model as it was.
        let session_model = (kind == "model_usage").then(|| self.model.clone());
        self.count_source(
            key,
            &format!("{model}#{provider}"),
            at,
            Usage {
                input: n(&u["input"]),
                output: n(&u["output"]),
                read: n(&u["cacheRead"]),
                write: n(&u["cacheWrite"]),
                reasoning: 0,
            },
            false,
        );
        if let Some(model) = session_model {
            self.model = model;
        }
    }
}
