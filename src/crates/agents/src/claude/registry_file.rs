//! One file of Claude Code's process registry, `sessions/<pid>.json`, as
//! last read: the process it names, the session that process runs, what it
//! says it does, and the Claude Code version that wrote it.
use super::processes::{Live, Pids};
use crate::Os;
use serde_json::Value;
use std::{fs, path::Path, time::SystemTime};

/// A Claude Code version, major, minor and patch.
pub(super) type Version = (u64, u64, u64);

/// One registry file as last read: its stamp, and what it says.
#[derive(Clone)]
pub(super) struct Entry {
    pub(super) stamp: (Option<SystemTime>, u64),
    pid: Option<i32>,
    pub(super) session: Option<String>,
    pub(super) kind: Option<String>,
    pub(super) entrypoint: Option<String>,
    pub(super) status: Option<Live>,
    /// The pid's domain (`darwin`, `linux`, `win32`); another domain's pid
    /// says nothing of the processes here.
    domain: Option<String>,
    /// When the process started, in Unix seconds, as `procStart` says (UTC).
    started: Option<i64>,
    /// The Claude Code version that wrote the file.
    pub(super) version: Option<Version>,
}

impl Entry {
    pub(super) fn read(path: &Path, stamp: (Option<SystemTime>, u64)) -> Self {
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

    /// Whether the file's process still runs, as the host tells it: one on
    /// its pid, started when the file says. A file without a pid, or from
    /// another pid domain, says nothing of its process: it runs. Without a
    /// host to ask, every process runs.
    pub(super) fn runs(&self, pids: Option<&dyn Pids>) -> bool {
        let (Some(pid), Some(pids)) = (self.pid, pids) else {
            return true;
        };
        if self
            .domain
            .as_deref()
            .is_some_and(|domain| domain != pid_domain(pids.os()))
        {
            return true;
        }
        pids.runs(pid, self.started)
    }
}

/// The pid domain Claude Code writes for each operating system: Node's
/// platform name.
fn pid_domain(os: Os) -> &'static str {
    match os {
        Os::MacOs => "darwin",
        Os::Windows => "win32",
        Os::Linux => "linux",
    }
}

/// `2.1.318` as a version; a suffix after the patch is left out.
pub(super) fn version(text: &str) -> Option<Version> {
    let mut parts = text.split('.').map(|part| {
        part.split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|digits| digits.parse::<u64>().ok())
    });
    Some((parts.next()??, parts.next()??, parts.next()??))
}

/// The Claude Code version the log's latest lines name, from its last 16 KB.
pub(super) fn last_version(log: &Path) -> Option<Version> {
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
