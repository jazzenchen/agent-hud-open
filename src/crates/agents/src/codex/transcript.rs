//! Codex transcript rules: which lines are the person's words, and the items
//! and turns its page shows.
use crate::transcript::items::{self, Items, Line, LineItem, ToolCall};
use serde_json::{json, Value};

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
