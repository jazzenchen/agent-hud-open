//! What one line of a client's transcript shows in a page of messages: the
//! part of the transcript behaviour (docs/agent-contract.md) that lists a
//! conversation. Each client states its own rule on its `Agent`; the engine
//! pages the lines and keeps their turns the same for all.
use super::shared_items::shared;
use serde_json::Value;

pub use super::shared_items::{message, text, time};

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

#[cfg(test)]
#[path = "items_tests.rs"]
mod tests;
