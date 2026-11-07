//! Reading a session's conversation: where a message landed, whether its turn
//! ended, the turn's last reply, and what began a caller's turn. The answers
//! come from a product's store of turns (`Turns`) where it holds the client;
//! otherwise, and for DeepSeek Harness, whose conversations a store is not
//! given, from the app's `session_detail` pages, the way the phone's
//! conversation page reads them. Nothing read here is kept.
mod paged;
mod stored;

pub use stored::{StoredTurn, Turns};

use super::{journal::Message, journal::Turn, Caller, Host};
use agent_hud_platform::{claude_plugin::unwrap_prompt, delivery::PromptForm};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub fn fingerprint(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.trim().as_bytes()))
}

/// Where the session's transcript ends now; None when it cannot be read.
pub fn end(query: &dyn Fn(&str, Value) -> super::Result<Value>, id: &str) -> Option<u64> {
    query(
        "session_detail",
        json!({"id":id,"messageTail":true,"messageLimit":1,"callLimit":1}),
    )
    .ok()?["endMessageCursor"]
        .as_u64()
}

/// Whether `row` is the words `message` delivered.
pub fn is_message(row: &Value, message: &Message) -> bool {
    row["role"] == "user"
        && row["text"]
            .as_str()
            .is_some_and(|text| is_words(text, message))
}

/// Whether `text`, a prompt as its log holds it, is the words `message`
/// delivered: by its label, or by its fingerprint once Agent HUD's Claude
/// plugin wrapper is taken off.
fn is_words(text: &str, message: &Message) -> bool {
    match message.form {
        PromptForm::Labelled => {
            text.contains(&format!("· {}]", message.id))
                || text.contains(&format!("· {} ·", message.id))
        }
        PromptForm::Own => {
            fingerprint(text) == message.fingerprint
                || fingerprint(unwrap_prompt(text)) == message.fingerprint
        }
    }
}

/// How far back of a send a recipient's turn may say it began, for clocks a
/// little apart.
pub(crate) const SLACK_MS: i64 = 5_000;

/// The store, for a client whose conversations it holds: Agent HUD hands a
/// store no DeepSeek Harness items, so those are read from pages.
pub(crate) fn stored<'a>(host: &Host<'a>, client: &str) -> Option<&'a dyn Turns> {
    host.conversations
        .filter(|_| matches!(client, "claude" | "codex" | "antigravity"))
}

/// The caller's current prompt, as `is_message` reads it.
pub fn prompt_row(text: &str) -> Value {
    json!({"role":"user","text":text})
}

/// The words that began the caller's current turn. Some(None) when its turn
/// began with no words this can read; None when it cannot tell which turn is
/// the caller's.
pub fn current_prompt(host: &Host<'_>, caller: &Caller) -> Option<Option<Value>> {
    match stored(host, &caller.client) {
        Some(store) => stored::current(store, host.snapshot, caller),
        None => paged::current(host.query, caller),
    }
}

pub struct Found {
    pub turn: Turn,
    pub done: bool,
    pub answer: Option<String>,
    pub ended_at: Option<Value>,
}

/// Where `message` landed in its recipient, and the answer once its turn
/// ended. None while the conversation does not show it yet.
pub fn answer(host: &Host<'_>, message: &Message) -> Option<Found> {
    match stored(host, &message.to.client) {
        Some(store) => stored::answer(store, host.snapshot, message),
        None => paged::answer(host, message),
    }
}

/// Whether a row of turn `theirs`, read after turn `ours`, is of a later
/// turn: another turn's id, unless both are numbers (DeepSeek Harness counts
/// its turns) and it is not larger. A row that names no turn says nothing.
fn after(ours: Option<&str>, theirs: Option<&str>) -> bool {
    match (ours, theirs) {
        (Some(ours), Some(theirs)) if ours != theirs => {
            match (ours.parse::<u64>(), theirs.parse::<u64>()) {
                (Ok(ours), Ok(theirs)) => theirs > ours,
                _ => true,
            }
        }
        _ => false,
    }
}

pub fn at(time: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(time)
        .map(|at| at.timestamp_millis())
        .unwrap_or(0)
}
