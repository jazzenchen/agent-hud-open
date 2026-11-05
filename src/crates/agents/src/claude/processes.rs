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
//! the file says (`procStart`): a reused pid is another program. Every
//! Claude Code process of a version that writes the registry registers, a
//! headless `-p` run too, and `/clear` and `--resume` rename its session in
//! place; so a session its transcript says runs or waits, written by such a
//! version, that no running process claims, has no process: it has ended.
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
    status: Option<Live>,
    /// The pid's domain (`darwin`, `linux`, `win32`); another domain's pid
    /// says nothing of the processes here.
    domain: Option<String>,
    /// When the process started, in Unix seconds, as `procStart` says (UTC).
    started: Option<i64>,
    /// The Claude Code version that wrote the file.
    version: Option<Version>,
}

/// A Claude Code version, major, minor and patch.
type Version = (u64, u64, u64);

/// What a running Claude Code process says it does now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Live {
    Idle,
    Busy,
    /// Waiting for the user: an approval, a question's answer or a plan.
    Waiting,
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
            status: match value["status"].as_str() {
                Some("waiting") => Some(Live::Waiting),
                Some("busy") => Some(Live::Busy),
                Some("idle") => Some(Live::Idle),
                _ => None,
            },
            domain: text("pidDomain"),
            started: value["procStart"].as_str().and_then(|text| {
                chrono::NaiveDateTime::parse_from_str(text, "%a %b %e %H:%M:%S %Y")
                    .ok()
                    .map(|at| at.and_utc().timestamp())
            }),
            version: value["version"].as_str().and_then(version),
        }
    }

    /// Whether the file's process still runs: one on its pid, started when
    /// the file says. A file without a pid, or from another pid domain, says
    /// nothing of its process: it runs.
    fn runs(&self) -> bool {
        let Some(pid) = self.pid else {
            return true;
        };
        if self
            .domain
            .as_deref()
            .is_some_and(|domain| domain != PID_DOMAIN)
        {
            return true;
        }
        running(pid, self.started)
    }
}

/// This system's pid domain, as Claude Code names it (Node's platform).
const PID_DOMAIN: &str = if cfg!(target_os = "macos") {
    "darwin"
} else if cfg!(windows) {
    "win32"
} else {
    std::env::consts::OS
};

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
    revision: u64,
}

impl ClaudeProcesses {
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
            if entry.runs() {
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

/// Whether a process runs on `pid`, as signal 0 tells it (sent nowhere, only
/// checked), and on macOS whether it started when `started` says: a reused
/// pid is another program. A start this user cannot read is taken on trust.
#[cfg(unix)]
fn running(pid: i32, started: Option<i64>) -> bool {
    if pid <= 0 {
        return true;
    }
    // SAFETY: kill with signal 0 delivers nothing; it only checks the pid.
    let sent = unsafe { libc::kill(pid, 0) } == 0;
    // Another user's process cannot be signalled, but it runs.
    if !sent && std::io::Error::last_os_error().raw_os_error() != Some(libc::EPERM) {
        return false;
    }
    match (started, start_of(pid)) {
        (Some(said), Some(began)) => (said - began).abs() <= 2,
        _ => true,
    }
}
/// Where no signal can ask, every process the registry names is running.
#[cfg(not(unix))]
fn running(_pid: i32, _started: Option<i64>) -> bool {
    true
}

/// When a process of this user started, in Unix seconds.
#[cfg(target_os = "macos")]
fn start_of(pid: i32) -> Option<i64> {
    // SAFETY: proc_pidinfo fills at most `size` bytes of the zeroed struct.
    unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let size = std::mem::size_of_val(&info) as i32;
        (libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        ) == size
            && info.pbi_uid == libc::getuid())
        .then_some(info.pbi_start_tvsec as i64)
    }
}
#[cfg(all(unix, not(target_os = "macos")))]
fn start_of(_pid: i32) -> Option<i64> {
    None
}

/// `2.1.318` as a version; a suffix after the patch is left out.
fn version(text: &str) -> Option<Version> {
    let mut parts = text.split('.').map(|part| {
        part.split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|digits| digits.parse::<u64>().ok())
    });
    Some((parts.next()??, parts.next()??, parts.next()??))
}

/// The Claude Code version the log's latest lines name, from its last 16 KB.
fn last_version(log: &Path) -> Option<Version> {
    use std::io::{Read, Seek, SeekFrom};
    const TAIL: u64 = 16 * 1024;
    let mut file = fs::File::open(log).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(TAIL)))
        .ok()?;
    let mut tail = Vec::new();
    file.take(TAIL).read_to_end(&mut tail).ok()?;
    let tail = String::from_utf8_lossy(&tail);
    let key = "\"version\":\"";
    let at = tail.rfind(key)? + key.len();
    version(tail[at..].split('"').next()?)
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
    fn a_running_process_tells_what_its_session_does() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join(".claude");
        let roots = vec![(config.join("projects"), "claude")];
        let file = config.join("sessions").join("1.json");
        let mut processes = ClaudeProcesses::default();
        write(&file, json!({"pid":live(),"sessionId":A,"status":"busy"}));
        processes.refresh(&roots, 0);
        assert_eq!(processes.live(A), Some(Live::Busy));
        // It asks: a new word from the process is news.
        write(
            &file,
            json!({"pid":live(),"sessionId":A,"status":"waiting","waitingFor":"input needed"}),
        );
        let revision = processes.revision();
        assert!(processes.refresh(&roots, 1_000));
        assert!(processes.revision() != revision);
        assert_eq!(processes.live(A), Some(Live::Waiting));
        // Two processes on one session: the one that waits is told.
        write(
            &config.join("sessions").join("2.json"),
            json!({"pid":live(),"sessionId":A,"status":"idle"}),
        );
        processes.refresh(&roots, 2_000);
        assert_eq!(processes.live(A), Some(Live::Waiting));
        // A file that names no status, or a session no file names, says nothing.
        write(&file, json!({"pid":live(),"sessionId":B}));
        fs::remove_file(config.join("sessions").join("2.json")).unwrap();
        processes.refresh(&roots, 3_000);
        assert_eq!(processes.live(B), None);
        assert_eq!(processes.live(A), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_process_that_no_longer_runs_tells_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join(".claude");
        let roots = vec![(config.join("projects"), "claude")];
        let mut finished = std::process::Command::new("true").spawn().unwrap();
        finished.wait().unwrap();
        write(
            &config.join("sessions").join("1.json"),
            json!({"pid":finished.id(),"sessionId":A,"status":"waiting"}),
        );
        let mut processes = ClaudeProcesses::default();
        processes.refresh(&roots, 0);
        assert_eq!(processes.live(A), None);
        assert!(processes.ended(A, None));
    }

    #[test]
    fn versions_read_major_minor_and_patch() {
        assert_eq!(version("2.1.318"), Some((2, 1, 318)));
        assert_eq!(version("2.1.318-beta.2"), Some((2, 1, 318)));
        assert_eq!(version("2.1"), None);
        assert!(version("2.1.318") > version("2.1.99"));
    }

    /// A Claude log whose lines name the version that wrote them.
    fn log(dir: &Path, session: &str, wrote: &str) -> PathBuf {
        let path = dir.join(format!("{session}.jsonl"));
        let line = json!({"type":"user","sessionId":session,"version":wrote,"entrypoint":"cli"});
        fs::write(&path, format!("{line}\n{line}\n")).unwrap();
        path
    }

    #[test]
    fn a_session_no_running_process_claims_has_ended_if_its_version_registers() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join(".claude");
        let roots = vec![(config.join("projects"), "claude")];
        let mut processes = ClaudeProcesses::default();
        // Nothing has registered yet: no version is known to, so nothing ends.
        let a = log(temp.path(), A, "2.1.318");
        processes.refresh(&roots, 0);
        assert!(!processes.unclaimed([(A, a.as_path())], 0));
        assert!(!processes.ended(A, None));

        // A process of 2.1.300 runs another session: from 2.1.300 on, every
        // process registers, so A, written by 2.1.318 and claimed by none, ended.
        write(
            &config.join("sessions").join("1.json"),
            json!({"pid":live(),"sessionId":B,"version":"2.1.300"}),
        );
        processes.refresh(&roots, 1_000);
        let c = "77777777-8888-4999-aaaa-bbbbbbbbbbbb";
        let old = log(temp.path(), c, "2.0.9");
        assert!(processes.unclaimed([(A, a.as_path()), (c, old.as_path())], 1_000));
        assert!(processes.ended(A, Some(500)));
        // An older Claude Code may not register: its session is left alone.
        assert!(!processes.ended(c, None));
        // Claimed sessions are never in question.
        assert!(!processes.unclaimed([(B, a.as_path())], 2_000));
        assert!(!processes.ended(B, None));
    }

    /// A pid another program now runs on is not the process the file named.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_pid_whose_process_started_at_another_time_is_another_program() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join(".claude");
        let roots = vec![(config.join("projects"), "claude")];
        let began = start_of(live() as i32).unwrap();
        let stamp = |at: i64| {
            chrono::DateTime::from_timestamp(at, 0)
                .unwrap()
                .format("%a %b %e %H:%M:%S %Y")
                .to_string()
        };
        write(
            &config.join("sessions").join("1.json"),
            json!({"pid":live(),"sessionId":A,"procStart":stamp(began),"pidDomain":"darwin"}),
        );
        write(
            &config.join("sessions").join("2.json"),
            json!({"pid":live(),"sessionId":B,"procStart":stamp(began - 86_400),"pidDomain":"darwin"}),
        );
        let mut processes = ClaudeProcesses::default();
        processes.refresh(&roots, 0);
        assert!(!processes.ended(A, None), "the same process");
        assert!(processes.ended(B, None), "a reused pid");
        // A pid from another domain is not this Mac's to check.
        write(
            &config.join("sessions").join("2.json"),
            json!({"pid":live(),"sessionId":B,"procStart":stamp(began - 86_400),"pidDomain":"linux"}),
        );
        processes.refresh(&roots, 1_000);
        assert!(!processes.ended(B, None));
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
