//! ZCode's sessions and calls, from the database its desktop app and its CLI
//! share (`~/.zcode/cli/db/db.sqlite`): each session as ZCode keeps it, then
//! each model call (`model_usage`). The desktop app keeps its own list of
//! tasks beside it (`~/.zcode/v2/tasks-index.sqlite`), where its archive,
//! its deletions and the names the user gives a task live.
use crate::account::hash;
use crate::transcript::{
    sql_column, sql_columns, sql_count, sql_model, sql_tables, sql_text, sql_time,
};
use crate::{
    transcript::records::{Page, Record},
    Usage,
};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

/// Sessions last active longer ago than this are not listed again: Agent
/// HUD keeps tokens 30 days and lists sessions 7.
const KEPT_MS: i64 = 30 * 86_400_000;

/// Kinds of session a person talks to: a session of their own, a fork of one
/// (whose parent is the session it was forked from) and a workflow they
/// started. Every other kind with a parent (a sub-agent's, a workflow's
/// step, a side chat about a selection) is that parent's child
/// (zcode.cjs 0.15, `session.task_type`).
const TOP_LEVEL: [&str; 3] = ["interactive", "fork", "workflow_parent"];

/// How the CLI names a session it has no title for yet.
const UNTITLED: [&str; 2] = ["Untitled session", "New session"];

/// What one session is, as ZCode and its desktop app keep it.
struct Session {
    title: Option<String>,
    cwd: Option<String>,
    parent: Option<String>,
    archived: bool,
    at: Option<i64>,
    /// Something was said in it.
    said: bool,
    /// The desktop app's word on it could not be read this time.
    unsure: bool,
}

/// What the desktop app says of one task.
#[derive(Default)]
struct Task {
    title: Option<String>,
    renamed: bool,
    hidden: bool,
}

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

/// Every session in the database, by its own id, with what the desktop app
/// says of its task.
fn sessions(
    db: &Connection,
    tasks: &HashMap<String, Task>,
) -> rusqlite::Result<HashMap<String, Session>> {
    let tables = sql_tables(db)?;
    let mut sessions = HashMap::new();
    if !tables.contains("session") {
        return Ok(sessions);
    }
    let s = sql_columns(db, "session")?;
    if !s.contains("id") {
        return Ok(sessions);
    }
    let column = |name| sql_column(&s, "s", name, "NULL");
    let said = if tables.contains("message") {
        "EXISTS(SELECT 1 FROM message m WHERE m.session_id=s.id)"
    } else {
        "1"
    };
    let linked = if tables.contains("session_task_link") {
        "(SELECT l.parent_session_id FROM session_task_link l WHERE l.child_session_id=s.id)"
    } else {
        "NULL"
    };
    let sql = format!(
        "SELECT s.id,{},{},{},{},{},{},{},{},{},{},{said},{linked} FROM session s",
        column("title"),
        column("title_source"),
        column("directory"),
        column("path"),
        column("parent_id"),
        column("task_type"),
        column("time_created"),
        column("time_updated"),
        column("time_archived"),
        column("time_deleted"),
    );
    let mut statement = db.prepare(&sql)?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let Some(id) = sql_text(row, 0) else {
            continue;
        };
        let task = tasks.get(&id);
        let placeholder = sql_text(row, 2).as_deref() == Some("default");
        let own = sql_text(row, 1)
            .map(|title| title.trim().to_owned())
            .filter(|title| !placeholder && !title.is_empty() && !UNTITLED.contains(&&**title));
        let title = match task {
            Some(task) if task.renamed && task.title.is_some() => task.title.clone(),
            _ => own.or_else(|| task.and_then(|task| task.title.clone())),
        };
        let kind = sql_text(row, 6);
        let parent = sql_text(row, 5)
            .or_else(|| sql_text(row, 12))
            .filter(|parent| *parent != id)
            .filter(|_| {
                !kind
                    .as_deref()
                    .is_some_and(|kind| TOP_LEVEL.contains(&kind))
            });
        let at = [sql_time(row, 7, false), sql_time(row, 8, false)]
            .into_iter()
            .flatten()
            .max();
        let archived = sql_time(row, 9, false).is_some()
            || sql_time(row, 10, false).is_some()
            || task.is_some_and(|task| task.hidden);
        let said = row.get::<_, Option<i64>>(11)?.unwrap_or(0) != 0;
        sessions.insert(
            id,
            Session {
                title,
                cwd: sql_text(row, 3).or_else(|| sql_text(row, 4)),
                parent,
                archived,
                at,
                said,
                unsure: false,
            },
        );
    }
    Ok(sessions)
}

/// The desktop app's tasks by session id: what the user named them, and
/// whether they archived or deleted them; none when ZCode's CLI is used
/// alone or the list is not one Agent HUD reads, and None while the desktop
/// app holds it.
fn tasks(path: &Path) -> Option<HashMap<String, Task>> {
    match read_tasks(path) {
        Ok(tasks) => Some(tasks),
        Err(error)
            if matches!(
                error.sqlite_error_code(),
                Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
            ) =>
        {
            None
        }
        Err(_) => Some(HashMap::new()),
    }
}

fn read_tasks(path: &Path) -> rusqlite::Result<HashMap<String, Task>> {
    let mut tasks = HashMap::new();
    if !path.is_file() {
        return Ok(tasks);
    }
    let index = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    index.busy_timeout(Duration::from_millis(250))?;
    let t = sql_columns(&index, "tasks")?;
    if !t.contains("task_id") {
        return Ok(tasks);
    }
    let column = |name| sql_column(&t, "t", name, "NULL");
    let local = if t.contains("host_id") {
        "WHERE t.host_id IS NULL OR t.host_id='local'"
    } else {
        ""
    };
    let sql = format!(
        "SELECT t.task_id,{},{},{},{},{} FROM tasks t {local}",
        column("title"),
        column("title_overridden"),
        column("archived"),
        column("deleted"),
        column("cli_deleted"),
    );
    let mut statement = index.prepare(&sql)?;
    let mut rows = statement.query([])?;
    let flag =
        |row: &rusqlite::Row<'_>, at| row.get::<_, Option<i64>>(at).ok().flatten() == Some(1);
    while let Some(row) = rows.next()? {
        let Some(id) = sql_text(row, 0) else {
            continue;
        };
        let task = tasks.entry(id).or_insert_with(Task::default);
        let title = sql_text(row, 1)
            .map(|title| title.trim().to_owned())
            .filter(|title| !title.is_empty());
        let renamed = flag(row, 2);
        if title.is_some() && (renamed || task.title.is_none()) {
            task.title = title;
            task.renamed |= renamed;
        }
        task.hidden |= flag(row, 3) || flag(row, 4) || flag(row, 5);
    }
    Ok(tasks)
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
