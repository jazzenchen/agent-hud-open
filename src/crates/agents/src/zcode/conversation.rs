//! A ZCode session's conversation, from the `message` and `part` tables of
//! its database. Each message is one line of the session: its `data` as
//! `info` and its parts in order, read with ZCode's own rules for turns
//! (`TURNS`) and rows (`ITEMS`). A message's place is its `sequence`, which
//! ZCode numbers from 0 in each session (zcode.cjs 0.15, migration
//! `0014_message_part_sequence`); an older database is read by row.
//!
//! What ZCode shows of a message is in its `semantics` (since 0.15): the
//! user's own words are a `user_prompt` or `slash_command` of the
//! `real_user`; the context it hands the model (`system_reminder`, with the
//! environment, the date, the skills) is `uiVisibility: hidden`. A message
//! of an older ZCode with no semantics is shown unless it is `synthetic` or
//! for the model only (`visibility: model-only`).
//!
//! What a Stop hook adds to continue a turn is not kept here: ZCode hands it
//! to the model as a `hook_context` attachment it holds in memory only
//! (`injectHookAdditionalContextIntoMessageHistory`), so the turn's further
//! replies follow its earlier ones with no prompt between them.
mod lines;

pub use lines::{last_messages, messages, Lines};

use crate::transcript::items::{Items, LineItem, LineKind, LineRole, ToolCall};
use crate::transcript::turns::{has, TurnStart, Turns};
use serde_json::{json, Value};

/// A user message's or a reply's turn, as ZCode anchors each message of
/// one; the user's own words start it.
pub static TURNS: Turns = Turns {
    start: turn,
    may_name: |line| has(line, b"turnId"),
};

fn turn(value: &Value) -> Option<TurnStart> {
    let info = &value["info"];
    let turn = info["anchor"]["turnId"].as_str()?.to_owned();
    if info["role"] == "user" && prompt(info) {
        let started = millis(&info["time"]["created"]).map(|at| json!(at));
        return Some(TurnStart::whole(Some(turn), started.as_ref()));
    }
    Some(TurnStart {
        turn: Some(turn),
        started: None,
        whole: false,
    })
}

/// What a ZCode message shows: the user's words and the notices ZCode shows
/// them, the replies' text, thinking and tool calls with what each returned.
pub static ITEMS: Items = Items { read: items };

fn items(value: &Value, place: u64) -> Vec<LineItem> {
    let info = &value["info"];
    if !shown(info) {
        return Vec::new();
    }
    let id = value["id"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("o{place}"));
    let created = millis(&info["time"]["created"]);
    let parts = value["parts"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let mut rows = Vec::new();
    match info["role"].as_str() {
        Some("user") => {
            let is_prompt = prompt(info);
            let origin = if is_prompt {
                Some("human".to_owned())
            } else {
                info["semantics"]["kind"].as_str().map(str::to_owned)
            };
            for (index, part) in parts.iter().enumerate() {
                if part["type"] != "text" || part["metadata"]["visibility"] == "model-only" {
                    continue;
                }
                let Some(text) = words(&part["text"]) else {
                    continue;
                };
                rows.push(LineItem {
                    id: part_id(part, &id, index),
                    parent: None,
                    role: LineRole::User,
                    kind: LineKind::Text,
                    text: Some(text),
                    tool: None,
                    tool_call_id: None,
                    timestamp: millis(&part["time"]["start"]).or_else(|| created.clone()),
                    is_prompt,
                    origin: origin.clone(),
                });
            }
        }
        Some("assistant") => {
            let parent = info["parentID"].as_str().map(str::to_owned);
            let row = |id: String, kind, text, timestamp: Option<String>| LineItem {
                id,
                parent: parent.clone(),
                role: LineRole::Assistant,
                kind,
                text,
                tool: None,
                tool_call_id: None,
                timestamp: timestamp.or_else(|| created.clone()),
                is_prompt: false,
                origin: None,
            };
            for (index, part) in parts.iter().enumerate() {
                let key = part_id(part, &id, index);
                let at = millis(&part["time"]["start"]);
                match part["type"].as_str() {
                    Some("text") if part["synthetic"] != true => {
                        if let Some(text) = words(&part["text"]) {
                            rows.push(row(key, LineKind::Text, Some(text), at));
                        }
                    }
                    Some("reasoning") => {
                        if let Some(text) = words(&part["text"]) {
                            rows.push(row(key, LineKind::Thinking, Some(text), at));
                        }
                    }
                    Some("tool") => {
                        let state = &part["state"];
                        let call = part["callID"].as_str().map(str::to_owned);
                        let mut asked = row(
                            key.clone(),
                            LineKind::Tool,
                            None,
                            millis(&state["time"]["start"]).or(at),
                        );
                        asked.tool = Some(ToolCall {
                            id: call.clone(),
                            name: part["tool"].as_str().unwrap_or_default().to_owned(),
                            arguments: state["input"].clone(),
                        });
                        asked.tool_call_id = call.clone();
                        rows.push(asked);
                        let answer = match state["status"].as_str() {
                            Some("completed") => &state["output"],
                            Some("error") => &state["error"],
                            _ => &Value::Null,
                        };
                        if let Some(text) = crate::transcript::items::text(answer.clone()) {
                            let mut result = row(
                                format!("{key}:result"),
                                LineKind::Tool,
                                Some(text),
                                millis(&state["time"]["end"]),
                            );
                            result.role = LineRole::Tool;
                            result.tool_call_id = call;
                            rows.push(result);
                        }
                    }
                    _ => {}
                }
            }
            // A call that failed before it said anything says why.
            if rows.is_empty() {
                let error = &info["error"];
                if let Some(text) =
                    words(&error["data"]["message"]).or_else(|| words(&error["message"]))
                {
                    rows.push(row(format!("{id}:error"), LineKind::Text, Some(text), None));
                }
            }
        }
        _ => {}
    }
    rows
}

/// Whether ZCode shows the message in its conversation. A timeline event (a
/// change of model) is a separator, not a message.
fn shown(info: &Value) -> bool {
    let semantics = &info["semantics"];
    if semantics.is_object() {
        return semantics["uiVisibility"] == "visible" && semantics["kind"] != "timeline_event";
    }
    info["visibility"] != "model-only" && info["synthetic"] != true
}

/// Whether a user message holds the user's own words.
fn prompt(info: &Value) -> bool {
    let semantics = &info["semantics"];
    if semantics.is_object() {
        return semantics["origin"] == "real_user"
            && matches!(
                semantics["kind"].as_str(),
                Some("user_prompt" | "slash_command")
            );
    }
    info["role"] == "user" && shown(info)
}

fn part_id(part: &Value, message: &str, index: usize) -> String {
    part["id"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{message}:{index}"))
}

fn words(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Milliseconds as RFC 3339.
fn millis(value: &Value) -> Option<String> {
    value.as_i64().filter(|at| *at > 0).and_then(crate::iso)
}

#[cfg(test)]
#[path = "conversation_tests.rs"]
mod tests;
