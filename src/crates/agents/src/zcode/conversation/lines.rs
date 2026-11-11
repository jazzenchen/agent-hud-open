//! Reading a session's messages a page at a time: each message's data and
//! its parts, in the order of their place in the session.
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
