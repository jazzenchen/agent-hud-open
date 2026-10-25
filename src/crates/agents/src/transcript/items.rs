//! What one line of a client's transcript shows in a page of messages: the
//! part of the transcript behaviour (docs/agent-contract.md) that lists a
//! conversation. Each client states its own rule on its `Agent`; the engine
//! pages the lines and keeps their turns the same for all.
use serde_json::{json, Value};

/// Who a row speaks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineRole {
    User,
    Assistant,
    Tool,
    /// A role the line names in its own words (`toolResult`, `developer`…),
    /// kept as written.
    Other(String),
}

impl LineRole {
    pub fn new(name: &str) -> Self {
        match name {
            "user" => Self::User,
            "assistant" => Self::Assistant,
            "tool" => Self::Tool,
            other => Self::Other(other.into()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
            Self::Other(name) => name,
        }
    }
}

/// What a row holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Text,
    /// A tool call, or what a tool call returned.
    Tool,
    Thinking,
}

/// A tool call a row asks for.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub id: Option<String>,
    /// The tool's name; empty when the line names none.
    pub name: String,
    pub arguments: Value,
}

impl ToolCall {
    pub fn new(id: &Value, name: &Value, arguments: Option<&Value>) -> Self {
        Self {
            id: id.as_str().map(str::to_owned),
            name: name.as_str().unwrap_or_default().to_owned(),
            arguments: arguments.cloned().unwrap_or(Value::Null),
        }
    }
}

/// One row of a page of messages.
#[derive(Clone, Debug, PartialEq)]
pub struct LineItem {
    pub id: String,
    pub parent: Option<String>,
    pub role: LineRole,
    pub kind: LineKind,
    pub text: Option<String>,
    pub tool: Option<ToolCall>,
    /// The tool call this row asks for or answers.
    pub tool_call_id: Option<String>,
    /// When the line was written, as the line says it or in RFC 3339.
    pub timestamp: Option<String>,
    /// The user's own words: neither the context a client hands the model
    /// nor its note of an interruption.
    pub is_prompt: bool,
    /// Where the line says its words came from, where it says (Claude Code's
    /// `origin.kind`: `human`, `task-notification`, `peer`…).
    pub origin: Option<String>,
}

/// What every row of one line shares.
pub struct Line {
    pub parent: Option<String>,
    pub timestamp: Option<String>,
    /// Whether the line's user rows are the user's own words.
    pub prompt: bool,
    pub origin: Option<String>,
}

impl Line {
    pub fn of(value: &Value, prompt: bool) -> Self {
        Self {
            parent: None,
            timestamp: time(value),
            prompt,
            origin: value["origin"]["kind"].as_str().map(str::to_owned),
        }
    }

    /// A row of this line; `text` is read as a page shows it.
    pub fn item(
        &self,
        id: String,
        role: &str,
        text: Value,
        tool: Option<ToolCall>,
        call: &Value,
    ) -> LineItem {
        let tool_call_id = call.as_str().map(str::to_owned);
        let kind = if tool.is_some() || tool_call_id.is_some() {
            LineKind::Tool
        } else {
            LineKind::Text
        };
        LineItem {
            id,
            parent: self.parent.clone(),
            is_prompt: role == "user" && self.prompt,
            role: LineRole::new(role),
            kind,
            text: self::text(text),
            tool,
            tool_call_id,
            timestamp: self.timestamp.clone(),
            origin: self.origin.clone(),
        }
    }
}

pub struct Items {
    /// The rows the line at byte `offset` shows.
    pub read: fn(&Value, u64) -> Vec<LineItem>,
}

/// The rule for clients that state none of their own yet: every client's
/// rule in the order all transcripts were read before each client stated its
/// own, then the shapes no client states yet.
pub static SHARED: Items = Items { read: shared };

/// The rule of the client with this id, or the shared rule.
pub fn of(source: &str) -> &'static Items {
    crate::by_id(source)
        .and_then(|agent| agent.items)
        .unwrap_or(&SHARED)
}

fn shared(value: &Value, offset: u64) -> Vec<LineItem> {
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
                        serde_json::to_string(&Value::Object(map)).ok()
                    }
                    other => serde_json::to_string(&other).ok(),
                })
                .collect::<Vec<_>>();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        other => serde_json::to_string(&other).ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{claude::transcript as claude, codex::transcript as codex};

    fn claude_lines() -> Vec<Value> {
        vec![
            json!({"type":"user","uuid":"u1","parentUuid":null,"promptId":"p1","timestamp":"2026-10-25T01:00:00Z","message":{"role":"user","content":"Fix the build"}}),
            json!({"type":"assistant","uuid":"a1","parentUuid":"u1","timestamp":"2026-10-25T01:00:01Z","message":{"role":"assistant","content":[{"type":"text","text":"Looking."},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}),
            json!({"type":"user","uuid":"r1","parentUuid":"a1","timestamp":"2026-10-25T01:00:02Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"Cargo.toml"}]}]}}),
            json!({"type":"user","uuid":"i1","parentUuid":"r1","timestamp":"2026-10-25T01:00:03Z","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}),
            json!({"type":"api-request-blob","message":{"role":"user","content":"Fix the build"}}),
            json!({"type":"system","uuid":"s1","content":"note"}),
        ]
    }

    fn codex_lines() -> Vec<Value> {
        vec![
            json!({"type":"response_item","timestamp":"2026-10-25T01:00:00Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Fix the build"}]}}),
            json!({"type":"response_item","timestamp":"2026-10-25T01:00:00Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>cwd</environment_context>"}]}}),
            json!({"type":"response_item","timestamp":"2026-10-25T01:00:00Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<turn_aborted>stopped</turn_aborted>"}]}}),
            json!({"type":"response_item","timestamp":"2026-10-25T01:00:01Z","payload":{"type":"message","role":"assistant","id":"m1","content":[{"type":"output_text","text":"Looking."}]}}),
            json!({"type":"response_item","timestamp":"2026-10-25T01:00:02Z","payload":{"type":"function_call","call_id":"c1","name":"shell","arguments":"{\"cmd\":\"ls\"}"}}),
            json!({"type":"response_item","timestamp":"2026-10-25T01:00:03Z","payload":{"type":"function_call_output","call_id":"c1","output":"Cargo.toml"}}),
            json!({"type":"response_item","timestamp":"2026-10-25T01:00:03Z","payload":{"type":"reasoning","summary":[]}}),
            json!({"type":"event_msg","timestamp":"2026-10-25T01:00:04Z","payload":{"type":"agent_message","message":"Looking."}}),
            json!({"type":"message","role":"user","content":[{"type":"input_text","text":"Older rollout"}]}),
        ]
    }

    #[test]
    fn claude_rows_are_its_texts_calls_and_results() {
        let lines = claude_lines();
        let read = |n: usize| (claude::ITEMS.read)(&lines[n], n as u64 * 100);

        let prompt = read(0);
        assert_eq!(prompt.len(), 1);
        assert_eq!(prompt[0].id, "u1");
        assert_eq!(prompt[0].role, LineRole::User);
        assert_eq!(prompt[0].kind, LineKind::Text);
        assert_eq!(prompt[0].text.as_deref(), Some("Fix the build"));
        assert_eq!(prompt[0].timestamp.as_deref(), Some("2026-10-25T01:00:00Z"));
        assert!(prompt[0].is_prompt);

        let reply = read(1);
        assert_eq!(reply.len(), 2);
        assert_eq!(reply[0].id, "a1:0");
        assert_eq!(reply[0].parent.as_deref(), Some("u1"));
        assert_eq!(reply[0].text.as_deref(), Some("Looking."));
        assert_eq!(reply[1].kind, LineKind::Tool);
        assert_eq!(reply[1].tool_call_id.as_deref(), Some("t1"));
        let tool = reply[1].tool.as_ref().unwrap();
        assert_eq!(tool.name, "Bash");
        assert_eq!(tool.arguments, json!({"command":"ls"}));
        assert!(!reply.iter().any(|item| item.is_prompt));

        let result = read(2);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, LineRole::Tool);
        assert_eq!(result[0].kind, LineKind::Tool);
        assert_eq!(result[0].tool_call_id.as_deref(), Some("t1"));
        assert_eq!(result[0].text.as_deref(), Some("Cargo.toml"));

        let interruption = read(3);
        assert_eq!(interruption.len(), 1);
        assert_eq!(interruption[0].role, LineRole::User);
        assert!(!interruption[0].is_prompt);

        assert!(read(4).is_empty(), "a request copy shows nothing");
        assert!(read(5).is_empty());
    }

    #[test]
    fn codex_rows_are_its_response_items() {
        let lines = codex_lines();
        let read = |n: usize| (codex::ITEMS.read)(&lines[n], n as u64 * 100);

        let prompt = read(0);
        assert_eq!(prompt.len(), 1);
        assert_eq!(prompt[0].id, "o0");
        assert_eq!(prompt[0].role, LineRole::User);
        assert_eq!(prompt[0].text.as_deref(), Some("Fix the build"));
        assert!(prompt[0].is_prompt);

        assert!(!read(1)[0].is_prompt, "context is not the user's words");
        assert!(!read(2)[0].is_prompt, "an interruption note is no prompt");

        let reply = read(3);
        assert_eq!(reply[0].id, "m1");
        assert_eq!(reply[0].role, LineRole::Assistant);
        assert_eq!(reply[0].kind, LineKind::Text);

        let call = read(4);
        assert_eq!(call[0].kind, LineKind::Tool);
        assert_eq!(call[0].tool_call_id.as_deref(), Some("c1"));
        let tool = call[0].tool.as_ref().unwrap();
        assert_eq!(tool.name, "shell");
        assert_eq!(tool.arguments, json!("{\"cmd\":\"ls\"}"));

        let result = read(5);
        assert_eq!(result[0].role, LineRole::Tool);
        assert_eq!(result[0].tool_call_id.as_deref(), Some("c1"));
        assert_eq!(result[0].text.as_deref(), Some("Cargo.toml"));

        assert!(read(6).is_empty());
        assert!(read(7).is_empty(), "events show nothing");
        let older = read(8);
        assert_eq!(older[0].id, "o800:0");
        assert!(older[0].is_prompt);
    }

    #[test]
    fn a_request_copy_shows_nothing_to_any_rule() {
        let copy = json!({"type":"api-request","message":{"role":"user","content":"Hi"}});
        assert!((SHARED.read)(&copy, 0).is_empty());
        assert!((claude::ITEMS.read)(&copy, 0).is_empty());
        assert!((codex::ITEMS.read)(&copy, 0).is_empty());
    }

    #[test]
    fn the_shared_rule_reads_each_clients_lines_as_its_own_rule_does() {
        for (rule, lines) in [
            (&claude::ITEMS, claude_lines()),
            (&codex::ITEMS, codex_lines()),
        ] {
            for (n, line) in lines.iter().enumerate() {
                let offset = n as u64 * 100;
                assert_eq!(
                    (rule.read)(line, offset),
                    (SHARED.read)(line, offset),
                    "{line}"
                );
            }
        }
        assert!(std::ptr::eq(of("claude"), &claude::ITEMS));
        assert!(std::ptr::eq(of("codex"), &codex::ITEMS));
        assert!(std::ptr::eq(of("pi"), &SHARED));
    }
}
