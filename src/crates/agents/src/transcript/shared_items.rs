//! The shared rule for clients that state none of their own, and the message
//! rows Claude's and Codex's own rules build on.
use super::items::{Line, LineItem, ToolCall};
use serde_json::{json, Value};

pub(super) fn shared(value: &Value, offset: u64) -> Vec<LineItem> {
    use crate::{claude::transcript as claude, codex::transcript as codex};
    if claude::request_copy(value) {
        return Vec::new();
    }
    if let Some(items) = codex::response_items(value, offset) {
        return items;
    }
    let prompt = if value["message"].is_object() {
        claude::claude_prompt(value) && !claude::interrupted(value)
    } else {
        codex::user_words(value)
    };
    let line = Line::of(value, prompt);
    let mut rows = Vec::new();
    if matches!(
        value["type"].as_str(),
        Some("user/message" | "assistant/message")
    ) {
        let role = if value["type"] == "user/message" {
            "user"
        } else {
            "assistant"
        };
        let content = &value["data"]["content"];
        let text = content
            .as_str()
            .map(str::to_owned)
            .or_else(|| {
                content.as_array().map(|parts| {
                    parts
                        .iter()
                        .filter_map(|v| v["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                })
            })
            .unwrap_or_default();
        if !text.is_empty() {
            rows.push(line.item(format!("o{offset}"), role, json!(text), None, &Value::Null));
        }
    } else if matches!(
        value["type"].as_str(),
        Some("user.message" | "assistant.message")
    ) {
        let role = if value["type"] == "user.message" {
            "user"
        } else {
            "assistant"
        };
        let data = &value["data"];
        if let Some(text) = data["content"]
            .as_str()
            .or_else(|| data["text"].as_str())
            .filter(|s| !s.is_empty())
        {
            let id = value["id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("o{offset}"));
            rows.push(line.item(id, role, json!(text), None, &Value::Null));
        }
    } else if let Some(items) = claude::message_items(value, offset) {
        rows = items;
    } else if matches!(
        value["type"].as_str(),
        Some("user" | "assistant" | "message")
    ) {
        rows = message(value, value, offset, prompt);
    }
    rows
}

/// The rows of a chat message `item` that line `value` holds: its text,
/// or each of its text, tool call and tool result blocks. Claude Code, Pi
/// and older Codex rollouts write their messages this way.
pub fn message(value: &Value, item: &Value, offset: u64, prompt: bool) -> Vec<LineItem> {
    let role = item["role"]
        .as_str()
        .or_else(|| value["role"].as_str())
        .or_else(|| value["type"].as_str())
        .unwrap_or("");
    if !matches!(role, "user" | "assistant" | "tool" | "toolResult") {
        return Vec::new();
    }
    let id = value["uuid"]
        .as_str()
        .or_else(|| item["id"].as_str())
        .or_else(|| value["id"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("o{offset}"));
    let line = Line {
        parent: value
            .get("parentUuid")
            .or_else(|| value.get("parentId"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        ..Line::of(value, prompt)
    };
    let mut rows = Vec::new();
    if let Some(text) = item["content"].as_str() {
        rows.push(line.item(id, role, json!(text), None, &item["toolCallId"]));
    } else if let Some(blocks) = item["content"].as_array() {
        for (index, block) in blocks.iter().enumerate() {
            let key = format!("{id}:{index}");
            match block["type"].as_str() {
                Some("text" | "input_text" | "output_text") => {
                    rows.push(line.item(key, role, block["text"].clone(), None, &Value::Null))
                }
                Some("tool_use" | "toolCall") => rows.push(line.item(
                    key,
                    "assistant",
                    Value::Null,
                    Some(ToolCall::new(
                        &block["id"],
                        &block["name"],
                        block.get("input").or_else(|| block.get("arguments")),
                    )),
                    &block["id"],
                )),
                Some("tool_result") => rows.push(line.item(
                    key,
                    "tool",
                    block["content"].clone(),
                    None,
                    &block["tool_use_id"],
                )),
                _ => {}
            }
        }
    } else if let Some(parts) = item["parts"].as_array() {
        let text = parts
            .iter()
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            rows.push(line.item(id, role, json!(text), None, &Value::Null));
        }
    }
    rows
}

/// When a line was written: its own `timestamp` (or `time`) text, or its
/// milliseconds in RFC 3339.
pub fn time(value: &Value) -> Option<String> {
    let at = value.get("timestamp").or_else(|| value.get("time"))?;
    if let Some(s) = at.as_str() {
        Some(s.to_owned())
    } else {
        at.as_i64()
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|at| at.to_rfc3339())
    }
}

/// A row's text as a page shows it: text as it is, the parts of a list one
/// per line, anything else as JSON.
pub fn text(value: Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text),
        Value::Array(items) => {
            let parts = items
                .into_iter()
                .filter_map(|item| match item {
                    Value::Null => None,
                    Value::String(text) => (!text.is_empty()).then_some(text),
                    Value::Object(mut map) => {
                        if let Some(text) = map.get("text").and_then(Value::as_str) {
                            return (!text.is_empty()).then(|| text.to_owned());
                        }
                        if let Some(content) = map.remove("content") {
                            if let Some(text) = self::text(content) {
                                return (!text.is_empty()).then_some(text);
                            }
                        }
                        if map.get("type").and_then(Value::as_str) == Some("image") {
                            return Some("[image]".to_owned());
                        }
                        if let Some(name) = map.get("tool_name").and_then(Value::as_str) {
                            return Some(format!("[tool: {name}]"));
                        }
                        Some(Value::Object(map).to_string())
                    }
                    other => Some(other.to_string()),
                })
                .collect::<Vec<_>>();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        other => Some(other.to_string()),
    }
}
