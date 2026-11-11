//! Claude Code transcript rules: which lines are prompts, turns and copies,
//! and the items its page shows.
use crate::transcript::{
    items::{self, Items, LineItem},
    turns::{has, TurnStart, Turns},
};
use serde_json::{json, Value};

pub use super::desktop_ask::{
    claude_desktop_ask, desktop_ask_view, desktop_ask_words, reworded_desktop_ask,
};
pub use super::desktop_index::{desktop_kind, ClaudeDesktopIndex, ClaudeDesktopProjectWorker};

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
        && (value["isMeta"] != true
            || value["origin"]["kind"] == "peer"
            || rewake_words(value).is_some())
        && value["isCompactSummary"] != true
        && !parts.iter().any(|part| part["type"] == "tool_result")
        && !text
            .is_some_and(|s| s.starts_with("<command-name>") || s.starts_with("<local-command-"))
}

/// The words Agent HUD's rewake hook handed a session, from the notification
/// its client wrote for them. Qoder CLI 1.1.86 writes an `isMeta` user line
/// of `origin.kind` `task-notification` whose system reminder reads `Stop
/// hook blocking error from command "<Agent HUD> --rewake-hook <client>":
/// <words>`. None for any other line, a background task's notice included.
pub fn rewake_words(value: &Value) -> Option<&str> {
    if value["origin"]["kind"] != "task-notification" {
        return None;
    }
    let content = value["message"]["content"].as_str()?;
    let flag = content.find(" --rewake-hook ")?;
    let start = flag + content[flag..].find("\": ")? + 3;
    let end = content.rfind("</system-reminder>")?;
    content.get(start..end).filter(|words| !words.is_empty())
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
    // A message another session or Agent HUD handed over shows as its words.
    let body = if value["origin"]["kind"] == "peer" {
        value["origin"]["body"].as_str().filter(|s| !s.is_empty())
    } else {
        rewake_words(value)
    };
    if let (true, Some(body)) = (prompt, body) {
        for row in &mut items {
            if row.is_prompt {
                row.text = Some(body.to_owned());
            }
        }
    }
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

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
