//! Linux process inspection through /proc. Only processes of the current user
//! are read.
use super::super::ProcessAncestor;
use std::{
    collections::{HashMap, HashSet},
    os::unix::fs::MetadataExt,
    path::PathBuf,
};

/// The hook's parent processes, nearest first, up to 16 or the first one that
/// belongs to another user. Linux has no application bundles.
pub(crate) fn parent_processes() -> Vec<ProcessAncestor> {
    let mut pid = unsafe { libc::getppid() };
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    while pid > 1 && result.len() < 16 && seen.insert(pid) {
        let directory = PathBuf::from(format!("/proc/{pid}"));
        if std::fs::metadata(&directory).map(|m| m.uid()).ok() != Some(unsafe { libc::getuid() }) {
            break;
        }
        let Ok(executable) = std::fs::read_link(directory.join("exe")) else {
            break;
        };
        let Ok(bytes) = std::fs::read(directory.join("cmdline")) else {
            break;
        };
        if bytes.len() > 1024 * 1024 {
            break;
        }
        let arguments = bytes
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect();
        let Ok(status) = std::fs::read_to_string(directory.join("status")) else {
            break;
        };
        let Some(parent) = status.lines().find_map(|l| {
            l.strip_prefix("PPid:")
                .and_then(|p| p.trim().parse::<i32>().ok())
        }) else {
            break;
        };
        result.push(ProcessAncestor {
            pid: u32::try_from(pid).ok(),
            executable,
            arguments,
            bundle: None,
        });
        pid = parent;
    }
    result
}

/// Not implemented on Linux: reading another process's environment is not
/// done here, so a daemon-served Codex TUI has no terminal target.
pub(crate) fn codex_cli_on_port(
    _port: u16,
    _wanted_env: &[&str],
) -> Option<(Vec<String>, HashMap<String, String>)> {
    None
}

/// Whether a process runs on `pid`, for Claude Code's process registry:
/// signal 0 only. No start is read yet, so a reused pid passes for the
/// registered process; `/proc/<pid>/stat`'s start time (clock ticks after
/// boot) would tell them apart.
pub(crate) fn process_runs(pid: i32, started: Option<i64>) -> bool {
    super::super::unix::pid_runs(pid, started, |_| None)
}

/// No start is read here yet (see `process_runs`).
pub(crate) fn process_start(_pid: i32) -> Option<i64> {
    None
}

/// Whether the stdin and stdout of the process on `pid` are terminals: both
/// link to a pseudo-terminal or a console.
pub(crate) fn terminal_io(pid: i32) -> bool {
    [0, 1].into_iter().all(|fd| {
        std::fs::read_link(format!("/proc/{pid}/fd/{fd}")).is_ok_and(|target| {
            target.starts_with("/dev/pts") || target.to_string_lossy().starts_with("/dev/tty")
        })
    })
}
