//! A ZCode session's conversation, from the `message` and `part` tables of
//! its database. Each message is one line of the session: its `data` as
//! `info` and its parts in order, read with ZCode's own rules for turns
//! (`TURNS`) and rows (`ITEMS`). A message's place is its `sequence`, which
//! ZCode numbers from 0 in each session (zcode.cjs 0.15, migration
//! `0014_message_part_sequence`); an older database is read by row.
//!
//! What ZCode shows of a message is in its `semantics` (since 0.15): the
//! user's own words are a `user_prompt` or `slash_command` of the
//! `real_user`; the context it hands the model (`system_reminder`, with the
//! environment, the date, the skills) is `uiVisibility: hidden`. A message
//! of an older ZCode with no semantics is shown unless it is `synthetic` or
//! for the model only (`visibility: model-only`).
//!
//! What a Stop hook adds to continue a turn is not kept here: ZCode hands it
//! to the model as a `hook_context` attachment it holds in memory only
//! (`injectHookAdditionalContextIntoMessageHistory`), so the turn's further
//! replies follow its earlier ones with no prompt between them.
use crate::transcript::items::{Items, LineItem, LineKind, LineRole, ToolCall};
use crate::transcript::turns::{has, TurnStart, Turns};
use crate::transcript::{sql_columns, sql_text};
use rusqlite::Connection;
use serde_json::{json, Value};

/// About as much message data as a page of a transcript file reads.
const BUDGET: usize = 2 * 1024 * 1024;

/// A page of a session's messages, each one line that `ITEMS` and `TURNS`
/// read.
#[derive(Debug)]
pub struct Lines {
    /// Each message at its place in the session.
    pub lines: Vec<(u64, Value)>,
    /// Where the page after this one starts.
    pub next: u64,
    /// No message comes after this page.
    pub complete: bool,
    /// A message comes before this page.
    pub earlier: bool,
}

/// Session `id`'s messages from place `from` on: at most `limit` of them,
/// and fewer when their data passes about 2 MiB. `id` is ZCode's own or
/// Agent HUD's (`zcode:<id>`).
pub fn messages(db: &Connection, id: &str, from: u64, limit: usize) -> rusqlite::Result<Lines> {
    read(db, id, Some(from), limit)
}

/// Session `id`'s last `limit` messages, fewer when their data passes about
/// 2 MiB.
pub fn last_messages(db: &Connection, id: &str, limit: usize) -> rusqlite::Result<Lines> {
    read(db, id, None, limit)
}

fn read(db: &Connection, id: &str, from: Option<u64>, limit: usize) -> rusqlite::Result<Lines> {
    let id = id.strip_prefix("zcode:").unwrap_or(id);
    let columns = sql_columns(db, "message")?;
    if !["id", "session_id", "data"]
        .iter()
        .all(|key| columns.contains(*key))
    {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let place = if columns.contains("sequence") {
        "m.sequence"
    } else {
        "m.rowid"
    };
    let mut keys = vec![place];
    if columns.contains("time_created") {
        keys.push("m.time_created");
    }
    keys.push("m.id");
    let (filter, order) = match from {
        Some(_) => (format!("AND {place}>=?3"), keys.join(", ")),
        // The last messages, newest first.
        None => (
            "AND ?3 IS NULL".to_owned(),
            keys.iter()
                .map(|key| format!("{key} DESC"))
                .collect::<Vec<_>>()
                .join(", "),
        ),
    };
    let sql = format!(
        "SELECT m.id, {place}, m.data FROM message m WHERE m.session_id=?1 {filter} ORDER BY {order} LIMIT ?2"
    );
    let mut statement = db.prepare(&sql)?;
    let take = limit.max(1) as i64;
    let mut rows = statement.query(rusqlite::params![id, take + 1, from.map(|f| f as i64)])?;
    let mut found = Vec::new();
    while let Some(row) = rows.next()? {
        found.push((sql_text(row, 0), row.get::<_, i64>(1)?, sql_text(row, 2)));
    }
    // One more than asked says whether more come after (or, for the last
    // messages, before).
    let more = found.len() as i64 > take;
    found.truncate(take as usize);
    let mut parts = parts(db)?.map(|sql| db.prepare(&sql)).transpose()?;
    let mut lines = Vec::new();
    let mut spent = 0;
    let mut cut = None;
    for (message, at, data) in found {
        if spent >= BUDGET && !lines.is_empty() {
            cut = Some(at);
            break;
        }
        let Some(message) = message else {
            continue;
        };
        let data = data.unwrap_or_default();
        spent += data.len();
        let mut list = Vec::new();
        if let Some(statement) = parts.as_mut() {
            let mut rows = statement.query([&message])?;
            while let Some(row) = rows.next()? {
                let text = sql_text(row, 1).unwrap_or_default();
                spent += text.len();
                let Ok(Value::Object(mut part)) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                if let Some(id) = sql_text(row, 0) {
                    part.insert("id".into(), json!(id));
                }
                list.push(Value::Object(part));
            }
        }
        let info = serde_json::from_str::<Value>(&data).unwrap_or(Value::Null);
        lines.push((
            at.max(0) as u64,
            json!({"id": message, "info": info, "parts": list}),
        ));
    }
    if from.is_none() {
        lines.reverse();
    }
    let first = lines
        .first()
        .map(|(at, _)| *at as i64)
        .or(from.map(|f| f as i64));
    let earlier = match first {
        Some(first) => db.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM message m WHERE m.session_id=?1 AND {place}<?2)"),
            rusqlite::params![id, first],
            |row| row.get::<_, bool>(0),
        )?,
        None => false,
    };
    let next = match (from, cut) {
        (Some(_), Some(cut)) => cut.max(0) as u64,
        _ => lines
            .last()
            .map(|(at, _)| at + 1)
            .unwrap_or(from.unwrap_or(0)),
    };
    Ok(Lines {
        complete: from.is_none() || (!more && cut.is_none()),
        earlier,
        lines,
        next,
    })
}

/// How a message's parts are read in order, when the database has them.
fn parts(db: &Connection) -> rusqlite::Result<Option<String>> {
    let columns = sql_columns(db, "part")?;
    if !["id", "message_id", "data"]
        .iter()
        .all(|key| columns.contains(*key))
    {
        return Ok(None);
    }
    let mut order = Vec::new();
    for key in ["sequence", "time_created"] {
        if columns.contains(key) {
            order.push(format!("p.{key}"));
        }
    }
    order.push("p.id".into());
    Ok(Some(format!(
        "SELECT p.id, p.data FROM part p WHERE p.message_id=?1 ORDER BY {}",
        order.join(", ")
    )))
}

/// A user message's or a reply's turn, as ZCode anchors each message of
/// one; the user's own words start it.
pub static TURNS: Turns = Turns {
    start: turn,
    may_name: |line| has(line, b"turnId"),
};

fn turn(value: &Value) -> Option<TurnStart> {
    let info = &value["info"];
    let turn = info["anchor"]["turnId"].as_str()?.to_owned();
    if info["role"] == "user" && prompt(info) {
        let started = millis(&info["time"]["created"]).map(|at| json!(at));
        return Some(TurnStart::whole(Some(turn), started.as_ref()));
    }
    Some(TurnStart {
        turn: Some(turn),
        started: None,
        whole: false,
    })
}

/// What a ZCode message shows: the user's words and the notices ZCode shows
/// them, the replies' text, thinking and tool calls with what each returned.
pub static ITEMS: Items = Items { read: items };

fn items(value: &Value, place: u64) -> Vec<LineItem> {
    let info = &value["info"];
    if !shown(info) {
        return Vec::new();
    }
    let id = value["id"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("o{place}"));
    let created = millis(&info["time"]["created"]);
    let parts = value["parts"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let mut rows = Vec::new();
    match info["role"].as_str() {
        Some("user") => {
            let is_prompt = prompt(info);
            let origin = if is_prompt {
                Some("human".to_owned())
            } else {
                info["semantics"]["kind"].as_str().map(str::to_owned)
            };
            for (index, part) in parts.iter().enumerate() {
                if part["type"] != "text" || part["metadata"]["visibility"] == "model-only" {
                    continue;
                }
                let Some(text) = words(&part["text"]) else {
                    continue;
                };
                rows.push(LineItem {
                    id: part_id(part, &id, index),
                    parent: None,
                    role: LineRole::User,
                    kind: LineKind::Text,
                    text: Some(text),
                    tool: None,
                    tool_call_id: None,
                    timestamp: millis(&part["time"]["start"]).or_else(|| created.clone()),
                    is_prompt,
                    origin: origin.clone(),
                });
            }
        }
        Some("assistant") => {
            let parent = info["parentID"].as_str().map(str::to_owned);
            let row = |id: String, kind, text, timestamp: Option<String>| LineItem {
                id,
                parent: parent.clone(),
                role: LineRole::Assistant,
                kind,
                text,
                tool: None,
                tool_call_id: None,
                timestamp: timestamp.or_else(|| created.clone()),
                is_prompt: false,
                origin: None,
            };
            for (index, part) in parts.iter().enumerate() {
                let key = part_id(part, &id, index);
                let at = millis(&part["time"]["start"]);
                match part["type"].as_str() {
                    Some("text") if part["synthetic"] != true => {
                        if let Some(text) = words(&part["text"]) {
                            rows.push(row(key, LineKind::Text, Some(text), at));
                        }
                    }
                    Some("reasoning") => {
                        if let Some(text) = words(&part["text"]) {
                            rows.push(row(key, LineKind::Thinking, Some(text), at));
                        }
                    }
                    Some("tool") => {
                        let state = &part["state"];
                        let call = part["callID"].as_str().map(str::to_owned);
                        let mut asked = row(
                            key.clone(),
                            LineKind::Tool,
                            None,
                            millis(&state["time"]["start"]).or(at),
                        );
                        asked.tool = Some(ToolCall {
                            id: call.clone(),
                            name: part["tool"].as_str().unwrap_or_default().to_owned(),
                            arguments: state["input"].clone(),
                        });
                        asked.tool_call_id = call.clone();
                        rows.push(asked);
                        let answer = match state["status"].as_str() {
                            Some("completed") => &state["output"],
                            Some("error") => &state["error"],
                            _ => &Value::Null,
                        };
                        if let Some(text) = crate::transcript::items::text(answer.clone()) {
                            let mut result = row(
                                format!("{key}:result"),
                                LineKind::Tool,
                                Some(text),
                                millis(&state["time"]["end"]),
                            );
                            result.role = LineRole::Tool;
                            result.tool_call_id = call;
                            rows.push(result);
                        }
                    }
                    _ => {}
                }
            }
            // A call that failed before it said anything says why.
            if rows.is_empty() {
                let error = &info["error"];
                if let Some(text) =
                    words(&error["data"]["message"]).or_else(|| words(&error["message"]))
                {
                    rows.push(row(format!("{id}:error"), LineKind::Text, Some(text), None));
                }
            }
        }
        _ => {}
    }
    rows
}

/// Whether ZCode shows the message in its conversation. A timeline event (a
/// change of model) is a separator, not a message.
fn shown(info: &Value) -> bool {
    let semantics = &info["semantics"];
    if semantics.is_object() {
        return semantics["uiVisibility"] == "visible" && semantics["kind"] != "timeline_event";
    }
    info["visibility"] != "model-only" && info["synthetic"] != true
}

/// Whether a user message holds the user's own words.
fn prompt(info: &Value) -> bool {
    let semantics = &info["semantics"];
    if semantics.is_object() {
        return semantics["origin"] == "real_user"
            && matches!(
                semantics["kind"].as_str(),
                Some("user_prompt" | "slash_command")
            );
    }
    info["role"] == "user" && shown(info)
}

fn part_id(part: &Value, message: &str, index: usize) -> String {
    part["id"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{message}:{index}"))
}

fn words(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Milliseconds as RFC 3339.
fn millis(value: &Value) -> Option<String> {
    value.as_i64().filter(|at| *at > 0).and_then(crate::iso)
}

#[cfg(test)]
#[path = "conversation_tests.rs"]
mod tests;
