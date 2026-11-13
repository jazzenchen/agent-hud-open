//! Returning to a Claude Code background job (`claude --bg`). A job has no
//! window of its own, so it moves into the Claude desktop app, as sessions a
//! phone started do: `claude stop <job>` keeps the conversation, the desktop
//! app refuses a session that still runs, so the move waits for the job's
//! process to leave Claude Code's registry (`sessions/<pid>.json`), then opens
//! `claude://resume?session=<id>`. The desktop app takes the session in as its
//! own `local_<id>`, which the next collection pass finds in its index. A job
//! whose turn is at work finishes that turn first: stopping it would cut the
//! turn off. The move runs on its own thread: the request that asked for it
//! holds the runtime.
use crate::os::Environment;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

/// Sessions moving now; a second click while one moves starts nothing.
static MOVING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
/// How long a turn at work may take to end, how long `claude stop` may take,
/// and how long the job's process may take to go.
const TURN: Duration = Duration::from_secs(30 * 60);
const STOP: Duration = Duration::from_secs(30);
const GONE: Duration = Duration::from_secs(10);

/// False when this host cannot move the job; the caller then falls back.
pub(super) fn open(home: &Path, session: &str, job: &str) -> Result<bool, String> {
    if !crate::os::MOVES_CLAUDE_JOBS || !super::is_uuid(session) || !job_name(job) {
        return Ok(false);
    }
    let env = std::env::vars().collect::<Environment>();
    if !crate::os::registered_bundle("com.anthropic.claudefordesktop")
        && !crate::os::application_installed(home, &env, "Claude")
    {
        return Ok(false);
    }
    let Some(claude) = crate::os::find_engine(home, &env, "claude") else {
        return Ok(false);
    };
    let config =
        crate::os::variable(&env, "CLAUDE_CONFIG_DIR").unwrap_or_else(|| home.join(".claude"));
    let path = crate::os::engine_path(home, &env);
    if !MOVING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(session.to_owned())
    {
        return Ok(true);
    }
    let (session, job) = (session.to_owned(), job.to_owned());
    let moving = session.clone();
    let spawned = std::thread::Builder::new()
        .name("agenthud-claude-job".into())
        .spawn(move || {
            let _ = run(
                Steps {
                    at_work: || at_work(&config, &session),
                    stop: || stop(&claude, &job, &path),
                    live: || holds(&config, &session),
                    open: || {
                        crate::os::open_session_url(&format!("claude://resume?session={session}"))
                    },
                },
                [TURN, GONE],
                Duration::from_millis(250),
            );
            MOVING
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&session);
        });
    if spawned.is_err() {
        MOVING
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&moving);
        return Err("The session could not be moved into the Claude desktop app".into());
    }
    Ok(true)
}

/// The move's steps, passed in so tests run them without a process.
struct Steps<W, S, L, O> {
    /// Whether the job's turn is at work.
    at_work: W,
    stop: S,
    /// Whether a process still holds the session.
    live: L,
    open: O,
}
/// `bounds`: how long the turn may stay at work, and the process stay live.
fn run<W, S, L, O>(
    steps: Steps<W, S, L, O>,
    bounds: [Duration; 2],
    poll: Duration,
) -> Result<(), String>
where
    W: Fn() -> bool,
    S: FnOnce(),
    L: Fn() -> bool,
    O: FnOnce() -> Result<(), String>,
{
    let wait = |check: &dyn Fn() -> bool, bound: Duration| {
        let deadline = Instant::now() + bound;
        while check() {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(poll);
        }
        true
    };
    if !wait(&steps.at_work, bounds[0]) {
        return Err("The background job's turn is still at work".into());
    }
    // A job that stopped already answers with an error; the registry decides.
    (steps.stop)();
    if !wait(&steps.live, bounds[1]) {
        return Err("The background job is still running".into());
    }
    (steps.open)()
}

fn stop(claude: &Path, job: &str, path: &str) {
    let mut command = Command::new(claude);
    command
        .args(["stop", job])
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::os::configure_command(&mut command);
    let Ok(mut child) = crate::os::spawn(&mut command) else {
        return;
    };
    let deadline = Instant::now() + STOP;
    while matches!(child.try_wait(), Ok(None)) {
        if Instant::now() >= deadline {
            let _ = child.kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.wait();
}

/// Whether a live Claude Code process holds the session, by its registry.
fn holds(config: &Path, session: &str) -> bool {
    !registry(config, session).is_empty()
}
/// Whether the session's turn is at work: its registry entry is `busy`,
/// running a shell command, or `waiting` on an approval.
fn at_work(config: &Path, session: &str) -> bool {
    registry(config, session)
        .iter()
        .any(|entry| matches!(entry["status"].as_str(), Some("busy" | "shell" | "waiting")))
}
/// The live processes holding the session (`sessions/<pid>.json`).
fn registry(config: &Path, session: &str) -> Vec<serde_json::Value> {
    let Ok(entries) = std::fs::read_dir(config.join("sessions")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
        .filter_map(|path| registry_entry(&path))
        .filter(|entry| entry["sessionId"] == session)
        .collect()
}
fn registry_entry(path: &PathBuf) -> Option<serde_json::Value> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > 1024 * 1024 {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}
fn job_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    const SESSION: &str = "11111111-2222-4333-8444-555555555555";

    fn entry(config: &Path, pid: u32, session: &str, status: &str) -> PathBuf {
        let path = config.join(format!("sessions/{pid}.json"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            serde_json::json!({"pid":pid,"sessionId":session,"kind":"bg","status":status})
                .to_string(),
        )
        .unwrap();
        path
    }

    #[test]
    fn a_turn_at_work_ends_then_the_job_stops_and_the_link_opens_once_it_is_gone() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join(".claude");
        let job = entry(&config, 4242, SESSION, "busy");
        entry(&config, 7, "66666666-7777-4888-9999-aaaaaaaaaaaa", "busy");
        assert!(holds(&config, SESSION) && at_work(&config, SESSION));

        let steps = RefCell::new(Vec::new());
        let checks = Cell::new(0);
        let result = run(
            Steps {
                at_work: || {
                    checks.set(checks.get() + 1);
                    // The turn ends a moment later.
                    if checks.get() == 3 {
                        entry(&config, 4242, SESSION, "idle");
                    }
                    at_work(&config, SESSION)
                },
                stop: || {
                    steps.borrow_mut().push("stop");
                    std::fs::remove_file(&job).unwrap();
                },
                live: || holds(&config, SESSION),
                open: || {
                    steps.borrow_mut().push("open");
                    Ok(())
                },
            },
            [Duration::from_secs(5), Duration::from_secs(5)],
            Duration::from_millis(1),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(checks.get(), 3);
        assert_eq!(*steps.borrow(), vec!["stop", "open"]);
    }

    #[test]
    fn a_job_that_keeps_working_or_running_is_not_handed_to_the_desktop_app() {
        let quick = [Duration::from_millis(20), Duration::from_millis(20)];
        let stopped = Cell::new(false);
        let opened = Cell::new(false);
        let result = run(
            Steps {
                at_work: || true,
                stop: || stopped.set(true),
                live: || true,
                open: || {
                    opened.set(true);
                    Ok(())
                },
            },
            quick,
            Duration::from_millis(1),
        );
        assert!(result.is_err());
        assert!(!stopped.get() && !opened.get());
        let result = run(
            Steps {
                at_work: || false,
                stop: || stopped.set(true),
                live: || true,
                open: || {
                    opened.set(true);
                    Ok(())
                },
            },
            quick,
            Duration::from_millis(1),
        );
        assert!(result.is_err());
        assert!(stopped.get() && !opened.get());
        assert_eq!(
            open(Path::new("/nonexistent"), "not-a-session", "abc"),
            Ok(false)
        );
        assert_eq!(open(Path::new("/nonexistent"), SESSION, "../x"), Ok(false));
    }
}
