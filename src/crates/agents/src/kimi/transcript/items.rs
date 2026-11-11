//! The rows a page of a Kimi session's messages shows (`ITEMS`), read from
//! each line of its wire.
use crate::transcript::items::{Items, Line, LineItem, ToolCall};
use serde_json::{json, Value};

/// What a Kimi wire line shows in a page of messages: `turn.prompt` for the
/// words that start a turn, `turn.steer` for words typed into a running one,
/// the user message a Stop hook hands Kimi to carry on with, and
/// `context.append_loop_event` for assistant text, tool calls and tool
/// results. Each agent writes its own wire, so a sub-agent's log holds only
/// its own lines.
pub static ITEMS: Items = Items { read: items };

fn items(value: &Value, offset: u64) -> Vec<LineItem> {
    match value["type"].as_str() {
        Some(kind @ ("turn.prompt" | "turn.steer")) => {
            let words = words(&value["input"]);
            if words.is_empty() {
                return Vec::new();
            }
            let id = if kind == "turn.prompt" {
                value["promptId"].as_str()
            } else {
                value["messageId"]
                    .as_str()
                    .or_else(|| value["promptIds"][0].as_str())
            }
            .map(str::to_owned)
            .unwrap_or_else(|| format!("o{offset}"));
            let line = Line::of(value, prompted(&value["origin"]));
            vec![line.item(id, "user", json!(words), None, &Value::Null)]
        }
        // Kimi copies every prompt and steer into its context too; only what a
        // Stop hook asked for is there alone.
        Some("context.append_message") => {
            let message = &value["message"];
            let origin = &message["origin"];
            if message["role"] != "user"
                || origin["kind"] != "system_trigger"
                || origin["name"] != "stop_hook"
            {
                return Vec::new();
            }
            let words = words(&message["content"]);
            if words.is_empty() {
                return Vec::new();
            }
            let id = message["id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("o{offset}"));
            let line = Line {
                parent: None,
                timestamp: crate::transcript::items::time(value),
                prompt: true,
                origin: origin["kind"].as_str().map(str::to_owned),
            };
            vec![line.item(id, "user", json!(words), None, &Value::Null)]
        }
        Some("context.append_loop_event") => {
            let event = &value["event"];
            let id = event["uuid"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("o{offset}"));
            let line = Line::of(value, false);
            match event["type"].as_str() {
                Some("content.part") if event["part"]["type"] == "text" => {
                    let Some(text) = event["part"]["text"].as_str().filter(|s| !s.is_empty())
                    else {
                        return Vec::new();
                    };
                    vec![line.item(id, "assistant", json!(text), None, &Value::Null)]
                }
                Some("tool.call") => vec![line.item(
                    id,
                    "assistant",
                    Value::Null,
                    Some(ToolCall::new(
                        &event["toolCallId"],
                        &event["name"],
                        event.get("args"),
                    )),
                    &event["toolCallId"],
                )],
                Some("tool.result") => {
                    let output = event["result"]
                        .get("output")
                        .unwrap_or(&event["result"])
                        .clone();
                    vec![line.item(id, "tool", output, None, &event["toolCallId"])]
                }
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

/// Whether words of this origin are a prompt: the person's own (no origin,
/// or `user`), a sub-agent's task in its own log, or what a Stop hook hands
/// Kimi to carry on with, as Agent HUD delivers other agents' messages.
/// Kimi's own reminders and goal continuations are not.
fn prompted(origin: &Value) -> bool {
    match origin["kind"].as_str() {
        None | Some("user") => true,
        Some("system_trigger") => matches!(origin["name"].as_str(), Some("subagent" | "stop_hook")),
        Some(_) => false,
    }
}

fn words(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_owned();
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").is_none_or(|kind| kind == "text"))
        .filter_map(|block| block["text"].as_str())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
