//! A product's store of conversation turns: the caller's turn and a
//! message's turn read from it instead of from transcript pages.
use super::{at, is_words, prompt_row, Caller, Found, Message, Turn, SLACK_MS};
use serde_json::Value;

/// Turns read in one look at the store.
const TURNS: usize = 50;

/// A store of conversation turns a product keeps from what Agent HUD
/// collects (`ProductExtension::conversation_sink`). Agent HUD keeps none of
/// its own: without one, every answer is read from transcript pages.
pub trait Turns: Send + Sync {
    /// One session's turns, oldest first: the last `limit` turns when
    /// `since_ms` is None, else the first `limit` that started at or after
    /// `since_ms`. With `replies`, each turn's assistant text too. None when
    /// the store cannot be read.
    fn session_turns(
        &self,
        client: &str,
        session: &str,
        since_ms: Option<i64>,
        limit: usize,
        replies: bool,
    ) -> Option<Vec<StoredTurn>>;
}

/// One turn as the store keeps it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoredTurn {
    pub id: String,
    /// The words that began it, where the prompt line holds them.
    pub prompt: Option<String>,
    pub started_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
    /// The assistant's text items, in order (only when asked for).
    pub replies: Vec<String>,
}

fn turns_for(
    store: &dyn Turns,
    client: &str,
    id: &str,
    since_ms: Option<i64>,
    limit: usize,
    replies: bool,
) -> Option<Vec<StoredTurn>> {
    let turns = store.session_turns(client, id, since_ms, limit, replies)?;
    if !turns.is_empty() || client != "antigravity" {
        return Some(turns);
    }
    let alt = match id.strip_prefix("antigravity:") {
        Some(stripped) => stripped.to_owned(),
        None => format!("antigravity:{id}"),
    };
    store.session_turns(client, &alt, since_ms, limit, replies)
}

/// The caller's turn from the store: the one its client named, or its latest.
/// A latest turn older than the one the collector's live facts show is one the
/// store has not caught up with, so it says nothing.
pub(super) fn current(
    store: &dyn Turns,
    snapshot: &Value,
    caller: &Caller,
) -> Option<Option<Value>> {
    let turns = turns_for(store, &caller.client, &caller.session_id, None, 5, false)?;
    let turn = match &caller.turn_id {
        Some(id) => turns.iter().rev().find(|turn| turn.id == *id)?,
        None => {
            let turn = turns.last()?;
            let live = crate::find(snapshot, &caller.session_id)
                .and_then(|session| session["liveFacts"]["turnStartedAt"].as_str())
                .map(at);
            if live
                .zip(turn.started_at_ms)
                .is_some_and(|(live, ours)| live > ours + SLACK_MS)
            {
                return None;
            }
            turn
        }
    };
    Some(turn.prompt.as_deref().map(prompt_row))
}

/// The message's turn among the recipient's turns that began after it was
/// sent: the one the journal named, else the first whose prompt is its words.
/// It is done once the store saw it end, a later turn began, or the
/// collector's live facts say its latest turn, ours, ended.
pub(super) fn answer(store: &dyn Turns, snapshot: &Value, message: &Message) -> Option<Found> {
    let id = &message.to.session_id;
    let turns = turns_for(
        store,
        &message.to.client,
        id,
        Some(message.sent_at_ms - SLACK_MS),
        TURNS,
        true,
    )?;
    let known = message.turn.as_ref().and_then(|turn| turn.id.as_deref());
    let index = known
        .and_then(|known| turns.iter().position(|turn| turn.id == known))
        .or_else(|| {
            turns.iter().position(|turn| {
                turn.prompt
                    .as_deref()
                    .is_some_and(|prompt| is_words(prompt, message))
            })
        })?;
    let ours = &turns[index];
    let later = index + 1 < turns.len();
    let live = crate::find(snapshot, id).map(|session| &session["liveFacts"]);
    let live_end = live
        .map(|live| &live["endedAt"])
        .filter(|at| !at.is_null())
        .cloned();
    let latest = live.is_some_and(|live| match live["turnId"].as_str() {
        Some(turn) => turn == ours.id,
        None => live["turnStartedAt"]
            .as_str()
            .zip(ours.started_at_ms)
            .is_some_and(|(theirs, ours)| at(theirs) >= ours),
    });
    let stored_end = ours.ended_at_ms.and_then(|ms| {
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
            .map(|at| Value::String(at.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)))
    });
    let done = later || ours.ended_at_ms.is_some() || (latest && live_end.is_some());
    let answer = ours
        .replies
        .iter()
        .rev()
        .find(|text| !text.trim().is_empty())
        .cloned();
    Some(Found {
        turn: Turn {
            id: Some(ours.id.clone()),
            cursor: message
                .turn
                .as_ref()
                .map(|turn| turn.cursor)
                .or(message.cursor)
                .unwrap_or(0),
        },
        done,
        answer: if done { answer } else { None },
        ended_at: if !done {
            None
        } else if latest && live_end.is_some() {
            live_end
        } else {
            stored_end
        },
    })
}
