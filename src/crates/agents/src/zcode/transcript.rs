//! ZCode's sessions and calls, from the database its desktop app and its CLI
//! share (`~/.zcode/cli/db/db.sqlite`): each session as ZCode keeps it, then
//! each model call (`model_usage`). The desktop app keeps its own list of
//! tasks beside it (`~/.zcode/v2/tasks-index.sqlite`), where its archive,
//! its deletions and the names the user gives a task live.
mod sessions;

use crate::account::hash;
use crate::transcript::{sql_column, sql_columns, sql_count, sql_model, sql_text, sql_time};
use crate::{
    transcript::records::{Page, Record},
    Usage,
};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use sessions::{sessions, tasks, Session};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Sessions last active longer ago than this are not listed again: Agent
/// HUD keeps tokens 30 days and lists sessions 7.
const KEPT_MS: i64 = 30 * 86_400_000;

/// The sessions new or changed since `cursor`, and the calls after it.
pub fn read(db: &Connection, cursor: &Value) -> rusqlite::Result<Page> {
    let index = db.path().and_then(|path| index_path(Path::new(path)));
    read_at(
        db,
        cursor,
        index.as_deref(),
        chrono::Utc::now().timestamp_millis(),
    )
}

/// The desktop app's task list beside the database at `path`.
fn index_path(path: &Path) -> Option<PathBuf> {
    let db = path.parent()?;
    let cli = db.parent()?;
    (db.file_name()? == "db" && cli.file_name()? == "cli")
        .then(|| cli.parent().map(|home| home.join("v2/tasks-index.sqlite")))
        .flatten()
}

pub(crate) fn read_at(
    db: &Connection,
    cursor: &Value,
    index: Option<&Path>,
    now_ms: i64,
) -> rusqlite::Result<Page> {
    let usage = sql_columns(db, "model_usage")?;
    if ![
        "id",
        "session_id",
        "model_id",
        "started_at",
        "input_tokens",
        "output_tokens",
    ]
    .iter()
    .all(|key| usage.contains(*key))
    {
        return Err(rusqlite::Error::InvalidQuery);
    }
    // A task list there but not readable now (the desktop app holding it)
    // leaves what it says as it was: no session is read again this time,
    // and calls say nothing of their session's title or archive.
    let tasks = index.map_or_else(|| Some(HashMap::new()), tasks);
    let unsure = tasks.is_none();
    let mut sessions = sessions(db, &tasks.unwrap_or_default())?;
    if unsure {
        for session in sessions.values_mut() {
            session.title = None;
            session.unsure = true;
        }
    }
    let after = cursor["after"]
        .as_i64()
        .or_else(|| cursor.as_i64())
        .unwrap_or(0);

    // Each session once, and again whenever it changes: its title, its
    // folder, its parent, or whether it is archived.
    let seen = cursor["seen"].as_object();
    let mut kept = Map::new();
    let mut records = Vec::new();
    for (id, session) in sessions.iter().filter(|_| !unsure) {
        let Some(at) = session.at.filter(|at| *at >= now_ms - KEPT_MS) else {
            continue;
        };
        if !session.said {
            continue;
        }
        let print = hash(&[
            session.title.as_deref().unwrap_or(""),
            &at.to_string(),
            if session.archived { "1" } else { "0" },
            session.cwd.as_deref().unwrap_or(""),
            session.parent.as_deref().unwrap_or(""),
        ])[..16]
            .to_owned();
        let unchanged = seen
            .and_then(|seen| seen.get(id))
            .is_some_and(|old| old.as_str() == Some(print.as_str()));
        kept.insert(id.clone(), json!(print));
        if !unchanged {
            records.push(record(
                id,
                Some(session),
                "session".into(),
                at,
                Usage::default(),
            ));
        }
    }

    let fields = [
        "completed_at",
        "reasoning_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
        "computed_total_tokens",
    ]
    .map(|key| sql_column(&usage, "mu", key, "NULL"));
    let sql = format!(
        "SELECT mu.rowid,mu.id,mu.session_id,mu.model_id,mu.started_at,{},mu.input_tokens,mu.output_tokens,{},{},{},{} FROM model_usage mu WHERE mu.rowid>?1 ORDER BY mu.rowid LIMIT 1000",
        fields[0], fields[1], fields[2], fields[3], fields[4]
    );
    let mut statement = db.prepare(&sql)?;
    let mut rows = statement.query([after])?;
    let mut last = after;
    let mut seen_rows = 0;
    while let Some(row) = rows.next()? {
        seen_rows += 1;
        last = row.get(0)?;
        let Some(at) = sql_time(row, 5, false).or_else(|| sql_time(row, 4, false)) else {
            continue;
        };
        let input = sql_count(row, 6)?;
        let output = sql_count(row, 7)?;
        let reasoning = sql_count(row, 8)?;
        let read = sql_count(row, 9)?;
        let write = sql_count(row, 10)?;
        let total = row.get_ref(11)?;
        let inclusive = if total == rusqlite::types::ValueRef::Null {
            !usage.contains("computed_total_tokens")
        } else {
            sql_count(row, 11)? == input.saturating_add(output)
        };
        if inclusive && read > input {
            continue;
        }
        let fresh = if inclusive {
            input - read
        } else {
            input.saturating_add(write)
        };
        let out = if inclusive {
            output
        } else {
            output.saturating_add(reasoning)
        };
        let write = write.min(fresh);
        let reasoning = reasoning.min(out);
        let raw = sql_text(row, 2).unwrap_or_else(|| "unknown".into());
        let mut call = record(
            &raw,
            sessions.get(&raw),
            sql_text(row, 1).unwrap_or_else(|| format!("row-{last}")),
            at,
            Usage {
                input: fresh - write,
                output: out - reasoning,
                read,
                write,
                reasoning,
            },
        );
        call.model = sql_model(sql_text(row, 3));
        records.push(call);
    }
    if unsure {
        kept = seen.cloned().unwrap_or_default();
    }
    let next = json!({"after": last, "seen": kept});
    Ok(Page {
        records,
        cursor: (seen_rows == 1000).then(|| next.clone()),
        watermark: (seen_rows < 1000).then_some(next),
    })
}

/// A record of session `id`: the session itself, or one of its calls. A
/// call of a session the database no longer holds says nothing of it.
fn record(id: &str, session: Option<&Session>, key: String, at: i64, usage: Usage) -> Record {
    Record {
        id: format!("zcode:{id}"),
        title: session.and_then(|s| s.title.clone()),
        title_fallback: None,
        cwd: session.and_then(|s| s.cwd.clone()),
        model: "Unknown".into(),
        key,
        at,
        usage,
        parent: session
            .and_then(|s| s.parent.as_ref())
            .map(|p| format!("zcode:{p}")),
        archived: session.filter(|s| !s.unsure).map(|s| s.archived),
        updated: None,
        duration_ms: None,
    }
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
