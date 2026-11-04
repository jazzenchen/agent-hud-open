//! Copilot CLI's session store, `session-store.db`: one row per model call in
//! `assistant_usage_events`, the only place on disk that keeps a call's
//! tokens (the `assistant.usage` events behind it are never written to
//! `events.jsonl`). Its layout is tokscale's (`copilot_session_store.rs`).
//!
//! Its `input_tokens` holds the cache's reads and writes, as Copilot's own
//! input counts do (CLI 1.0.51: "input token usage includes cached"), and its
//! `output_tokens` the reasoning; each is kept apart here. A row is the
//! session's call wherever else Copilot reported it: the session takes its
//! tokens from here once a row of it is read, over the OTel export and the
//! `session.shutdown` totals (`Log::copilot_store_counts`).
use crate::transcript::{sql_column, sql_columns, sql_count, sql_text};
use crate::{
    transcript::records::{Page, Record},
    Log, Usage,
};
use rusqlite::{types::ValueRef, Connection};
use serde_json::{json, Value};
use std::path::Path;

/// Whether `path` is the session store.
pub fn accepts(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == "session-store.db")
}

/// What a store row does to the session it names.
#[derive(Debug, PartialEq, Eq)]
pub enum Counts {
    /// It counts beside the session's other store rows.
    Add,
    /// It is the session's first: what other sources counted gives way.
    Replace,
}

impl Log {
    /// A store row read for this session: its first replaces what the OTel
    /// export or `session.shutdown` counted, and marks the session so those
    /// count no more.
    pub fn copilot_store_counts(&mut self) -> Counts {
        if self.source_state["hasStore"] == true {
            return Counts::Add;
        }
        for flag in ["hasStore", "hasInference", "hasOtel"] {
            self.source_state[flag] = json!(true);
        }
        Counts::Replace
    }

    /// Whether a call from the OTel export still counts: not once the store
    /// counts the session.
    pub fn copilot_otel_counts(&self) -> bool {
        self.source_state["hasStore"] != true
    }
}

/// A page of calls after the cursor's row; a row with no tokens is left out.
pub fn read(db: &Connection, cursor: &Value) -> rusqlite::Result<Page> {
    let events = sql_columns(db, "assistant_usage_events")?;
    if ![
        "id",
        "session_id",
        "input_tokens",
        "output_tokens",
        "created_at",
    ]
    .iter()
    .all(|key| events.contains(*key))
    {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let optional = [
        "copilot_usage_model",
        "model",
        "cache_read_tokens",
        "cache_write_tokens",
        "reasoning_tokens",
        "duration_ms",
    ]
    .map(|key| sql_column(&events, "e", key, "NULL"));
    let sessions = sql_columns(db, "sessions").unwrap_or_default();
    let join = sessions.contains("id");
    let cwd = if join {
        sql_column(&sessions, "s", "cwd", "NULL")
    } else {
        "NULL".into()
    };
    let after = cursor["after"].as_i64().unwrap_or(0);
    let sql = format!(
        "SELECT e.id,e.session_id,{},{},e.input_tokens,e.output_tokens,{},{},{},{},e.created_at,{cwd} \
         FROM assistant_usage_events e {} WHERE e.id>?1 ORDER BY e.id LIMIT 1000",
        optional[0],
        optional[1],
        optional[2],
        optional[3],
        optional[4],
        optional[5],
        if join {
            "LEFT JOIN sessions s ON s.id=e.session_id"
        } else {
            ""
        }
    );
    let mut statement = db.prepare(&sql)?;
    let mut rows = statement.query([after])?;
    let mut records = Vec::new();
    let mut last = after;
    let mut seen = 0;
    while let Some(row) = rows.next()? {
        seen += 1;
        last = row.get(0)?;
        let (Some(session), Some(at)) = (sql_text(row, 1), time(row.get_ref(10)?)) else {
            continue;
        };
        let input = sql_count(row, 4)?;
        let output = sql_count(row, 5)?;
        let read = sql_count(row, 6)?.min(input);
        let write = sql_count(row, 7)?.min(input - read);
        let reasoning = sql_count(row, 8)?.min(output);
        let usage = Usage {
            input: input - read - write,
            output: output - reasoning,
            read,
            write,
            reasoning,
        };
        if usage.total() == 0 {
            continue;
        }
        let model = sql_text(row, 2)
            .or_else(|| sql_text(row, 3))
            .unwrap_or_else(|| "Unknown".into());
        let id = format!("copilot:{session}");
        records.push(Record {
            key: format!("{id}:store:{last}"),
            id,
            title: None,
            title_fallback: None,
            cwd: sql_text(row, 11),
            model,
            at,
            usage,
            parent: None,
            archived: Some(false),
            updated: None,
            duration_ms: match row.get_ref(9)? {
                ValueRef::Integer(ms) if ms > 0 => Some(ms as u64),
                _ => None,
            },
        });
    }
    Ok(Page {
        records,
        cursor: (seen == 1000).then(|| json!({"after": last})),
        watermark: (seen < 1000).then(|| json!({"after": last})),
    })
}

/// `created_at`: RFC 3339, SQLite's `YYYY-MM-DD HH:MM:SS[.fff]` in UTC, or
/// a number of seconds or milliseconds.
fn time(value: ValueRef<'_>) -> Option<i64> {
    let number = match value {
        ValueRef::Integer(n) => n as f64,
        ValueRef::Real(n) => n,
        ValueRef::Text(bytes) => {
            let text = std::str::from_utf8(bytes).ok()?.trim();
            if let Some(at) = crate::transcript::metering::date_ms(text) {
                return Some(at);
            }
            if let Ok(at) = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f") {
                return Some(at.and_utc().timestamp_millis());
            }
            text.parse().ok()?
        }
        _ => return None,
    };
    let ms = if number < 1e10 {
        number * 1000.0
    } else {
        number
    };
    (number.is_finite() && number > 0.0).then_some(ms as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// tokscale's layout of the store, which Copilot CLI writes.
    fn store() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE sessions(id TEXT PRIMARY KEY,cwd TEXT,repository TEXT,host_type TEXT,
               branch TEXT,summary TEXT,created_at TEXT,updated_at TEXT);
             CREATE TABLE assistant_usage_events(id INTEGER PRIMARY KEY AUTOINCREMENT,
               session_id TEXT,turn_index INTEGER,model TEXT,copilot_usage_model TEXT,
               input_tokens INTEGER,output_tokens INTEGER,cache_read_tokens INTEGER,
               cache_write_tokens INTEGER,reasoning_tokens INTEGER,total_nano_aiu INTEGER,
               duration_ms INTEGER,created_at TEXT);
             INSERT INTO sessions(id,cwd,created_at) VALUES('s1','/w/repo','2026-10-18 08:00:00');
             INSERT INTO assistant_usage_events(session_id,turn_index,model,copilot_usage_model,
               input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,reasoning_tokens,
               total_nano_aiu,duration_ms,created_at)
             VALUES('s1',0,'gpt-5.4-mini',NULL,21343,100,0,20974,0,556570000,1234,'2026-10-18 08:00:05'),
                   ('s1',1,'auto','claude-sonnet-4.6',30000,500,25000,1000,120,1,NULL,'2026-10-18T08:01:00.000Z'),
                   ('s2',0,'gpt-5',NULL,0,0,0,0,0,0,NULL,'2026-10-18 08:02:00');",
        )
        .unwrap();
        db
    }

    #[test]
    fn each_call_keeps_its_cache_and_reasoning_apart() {
        let page = read(&store(), &Value::Null).unwrap();
        let got = page
            .records
            .iter()
            .map(|r| {
                (
                    r.id.as_str(),
                    r.key.as_str(),
                    r.model.as_str(),
                    r.at,
                    r.usage,
                    r.duration_ms,
                    r.cwd.as_deref(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            got,
            [
                (
                    "copilot:s1",
                    "copilot:s1:store:1",
                    "gpt-5.4-mini",
                    1_792_310_405_000,
                    Usage {
                        input: 369,
                        output: 100,
                        write: 20_974,
                        ..Usage::default()
                    },
                    Some(1_234),
                    Some("/w/repo"),
                ),
                (
                    "copilot:s1",
                    "copilot:s1:store:2",
                    "claude-sonnet-4.6",
                    1_792_310_460_000,
                    Usage {
                        input: 4_000,
                        output: 380,
                        read: 25_000,
                        write: 1_000,
                        reasoning: 120,
                    },
                    None,
                    Some("/w/repo"),
                ),
            ]
        );
        assert_eq!(page.watermark, Some(json!({"after": 3})));
        assert!(read(&store(), &json!({"after": 3}))
            .unwrap()
            .records
            .is_empty());
    }

    #[test]
    fn a_store_without_the_usage_table_is_not_read() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE sessions(id TEXT)").unwrap();
        assert!(read(&db, &Value::Null).is_err());
    }

    #[test]
    fn the_first_store_row_replaces_the_other_sources_once() {
        let mut log = Log::new(std::env::temp_dir().join("events.jsonl"), "copilot");
        assert!(log.copilot_otel_counts());
        assert_eq!(log.copilot_store_counts(), Counts::Replace);
        assert_eq!(log.copilot_store_counts(), Counts::Add);
        assert!(!log.copilot_otel_counts());
        assert_eq!(log.source_state["hasInference"], true);
    }
}
