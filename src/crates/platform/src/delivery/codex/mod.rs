//! Codex threads take a message from the desktop app that has them open, from
//! the shared CLI daemon on macOS and Linux, or through Codex's own queue.
use super::*;
use std::path::Path;

mod desktop;
// How a send reaches Codex: the desktop app's socket and the shared CLI
// daemon on macOS and Linux; the desktop app's pipe and the queue on Windows.
#[cfg(unix)]
#[path = "unix/mod.rs"]
mod transport;
#[cfg(windows)]
#[path = "windows.rs"]
mod transport;
pub(super) use transport::{queue_created, start_in_daemon, MODEL_AT_START, OPERATIONS, SUPPORTED};
mod model_list;
pub(super) use model_list::{models, persist_started_thread, thread_start};

/// Host injection point for isolated fixtures or a shared native state adapter.
pub trait CodexTransport: Send + Sync {
    fn deliver(
        &self,
        data: &Path,
        target: &SessionTarget,
        request: &SendRequest,
        timeout: Duration,
    ) -> Result<Receipt>;
    /// Whether something that takes a steer answers in `data` now. A
    /// fixture always does.
    fn steer_owner(&self, _data: &Path) -> bool {
        true
    }
}
/// The installed clients. With `queue` set, an `enqueue` that finds no
/// shared daemon goes into Codex's queue through a short-lived app-server.
#[derive(Clone, Debug, Default)]
pub struct NativeCodexTransport {
    pub queue: Option<QueueRoute>,
}
/// How a message reaches Codex's queue without a shared daemon, and the
/// folders its desktop app is looked for in to wake a thread nobody has open.
#[derive(Clone, Debug)]
pub struct QueueRoute {
    pub executable: std::path::PathBuf,
    pub environment: std::collections::BTreeMap<String, String>,
    pub applications: Vec<std::path::PathBuf>,
}
impl CodexTransport for NativeCodexTransport {
    fn deliver(
        &self,
        data: &Path,
        target: &SessionTarget,
        request: &SendRequest,
        timeout: Duration,
    ) -> Result<Receipt> {
        transport::deliver(data, target, request, timeout, self.queue.as_ref())
    }
    fn steer_owner(&self, data: &Path) -> bool {
        transport::steer_owner(data)
    }
}

/// Adds a message to Codex's queue through a short-lived app-server.
fn enqueue_stdio(
    route: &QueueRoute,
    data: &Path,
    thread: &str,
    text: &str,
    message_id: &str,
    timeout: Duration,
) -> Result<()> {
    let mut server =
        process::StdioServer::open(&route.executable, data, &route.environment, timeout)?;
    let value = server.mutation(
        "thread/queue/add",
        serde_json::json!({"threadId":thread,"clientUserMessageId":message_id,
        "input":[{"type":"text","text":text,"text_elements":[]}]}),
    )?;
    server.finish();
    if value["queuedSubmission"].is_object() {
        Ok(())
    } else {
        Err(Failure::AcceptanceUncertain)
    }
}

/// Whether an app has the thread open now: Codex holds the thread's writer
/// lock for as long as it has the thread loaded (the desktop app for hours
/// after its last turn). Asked without waiting, and let go at once.
pub fn thread_open(data: &Path, thread: &str) -> bool {
    let Ok(file) = std::fs::File::open(
        data.join("thread-writer-locks")
            .join(format!("{thread}.lock")),
    ) else {
        return false;
    };
    match file.try_lock_shared() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(std::fs::TryLockError::WouldBlock) => true,
        Err(std::fs::TryLockError::Error(_)) => false,
    }
}

/// Thread ids whose writer lock an app currently holds in `data`, skipping any
/// id for which `exclude` returns true (for example, threads already present
/// in the 24-hour snapshot) so their lock files are not probed unnecessarily.
pub fn open_threads_excluding(data: &Path, exclude: impl Fn(&str) -> bool) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(data.join("thread-writer-locks")) else {
        return Vec::new();
    };
    let mut ids = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let text = name.to_str()?;
            if text.starts_with('.') {
                return None;
            }
            let id = text.strip_suffix(".lock")?;
            (!id.is_empty() && !exclude(id) && thread_open(data, id)).then(|| id.to_owned())
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn safe_thread_id(thread: &str) -> bool {
    !thread.is_empty() && !thread.contains('/') && !thread.contains('\\') && !thread.contains("..")
}

/// Whether `thread` has an archived rollout in `data/archived_sessions`.
fn archived_in_folder(data: &Path, thread: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(data.join("archived_sessions")) else {
        return false;
    };
    let suffix = format!("-{thread}.jsonl");
    let exact = format!("{thread}.jsonl");
    entries.flatten().any(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|name| name == exact || name.ends_with(&suffix))
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SqliteThread {
    Active,
    Excluded,
    Missing,
}

fn sqlite_thread_state(data: &Path, thread: &str) -> SqliteThread {
    use rusqlite::{Connection, OpenFlags, OptionalExtension};
    let db_path = data.join("state_5.sqlite");
    if !db_path.is_file() {
        return SqliteThread::Missing;
    }
    let Ok(conn) = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return SqliteThread::Missing;
    };
    let _ = conn.busy_timeout(Duration::from_millis(50));
    let row: rusqlite::Result<Option<(i64, String, String)>> = conn
        .query_row(
            "SELECT archived, rollout_path, source FROM threads WHERE id = ?1",
            [thread],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional();
    match row {
        Ok(Some((archived, rollout_path, source))) => {
            if archived != 0 || source.trim_start().starts_with("{\"subagent\"") {
                return SqliteThread::Excluded;
            }
            if (!rollout_path.is_empty() && Path::new(&rollout_path).is_file())
                || thread_open(data, thread)
            {
                SqliteThread::Active
            } else {
                SqliteThread::Excluded
            }
        }
        Ok(None) | Err(_) => SqliteThread::Missing,
    }
}

/// Whether `thread` is known to be archived (or a sub-agent in `state_5.sqlite`).
pub fn thread_archived(data: &Path, thread: &str) -> bool {
    !safe_thread_id(thread)
        || archived_in_folder(data, thread)
        || sqlite_thread_state(data, thread) == SqliteThread::Excluded
}

/// Whether `thread` still exists as a non-archived, non-subagent Codex thread
/// (either live-locked via `thread-writer-locks` or active with an existing
/// rollout in `state_5.sqlite`). Archived and deleted threads return `false`.
pub fn thread_active(data: &Path, thread: &str) -> bool {
    if !safe_thread_id(thread) || archived_in_folder(data, thread) {
        return false;
    }
    match sqlite_thread_state(data, thread) {
        SqliteThread::Active => true,
        SqliteThread::Excluded => false,
        SqliteThread::Missing => thread_open(data, thread),
    }
}
/// Whether Codex's desktop app is installed in one of `applications`:
/// Codex.app, or ChatGPT.app that carries it, in an app folder (macOS), or
/// one of the app's own programs (Windows lists those).
pub fn desktop_installed(applications: &[std::path::PathBuf]) -> bool {
    applications.iter().any(|folder| {
        folder.is_file()
            || ["Codex.app", "ChatGPT.app"]
                .iter()
                .any(|app| folder.join(app).is_dir())
    })
}

/// Why a message Codex queued has not gone in yet: none while an app has the
/// thread open, which runs it once idle. Otherwise the desktop app, when
/// installed, is asked to open the thread so it runs the queue.
pub(super) fn queued_reason(
    data: &Path,
    thread: &str,
    applications: &[std::path::PathBuf],
) -> Option<String> {
    use process::ProcessRunner;
    if thread_open(data, thread) {
        return None;
    }
    Some(
        if desktop_installed(applications) {
            match process::NativeProcessRunner.open(&format!("codex://threads/wake/{thread}")) {
                Ok(()) => "Woke the thread in the Codex app",
                Err(_) => "Queued; the Codex app did not open the thread",
            }
        } else {
            "Queued until Codex opens the thread"
        }
        .into(),
    )
}

/// Desktop's current-turn selector, including canonical history with empty turns.
pub fn current_desktop_turn(state: &serde_json::Value) -> Result<Option<(String, String)>> {
    use serde_json::Value;
    fn current(turns: Vec<&Value>) -> Result<Option<(String, String)>> {
        for turn in turns.into_iter().rev() {
            let status = turn["status"]
                .as_str()
                .ok_or_else(|| Failure::Protocol("Desktop turn status".into()))?;
            let id = turn
                .get("turnId")
                .filter(|v| !v.is_null())
                .or_else(|| turn.get("id"));
            if turn.get("turnId").is_some_and(|v| !v.is_null())
                || !matches!(status, "completed" | "failed")
            {
                let id = id
                    .and_then(Value::as_str)
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| Failure::Protocol("Desktop current turn identity".into()))?;
                return Ok(Some((id.into(), status.into())));
            }
        }
        Ok(None)
    }
    let Some(history) = state
        .get("turnHistory")
        .filter(|v| v["kind"] == "canonical")
    else {
        let turns = state["turns"]
            .as_array()
            .ok_or_else(|| Failure::Protocol("Desktop turns".into()))?;
        return current(turns.iter().collect());
    };
    let islands = history["history"]["islands"]
        .as_array()
        .ok_or_else(|| Failure::Protocol("Desktop history islands".into()))?;
    let entities = history["history"]["entitiesByKey"]
        .as_object()
        .ok_or_else(|| Failure::Protocol("Desktop history entities".into()))?;
    let turns = |island: &Value| -> Result<Vec<&Value>> {
        island["entries"]
            .as_array()
            .ok_or_else(|| Failure::Protocol("Desktop history entries".into()))?
            .iter()
            .map(|entry| {
                let key = entry["value"]
                    .as_str()
                    .ok_or_else(|| Failure::Protocol("Desktop history key".into()))?;
                entities
                    .get(key)
                    .ok_or_else(|| Failure::Protocol("Desktop history entity".into()))
            })
            .collect()
    };
    if let Some(island) = islands
        .last()
        .filter(|v| v["newerBoundary"]["status"] == "exhausted")
    {
        if let Some(value) = current(turns(island)?)? {
            return Ok(Some(value));
        }
    }
    current(
        islands
            .iter()
            .map(turns)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect(),
    )
}
