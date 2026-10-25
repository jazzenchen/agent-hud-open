//! What a collection pass learns about conversations, handed to a store that
//! keeps it (docs: root `docs/proposals/conversation-model.md`). Core keeps no
//! conversation text itself: it reads each log once, for its tokens and its
//! items together, and passes the items on after the log's ledger
//! transaction commits.
use std::{collections::HashMap, path::PathBuf, sync::Arc};

/// Who wrote an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// The user's words that reached a running turn (the prompt itself is
    /// `Fact::TurnStarted`).
    User,
    Assistant,
}

/// What an item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Tool,
    Thinking,
}

/// One unit the agent or the user wrote inside a turn.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Item {
    /// The turn it belongs to, as its log names the turn; None before the
    /// log's first prompt.
    pub turn: Option<String>,
    /// The client's own id for it, or `o<offset>` where its log gives none.
    pub id: String,
    /// Where it is in its log: the line's byte offset times 64 plus the
    /// block's index within the line, so items sort in the order written.
    pub order: u64,
    pub role: Role,
    pub kind: Kind,
    /// Text items: the words. Tool items: the tool's name. Thinking: what the
    /// log holds (Claude logs none).
    pub text: String,
    /// When the line says it was written, in Unix milliseconds.
    pub at_ms: Option<i64>,
}

/// A change in one session's conversation, in the order its log holds it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Fact {
    /// A prompt started a turn.
    TurnStarted {
        turn: String,
        /// The user's words, where the prompt line holds them.
        prompt: Option<String>,
        /// Where the prompt came from, where the client says: Claude Code
        /// names `human` for what the user typed and `task-notification`
        /// for a background task's notice, among others.
        origin: Option<String>,
        at_ms: Option<i64>,
    },
    Item(Item),
    /// The agent stopped: `outcome` as the session's live facts name it
    /// (completed, interrupted, failed, limited).
    TurnEnded {
        turn: String,
        at_ms: Option<i64>,
        outcome: Option<String>,
    },
}

/// What one read of one log yielded, up to `end_offset`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    pub source: String,
    pub session: String,
    pub path: PathBuf,
    /// The byte offset in `path` the facts reach: a store that holds this
    /// batch holds everything before it.
    pub end_offset: u64,
    pub facts: Vec<Fact>,
}

/// A store that keeps conversations. Core calls it from its collection
/// thread, so `deliver` must not block on anything but its own storage.
pub trait ConversationSink: Send + Sync {
    /// Keeps a batch. An error leaves the store's position where it was, and
    /// core replays from there on a later pass.
    fn deliver(&self, batch: Batch) -> Result<(), String>;
    /// Where the store's knowledge of each log ends, by path. A log absent
    /// here is one the store has never seen.
    fn positions(&self) -> Result<HashMap<PathBuf, u64>, String>;
}

pub type Sink = Arc<dyn ConversationSink>;
