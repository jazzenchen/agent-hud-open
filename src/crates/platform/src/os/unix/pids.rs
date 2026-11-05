//! Whether a process still runs on a POSIX host, for Claude Code's process
//! registry: signal 0 asks the pid, and each host reads its start its own way.

/// Whether a process runs on `pid`, as signal 0 tells it (sent nowhere, only
/// checked), and whether it started when `started` says, as `start_of` reads
/// it: a reused pid is another program. A start this user cannot read is
/// taken on trust.
pub(crate) fn pid_runs(pid: i32, started: Option<i64>, start_of: fn(i32) -> Option<i64>) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pid_runs_while_its_process_does_and_started_when_said() {
        let own = std::process::id() as i32;
        assert!(pid_runs(own, None, |_| None));
        // A start nobody can read is taken on trust.
        assert!(pid_runs(own, Some(1_000), |_| None));
        assert!(pid_runs(own, Some(1_001), |_| Some(1_000)));
        assert!(!pid_runs(own, Some(1_000 - 86_400), |_| Some(1_000)));
        let mut finished = std::process::Command::new("true").spawn().unwrap();
        finished.wait().unwrap();
        assert!(!pid_runs(finished.id() as i32, None, |_| None));
    }
}
