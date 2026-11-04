//! Antigravity JSONL transcript rules (`brain/<id>/.system_generated/logs/transcript.jsonl`):
//! where its sessions live, turn boundaries, and conversation items.
use crate::transcript::{
    items::{Items, LineItem, LineKind, LineRole, ToolCall},
    text,
    turns::{has, TurnStart, Turns},
};
use serde_json::Value;
use std::path::Path;

/// Infers the Antigravity client surface (`Desktop`, `IDE`, or `CLI`) from a
/// storage path (`~/.gemini/antigravity`, `~/.gemini/antigravity-ide`, or
/// `~/.gemini/antigravity-cli`).
pub fn client_from_path(path: &Path) -> Option<String> {
    for part in path.components() {
        let name = part.as_os_str().to_str()?;
        match name {
            "antigravity-cli" => return Some("CLI".into()),
            "antigravity-ide" => return Some("IDE".into()),
            "antigravity" => return Some("Desktop".into()),
            _ => {}
        }
    }
    None
}

/// Extracts the conversation UUID from an Antigravity `brain/<id>/...` or
/// `conversations/<id>.db` path.
pub fn session_id_from_path(path: &Path) -> Option<String> {
    let mut parts = path.components();
    while let Some(part) = parts.next() {
        if part.as_os_str() == "brain" {
            if let Some(id) = parts.next().and_then(|c| c.as_os_str().to_str()) {
                if !id.is_empty() {
                    return Some(id.to_owned());
                }
            }
        }
    }
    path.file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty() && *s != "transcript" && *s != "transcript_full")
        .map(str::to_owned)
}

/// Extracts the user's request text from Antigravity's `<USER_REQUEST>` wrapper
/// and strips `<ADDITIONAL_METADATA>` / `<USER_SETTINGS_CHANGE>` blocks.
pub fn clean_user_content(raw: &str) -> String {
    if let Some((_, rest)) = raw.split_once("<USER_REQUEST>") {
        if let Some((inside, _)) = rest.split_once("</USER_REQUEST>") {
            return inside.trim().to_owned();
        }
    }
    let mut cleaned = raw.to_owned();
    for (open, close) in [
        ("<ADDITIONAL_METADATA>", "</ADDITIONAL_METADATA>"),
        ("<USER_SETTINGS_CHANGE>", "</USER_SETTINGS_CHANGE>"),
        ("<CONTEXT_SUMMARY>", "</CONTEXT_SUMMARY>"),
    ] {
        while let Some(start) = cleaned.find(open) {
            if let Some(end_rel) = cleaned[start..].find(close) {
                let end = start + end_rel + close.len();
                cleaned.replace_range(start..end, "");
            } else {
                break;
            }
        }
    }
    cleaned.trim().to_owned()
}

pub(super) fn tool_name(call: &Value) -> Option<&str> {
    text(&call["name"]).or_else(|| text(call))
}

/// An Antigravity turn starts at each explicit `USER_INPUT` step.
pub static TURNS: Turns = Turns {
    start: |value| {
        if value["type"] != "USER_INPUT" || value["source"] == "SYSTEM" {
            return None;
        }
        let step = value["step_index"].as_u64()?;
        Some(TurnStart::whole(
            Some(format!("step-{step}")),
            value.get("created_at").or_else(|| value.get("timestamp")),
        ))
    },
    may_name: |line| has(line, b"USER_INPUT"),
};

/// Visible user prompts, assistant thinking, assistant text, and tool calls in
/// an Antigravity `transcript.jsonl` step.
pub static ITEMS: Items = Items { read: read_items };

fn read_items(value: &Value, offset: u64) -> Vec<LineItem> {
    let timestamp = value["created_at"]
        .as_str()
        .or_else(|| value["timestamp"].as_str())
        .map(str::to_owned);
    let base_id = value["step_index"]
        .as_u64()
        .map(|s| format!("step-{s}"))
        .unwrap_or_else(|| format!("o{offset}"));
    let mut rows = Vec::new();
    match value["type"].as_str() {
        Some("USER_INPUT") if value["source"] != "SYSTEM" => {
            if let Some(raw) = text(&value["content"]) {
                let prompt = clean_user_content(raw);
                if !prompt.is_empty() {
                    rows.push(LineItem {
                        id: base_id,
                        parent: None,
                        role: LineRole::User,
                        kind: LineKind::Text,
                        text: Some(prompt),
                        tool: None,
                        tool_call_id: None,
                        timestamp,
                        is_prompt: true,
                        origin: Some("human".into()),
                    });
                }
            }
        }
        Some("PLANNER_RESPONSE") => {
            if let Some(thinking) = text(&value["thinking"]) {
                rows.push(LineItem {
                    id: format!("{base_id}:thinking"),
                    parent: None,
                    role: LineRole::Assistant,
                    kind: LineKind::Thinking,
                    text: Some(thinking.trim().to_owned()),
                    tool: None,
                    tool_call_id: None,
                    timestamp: timestamp.clone(),
                    is_prompt: false,
                    origin: None,
                });
            }
            if let Some(content) = text(&value["content"]) {
                rows.push(LineItem {
                    id: format!("{base_id}:text"),
                    parent: None,
                    role: LineRole::Assistant,
                    kind: LineKind::Text,
                    text: Some(content.trim().to_owned()),
                    tool: None,
                    tool_call_id: None,
                    timestamp: timestamp.clone(),
                    is_prompt: false,
                    origin: None,
                });
            }
            if let Some(calls) = value["tool_calls"].as_array() {
                for (idx, call) in calls.iter().enumerate() {
                    let Some(name) = tool_name(call) else {
                        continue;
                    };
                    let call_id = text(&call["id"])
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("{base_id}:call:{idx}"));
                    let arguments = call
                        .get("args")
                        .or_else(|| call.get("arguments"))
                        .cloned()
                        .unwrap_or(Value::Null);
                    rows.push(LineItem {
                        id: format!("{base_id}:tool:{idx}"),
                        parent: None,
                        role: LineRole::Assistant,
                        kind: LineKind::Tool,
                        text: None,
                        tool: Some(ToolCall {
                            id: Some(call_id.clone()),
                            name: name.to_owned(),
                            arguments,
                        }),
                        tool_call_id: Some(call_id),
                        timestamp: timestamp.clone(),
                        is_prompt: false,
                        origin: None,
                    });
                }
            }
        }
        _ => {}
    }
    rows
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
