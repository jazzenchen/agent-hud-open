//! Every session in ZCode's database, with what the desktop app's task list
//! says of its task: the name the user gave it, and whether they archived or
//! deleted it.
use crate::transcript::{sql_column, sql_columns, sql_tables, sql_text, sql_time};
use rusqlite::{Connection, OpenFlags};
use std::{collections::HashMap, path::Path, time::Duration};

/// Kinds of session a person talks to: a session of their own, a fork of one
/// (whose parent is the session it was forked from) and a workflow they
/// started. Every other kind with a parent (a sub-agent's, a workflow's
/// step, a side chat about a selection) is that parent's child
/// (zcode.cjs 0.15, `session.task_type`).
const TOP_LEVEL: [&str; 3] = ["interactive", "fork", "workflow_parent"];

/// How the CLI names a session it has no title for yet.
const UNTITLED: [&str; 2] = ["Untitled session", "New session"];

/// What one session is, as ZCode and its desktop app keep it.
pub(super) struct Session {
    pub(super) title: Option<String>,
    pub(super) cwd: Option<String>,
    pub(super) parent: Option<String>,
    pub(super) archived: bool,
    pub(super) at: Option<i64>,
    /// Something was said in it.
    pub(super) said: bool,
    /// The desktop app's word on it could not be read this time.
    pub(super) unsure: bool,
}

/// What the desktop app says of one task.
#[derive(Default)]
pub(super) struct Task {
    title: Option<String>,
    renamed: bool,
    hidden: bool,
}

/// Every session in the database, by its own id, with what the desktop app
/// says of its task.
pub(super) fn sessions(
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
pub(super) fn tasks(path: &Path) -> Option<HashMap<String, Task>> {
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
