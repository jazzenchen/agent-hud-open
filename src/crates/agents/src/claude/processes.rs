//! Claude Code's process registry: every live Claude Code process, in a
//! terminal or in the desktop app, writes `sessions/<pid>.json` under its
//! config home naming its session. It removes the file as it exits cleanly;
//! a process killed outright, as a closed terminal tab kills it, leaves the
//! file behind with a pid that no longer runs (Claude Code clears such files
//! the next time it starts). A session with no file whose process runs has no
//! one to go on with it: what it waited for will not come. A running process
//! also says what it does now (`status`): waiting for the user, busy, or
//! idle. That is its own word, so a session it says waits waits for as long
//! as it says so, past the transcript's half hour.
//!
//! A file's process counts only while the process on its pid started when
//! the file says (`procStart`): a reused pid is another program. Whether it
//! does is the host's to tell (`Pids`), as only the operating system knows.
//! Every Claude Code process of a version that writes the registry
//! registers, a headless `-p` run too, and `/clear` and `--resume` rename its
//! session in place; so a session its transcript says runs or waits, written
//! by such a version, that no running process claims, has no process: it has
//! ended. Read on every collection pass, each file again only when it
//! changed; each file's process is asked whether it still runs every pass.
use super::registry_file::{last_version, Entry, Version};
use crate::Os;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

/// How long a session's last exit is kept: past it, the transcript's own
/// abandonment has made the session idle anyway.
const GONE_KEPT_MS: i64 = 3_600_000;

/// What a running Claude Code process says it does now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Live {
    Idle,
    Busy,
    /// Waiting for the user: an approval, a question's answer or a plan.
    Waiting,
}

/// The host's word on the processes the registry names. The platform layer
/// answers it for the operating system Agent HUD runs on; where it cannot
/// ask, it says every process runs.
pub trait Pids: Send + Sync {
    /// The operating system whose pids these are.
    fn os(&self) -> Os;
    /// Whether a process runs on `pid` that started when `started` says, in
    /// Unix seconds: a reused pid is another program. A start the host
    /// cannot read is taken on trust.
    fn runs(&self, pid: i32, started: Option<i64>) -> bool;
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
    /// What the running processes say each session does, the most pressing
    /// first when several name one: waiting, then busy, then idle.
    live: HashMap<String, Live>,
    /// Sessions whose last process left, and when that was first seen.
    gone: HashMap<String, i64>,
    /// The oldest Claude Code version seen writing the registry: from it on,
    /// every process registers.
    registering: Option<Version>,
    /// Sessions `unclaimed` looked at, with their log's length then, so that
    /// a log is read again only once it grew.
    checked: HashMap<String, u64>,
    /// The host that tells whether a file's process still runs; without
    /// one, every process the registry names runs.
    pids: Option<Arc<dyn Pids>>,
    revision: u64,
}

impl ClaudeProcesses {
    /// Lets the host tell whether each file's process still runs.
    pub fn set_pids(&mut self, pids: Arc<dyn Pids>) {
        self.pids = Some(pids);
    }

    /// Reads the registry of every Claude config home among `provider_roots`.
    /// True when a session lost its last process or got one back, or a
    /// running process says something new of what its session does.
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
        if let Some(oldest) = self.files.values().filter_map(|entry| entry.version).min() {
            self.registering = Some(self.registering.map_or(oldest, |known| known.min(oldest)));
        }
        let (mut held, mut dead) = (HashSet::new(), HashSet::new());
        let mut live = HashMap::<String, Live>::new();
        for entry in self.files.values() {
            let Some(session) = entry.session.clone() else {
                continue;
            };
            // A file without a pid says nothing of its process: it holds.
            if entry.runs(self.pids.as_deref()) {
                if let Some(status) = entry.status {
                    let kept = live.entry(session.clone()).or_insert(status);
                    *kept = (*kept).max(status);
                }
                held.insert(session);
            } else {
                dead.insert(session);
            }
        }
        let mut changed = live != self.live;
        self.live = live;
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

    /// The sessions among `logs` (session, log) that their transcript says run
    /// or wait which no running process claims: written by a version that
    /// registers, each has gone, first seen now. True when one has.
    pub fn unclaimed<'a>(
        &mut self,
        logs: impl IntoIterator<Item = (&'a str, &'a Path)>,
        now: i64,
    ) -> bool {
        let Some(registering) = self.registering else {
            return false;
        };
        let mut checked = HashMap::new();
        let mut changed = false;
        for (session, log) in logs {
            if self.held.contains(session) || self.gone.contains_key(session) {
                continue;
            }
            let length = fs::metadata(log).map(|meta| meta.len()).unwrap_or(0);
            let seen = self.checked.get(session) == Some(&length);
            checked.insert(session.to_owned(), length);
            if !seen && last_version(log).is_some_and(|wrote| wrote >= registering) {
                self.gone.insert(session.to_owned(), now);
                changed = true;
            }
        }
        self.checked = checked;
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

    /// What a running process says the session does now; none when no
    /// running process names it or none says.
    pub fn live(&self, session: &str) -> Option<Live> {
        self.live.get(session).copied()
    }

    /// Goes up whenever `ended` or `live` may answer differently.
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
#[path = "processes_tests.rs"]
mod tests;
