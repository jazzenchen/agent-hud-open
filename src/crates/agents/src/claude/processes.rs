//! Claude Code's process registry: every live Claude Code process, in a
//! terminal or in the desktop app, writes `sessions/<pid>.json` under its
//! config home naming its session. It removes the file as it exits cleanly;
//! a process killed outright, as a closed terminal tab kills it, leaves the
//! file behind with a pid that no longer runs (Claude Code clears such files
//! the next time it starts). A session with no file whose process runs has no
//! one to go on with it: what it waited for will not come.
//! Read on every collection pass, each file again only when it changed; each
//! file's process is asked whether it still runs every pass.
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// How long a session's last exit is kept: past it, the transcript's own
/// abandonment has made the session idle anyway.
const GONE_KEPT_MS: i64 = 3_600_000;

/// One registry file as last read: its stamp, and what it says.
#[derive(Clone)]
struct Entry {
    stamp: (Option<SystemTime>, u64),
    pid: Option<i32>,
    session: Option<String>,
    kind: Option<String>,
    entrypoint: Option<String>,
}

impl Entry {
    fn read(path: &Path, stamp: (Option<SystemTime>, u64)) -> Self {
        let value = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .unwrap_or(Value::Null);
        let text = |key: &str| value[key].as_str().map(str::to_owned);
        Self {
            stamp,
            pid: value["pid"]
                .as_i64()
                .and_then(|pid| i32::try_from(pid).ok()),
            session: text("sessionId").filter(|id| super::jobs::uuid(id)),
            kind: text("kind"),
            entrypoint: text("entrypoint"),
        }
    }
}

/// A process as the registry names it, running or not.
pub struct Process<'a> {
    pub session: &'a str,
    pub kind: Option<&'a str>,
    pub entrypoint: Option<&'a str>,
}

#[derive(Clone, Default)]
pub struct ClaudeProcesses {
    files: HashMap<PathBuf, Entry>,
    /// Sessions a running process names now.
    held: HashSet<String>,
    /// Sessions whose last process left, and when that was first seen.
    gone: HashMap<String, i64>,
    revision: u64,
}

impl ClaudeProcesses {
    /// Reads the registry of every Claude config home among `provider_roots`.
    /// True when a session lost its last process or got one back.
    pub fn refresh(&mut self, provider_roots: &[(PathBuf, &'static str)], now: i64) -> bool {
        let mut files = HashMap::new();
        for config in config_homes(provider_roots) {
            let Ok(entries) = fs::read_dir(config.join("sessions")) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let Some(meta) = super::jobs::small_file(&path) else {
                    continue;
                };
                let stamp = (meta.modified().ok(), meta.len());
                let entry = match self.files.remove(&path) {
                    Some(kept) if kept.stamp == stamp => kept,
                    _ => Entry::read(&path, stamp),
                };
                files.insert(path, entry);
            }
        }
        self.files = files;
        let (mut held, mut dead) = (HashSet::new(), HashSet::new());
        for entry in self.files.values() {
            let Some(session) = entry.session.clone() else {
                continue;
            };
            // A file without a pid says nothing of its process: it holds.
            if entry.pid.is_none_or(running) {
                held.insert(session);
            } else {
                dead.insert(session);
            }
        }
        let mut changed = false;
        // Gone: held before and not now, or named only by files whose process
        // no longer runs. A session already gone keeps the time it went.
        for session in self.held.iter().chain(&dead) {
            if !held.contains(session) && !self.gone.contains_key(session) {
                self.gone.insert(session.clone(), now);
                changed = true;
            }
        }
        for session in &held {
            changed |= self.gone.remove(session).is_some();
        }
        self.held = held;
        self.gone
            .retain(|session, at| now - *at < GONE_KEPT_MS || dead.contains(session));
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }

    /// Whether the session's process has gone since its last event: it had
    /// one, none holds it now, and nothing happened in it after it left.
    pub fn ended(&self, session: &str, last_event: Option<i64>) -> bool {
        self.gone
            .get(session)
            .is_some_and(|gone| last_event.is_none_or(|last| last <= *gone))
    }

    /// Goes up whenever `ended` may answer differently.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The processes the registry names, for the background jobs' places.
    pub fn processes(&self) -> impl Iterator<Item = Process<'_>> {
        self.files.values().filter_map(|entry| {
            Some(Process {
                session: entry.session.as_deref()?,
                kind: entry.kind.as_deref(),
                entrypoint: entry.entrypoint.as_deref(),
            })
        })
    }
}

/// Whether a process runs, as signal 0 tells it: sent nowhere, only checked.
/// A reused pid reads as running, which keeps the transcript's half hour.
#[cfg(unix)]
fn running(pid: i32) -> bool {
    if pid <= 0 {
        return true;
    }
    // SAFETY: kill with signal 0 delivers nothing; it only checks the pid.
    let sent = unsafe { libc::kill(pid, 0) } == 0;
    // Another user's process cannot be signalled, but it runs.
    sent || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}
/// Where no signal can ask, every process the registry names is running.
#[cfg(not(unix))]
fn running(_pid: i32) -> bool {
    true
}

/// The Claude config homes the provider roots lie in, each once.
pub fn config_homes(provider_roots: &[(PathBuf, &'static str)]) -> Vec<PathBuf> {
    let mut configs = Vec::<PathBuf>::new();
    for (root, source) in provider_roots {
        if *source == "claude" {
            if let Some(config) = root.parent() {
                if !configs.iter().any(|known| known == config) {
                    configs.push(config.to_path_buf());
                }
            }
        }
    }
    configs
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const A: &str = "11111111-2222-4333-8444-555555555555";
    const B: &str = "66666666-7777-4888-9999-aaaaaaaaaaaa";

    fn write(path: &Path, value: Value) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }
    /// A pid that runs for as long as the test does: its own.
    fn live() -> u32 {
        std::process::id()
    }

    #[test]
    fn a_session_whose_process_left_has_ended_until_one_takes_it_again() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join(".claude");
        let roots = vec![(config.join("projects"), "claude")];
        let first = config.join("sessions").join("7.json");
        write(
            &first,
            json!({"pid":live(),"sessionId":A,"kind":"interactive","entrypoint":"cli"}),
        );
        write(
            &config.join("sessions").join("8.json"),
            json!({"pid":8,"sessionId":"not-a-session"}),
        );
        let mut processes = ClaudeProcesses::default();
        assert!(!processes.refresh(&roots, 1_000));
        assert!(!processes.ended(A, Some(900)));
        assert_eq!(
            processes.processes().map(|p| p.session).collect::<Vec<_>>(),
            vec![A]
        );

        // The terminal closed: its file went, so the session ended then.
        fs::remove_file(&first).unwrap();
        let revision = processes.revision();
        assert!(processes.refresh(&roots, 2_000));
        assert!(processes.revision() != revision);
        assert!(processes.ended(A, Some(1_500)));
        assert!(processes.ended(A, None));
        // Something happened in it after the exit was seen: a new process
        // wrote before the registry showed it.
        assert!(!processes.ended(A, Some(2_500)));
        // A session never seen with a process says nothing.
        assert!(!processes.ended(B, Some(0)));
        assert!(!processes.refresh(&roots, 3_000), "nothing new");

        // Resumed in a new process: no longer ended.
        write(
            &config.join("sessions").join("9.json"),
            json!({"pid":live(),"sessionId":A}),
        );
        assert!(processes.refresh(&roots, 4_000));
        assert!(!processes.ended(A, Some(1_500)));
    }

    #[test]
    fn an_exit_is_forgotten_once_the_transcript_has_given_up_on_the_session() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join(".claude");
        let roots = vec![(config.join("projects"), "claude")];
        let file = config.join("sessions").join("7.json");
        write(&file, json!({"pid":live(),"sessionId":A}));
        let mut processes = ClaudeProcesses::default();
        processes.refresh(&roots, 0);
        fs::remove_file(&file).unwrap();
        processes.refresh(&roots, 1_000);
        assert!(processes.ended(A, None));
        processes.refresh(&roots, 1_000 + GONE_KEPT_MS);
        assert!(!processes.ended(A, None));
    }

    /// A closed terminal tab kills Claude Code before it can take its file
    /// away: the file stays, naming a pid that no longer runs.
    #[cfg(unix)]
    #[test]
    fn a_file_whose_process_no_longer_runs_ends_its_session() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join(".claude");
        let roots = vec![(config.join("projects"), "claude")];
        let mut finished = std::process::Command::new("true").spawn().unwrap();
        finished.wait().unwrap();
        let dead = finished.id();
        let file = config.join("sessions").join(format!("{dead}.json"));
        write(&file, json!({"pid":live(),"sessionId":A}));
        let mut processes = ClaudeProcesses::default();
        processes.refresh(&roots, 0);
        assert!(!processes.ended(A, None));
        // Its process was killed: the same file now names a dead pid.
        write(&file, json!({"pid":dead,"sessionId":A}));
        assert!(processes.refresh(&roots, 1_000));
        assert!(processes.ended(A, Some(500)));
        assert!(!processes.refresh(&roots, 2_000), "it ended once");
        // A stale file found at first sight ends its session too, and stays
        // ended past the usual hour while the file is there.
        let b = config.join("sessions").join("b.json");
        write(&b, json!({"pid":dead,"sessionId":B}));
        assert!(processes.refresh(&roots, 3_000));
        assert!(processes.ended(B, Some(2_500)));
        processes.refresh(&roots, 3_000 + GONE_KEPT_MS);
        assert!(processes.ended(B, Some(2_500)));
        assert!(processes.ended(A, Some(500)));
    }

    #[test]
    fn config_homes_are_the_claude_roots_parents_once_each() {
        let home = PathBuf::from("home");
        let roots = vec![
            (home.join(".claude").join("projects"), "claude"),
            (home.join(".claude").join("projects"), "claude"),
            (home.join(".codex").join("sessions"), "codex"),
            (home.join("work").join("projects"), "claude"),
        ];
        assert_eq!(
            config_homes(&roots),
            vec![home.join(".claude"), home.join("work")]
        );
    }
}
