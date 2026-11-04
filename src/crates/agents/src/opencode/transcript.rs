//! The SQLite session & child-session reader of OpenCode's database
//! (`opencode.db`), and of its forks' (`Store`).
use crate::transcript::{sql_column, sql_columns, sql_tables, sql_text, unsigned};
use crate::{
    transcript::records::{Page, Record},
    Usage,
};
use rusqlite::Connection;
use serde_json::{json, Value};

/// How long a reply took: from its creation to its completion, when it
/// finished without calling tools. A reply that called tools completes after
/// they ran, so that span is not the model's.
pub fn reply_time(finish: &Value, time: &Value) -> Option<u64> {
    let finish = finish.as_str()?;
    if finish == "tool-calls" {
        return None;
    }
    let took = time["completed"].as_f64()? - time["created"].as_f64()?;
    (took.is_finite() && took >= 1.0).then_some(took as u64)
}

pub fn read(db: &Connection, cursor: &Value, store: &super::Store) -> rusqlite::Result<Page> {
    let source = store.agent.id;
    let tables = sql_tables(db)?;
    let stage = cursor["stage"].as_u64().unwrap_or(0);
    let after_rowid = cursor["after"].as_i64().unwrap_or(0);
    let new = tables.contains("session_message");
    let names = if new {
        vec!["session_message", "message"]
    } else {
        vec!["message"]
    };
    let Some(messages) = names.get(stage as usize) else {
        return Ok(Page {
            records: vec![],
            cursor: None,
            watermark: None,
        });
    };
    if !tables.contains(*messages) {
        let more = (stage as usize) + 1 < names.len();
        return Ok(Page {
            records: vec![],
            cursor: more.then(|| json!({"stage": stage + 1, "after": 0})),
            watermark: None,
        });
    }
    let title_table = if *messages == "session_message" {
        ["session_v2", "session"]
    } else {
        ["session", "session_v2"]
    }
    .into_iter()
    .find(|table| tables.contains(*table));
    let s = title_table
        .map(|table| sql_columns(db, table))
        .transpose()?
        .unwrap_or_default();
    let mut meta = ["title", "directory", "time_archived", "parent_id"]
        .map(|field| sql_column(&s, "s", field, "NULL"));
    // A title the client made up for an untitled session (MiMo Code's
    // "Untitled") names nothing.
    if s.contains("title") && s.contains("title_source") {
        meta[0] = "CASE WHEN s.title_source='fallback' THEN NULL ELSE s.title END".into();
    }
    let fields = [
        "role",
        "time",
        "tokens",
        "modelID",
        "providerID",
        "model",
        "path",
        "finish",
    ]
    .map(|field| format!("'{field}',json_extract(m.data,'$.{field}')"))
    .join(",");
    let mut filter = if *messages == "session_message" {
        "m.type='assistant'".into()
    } else {
        format!(
            "json_extract(m.data,'$.role')='assistant'{}",
            if new {
                " AND m.session_id NOT IN (SELECT session_id FROM session_message WHERE type='assistant' AND session_id IS NOT NULL)"
            } else {
                ""
            }
        )
    };
    // One malformed row would fail the whole statement, so it is left out
    // before its fields are read.
    filter.insert_str(0, "json_valid(m.data) AND ");
    if let Some(prefix) = store.copies {
        filter.push_str(&format!(
            " AND IFNULL(substr(m.session_id,1,{}),'')<>'{prefix}'",
            prefix.len()
        ));
    }
    for table in store.imports.iter().filter(|t| tables.contains(**t)) {
        let columns = sql_columns(db, table)?;
        if !columns.contains("session_id") {
            continue;
        }
        if columns.contains("message_ids") {
            filter.push_str(&format!(
                " AND m.id NOT IN (SELECT j.value FROM {table} i, json_each(i.message_ids) j WHERE json_valid(i.message_ids)) AND m.session_id NOT IN (SELECT session_id FROM {table} WHERE message_ids IS NULL OR NOT json_valid(message_ids))"
            ));
        } else {
            filter.push_str(&format!(
                " AND m.session_id NOT IN (SELECT session_id FROM {table})"
            ));
        }
    }
    let join = title_table
        .map(|t| format!("LEFT JOIN {t} s ON s.id=m.session_id"))
        .unwrap_or_default();
    // rowid is a pagination cursor, not an append watermark: OpenCode changes
    // tokens and session metadata on existing rows. A changed file starts a new
    // bounded pass; unchanged snapshots never call this reader.
    let mut statement = db.prepare(&format!("SELECT m.rowid,m.id,m.session_id,json_object({fields}),{},{},{},{} FROM {messages} m {join} WHERE m.rowid>?1 AND {filter} ORDER BY m.rowid LIMIT 1000",meta[0],meta[1],meta[2],meta[3]))?;
    let mut rows = statement.query([after_rowid])?;
    let mut records = Vec::new();
    let mut seen = 0;
    let mut last_rowid = after_rowid;
    while let Some(row) = rows.next()? {
        seen += 1;
        last_rowid = row.get(0)?;
        let msg_id = sql_text(row, 1).ok_or(rusqlite::Error::InvalidQuery)?;
        let Some(raw) = sql_text(row, 2) else {
            continue;
        };
        let v: Value = serde_json::from_str(&row.get::<_, String>(3)?)
            .map_err(|_| rusqlite::Error::InvalidQuery)?;
        let Some(at) = v["time"]["created"]
            .as_f64()
            .filter(|n| *n > 0.0)
            .map(|n| n as i64)
        else {
            continue;
        };
        let t = &v["tokens"];
        if !t.is_object() {
            continue;
        }
        let model = v["modelID"]
            .as_str()
            .or_else(|| v["model"]["id"].as_str())
            .unwrap_or("Unknown");
        let provider = v["providerID"]
            .as_str()
            .or_else(|| v["model"]["providerID"].as_str())
            .unwrap_or("Unknown");
        let title = sql_text(row, 4).filter(|s| {
            !((s.starts_with("New session - ") || s.starts_with("Child session - "))
                && s.ends_with('Z'))
        });
        records.push(Record {
            id: format!("{source}:{raw}"),
            title,
            title_fallback: None,
            cwd: sql_text(row, 5).or_else(|| v["path"]["root"].as_str().map(str::to_owned)),
            model: format!("{model}#{provider}"),
            key: format!("{source}:{msg_id}"),
            at,
            usage: Usage {
                input: unsigned(&t["input"]),
                output: unsigned(&t["output"]),
                read: unsigned(&t["cache"]["read"]),
                write: unsigned(&t["cache"]["write"]),
                reasoning: unsigned(&t["reasoning"]),
            },
            parent: sql_text(row, 7).map(|id| format!("{source}:{id}")),
            archived: Some(!matches!(row.get_ref(6)?, rusqlite::types::ValueRef::Null)),
            updated: v["time"]["completed"].as_i64(),
            duration_ms: reply_time(&v["finish"], &v["time"]),
        });
    }
    let cursor = if seen == 1000 {
        Some(json!({"stage": stage, "after": last_rowid}))
    } else if (stage as usize) + 1 < names.len() {
        Some(json!({"stage": stage + 1, "after": 0}))
    } else {
        None
    };
    Ok(Page {
        records,
        cursor,
        watermark: None,
    })
}
