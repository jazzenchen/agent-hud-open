//! Windows process ancestry through the local Win32_Process provider. Read only
//! this user's parents, stop at a reused PID, and never read process environments.
use super::super::ProcessAncestor;
use serde::Deserialize;
use std::{
    collections::HashMap,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Parent {
    executable: PathBuf,
    command: String,
}

const MAX_OUTPUT_BYTES: u64 = 1024 * 1024;

pub(crate) fn parent_processes() -> Vec<ProcessAncestor> {
    #[cfg(test)]
    let started = Instant::now();
    let Some(system) = std::env::var_os("SystemRoot") else {
        return Vec::new();
    };
    let mut command =
        Command::new(PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe"));
    super::configure_command(&mut command);
    command
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
        .arg(include_str!(
            "../../../scripts/windows/parent-processes.ps1"
        ))
        .env("AGENT_HUD_CALLER_PID", std::process::id().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(test)]
    command
        .env("AGENT_HUD_ANCESTRY_DIAGNOSTICS", "1")
        .stderr(Stdio::piped());
    let Ok(mut child) = super::spawn(&mut command) else {
        #[cfg(test)]
        eprintln!(
            "ancestry: spawn failed after {}ms",
            started.elapsed().as_millis()
        );
        return Vec::new();
    };
    #[cfg(test)]
    let diagnostics = {
        let stderr = child.stderr.take().unwrap();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr.take(16 * 1024).read_to_end(&mut bytes);
            let _ = sender.send(bytes);
        });
        eprintln!(
            "ancestry: spawned after {}ms",
            started.elapsed().as_millis()
        );
        receiver
    };
    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _read = stdout.take(MAX_OUTPUT_BYTES).read_to_end(&mut bytes);
        #[cfg(test)]
        eprintln!(
            "ancestry: stdout read {:?}",
            _read.map_err(|error| error.kind())
        );
        let _ = sender.send(bytes);
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                #[cfg(test)]
                eprintln!(
                    "ancestry: child exit {_status} after {}ms",
                    started.elapsed().as_millis()
                );
                break;
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _state => {
                #[cfg(test)]
                eprintln!(
                    "ancestry: stop state={_state:?} after {}ms",
                    started.elapsed().as_millis()
                );
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    let output = receiver.recv_timeout(Duration::from_millis(100));
    #[cfg(test)]
    {
        match &output {
            Ok(bytes) => eprintln!(
                "ancestry: stdout bytes={} complete-records={} after {}ms",
                bytes.len(),
                parents(bytes).count(),
                started.elapsed().as_millis()
            ),
            Err(error) => eprintln!("ancestry: stdout receive {error:?}"),
        }
        match diagnostics.recv_timeout(Duration::from_millis(100)) {
            Ok(bytes) => {
                for line in String::from_utf8_lossy(&bytes).lines() {
                    // Never relay a PowerShell exception, command line or path.
                    // Only the script's fixed stage names and numeric timing
                    // records belong in the native test's failure output.
                    if let Some(report) =
                        line.strip_prefix("agent-hud-ancestry: ").filter(|report| {
                            report.len() <= 160
                                && report.bytes().all(|byte| {
                                    byte.is_ascii_lowercase()
                                        || byte.is_ascii_digit()
                                        || matches!(byte, b' ' | b'-' | b'=')
                                })
                        })
                    {
                        eprintln!("ancestry: {report}");
                    }
                }
            }
            Err(error) => eprintln!("ancestry: diagnostic receive {error:?}"),
        }
    }
    let Ok(bytes) = output else {
        return Vec::new();
    };
    parents(&bytes)
        .map_while(|parent| {
            Some(ProcessAncestor {
                pid: None,
                executable: parent.executable,
                arguments: arguments(&parent.command)?,
                bundle: None,
            })
        })
        .collect()
}

/// Each completed record was checked for same-user ownership and PID reuse by
/// the script before it was emitted. Keep that prefix even if a later query
/// fails or times out; a partial or malformed record ends the trusted chain.
fn parents(bytes: &[u8]) -> impl Iterator<Item = Parent> + '_ {
    bytes
        .split_inclusive(|byte| *byte == b'\n')
        .take(16)
        .map_while(|line| {
            if line.last() != Some(&b'\n') {
                return None;
            }
            serde_json::from_slice(line).ok()
        })
}

/// Decode the command line using Windows' native quoting rules.
fn arguments(command: &str) -> Option<Vec<String>> {
    use windows_sys::Win32::Foundation::LocalFree;
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn CommandLineToArgvW(command: *const u16, count: *mut i32) -> *mut *mut u16;
    }
    if command.is_empty() || command.contains('\0') {
        return None;
    }
    let command = command.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut count = 0;
    let values = unsafe { CommandLineToArgvW(command.as_ptr(), &mut count) };
    if values.is_null() {
        return None;
    }
    let mut result = Vec::new();
    for index in 0..count {
        let value = unsafe { *values.add(index as usize) };
        let mut length = 0;
        unsafe {
            while *value.add(length) != 0 {
                length += 1;
            }
            result.push(String::from_utf16_lossy(std::slice::from_raw_parts(
                value, length,
            )));
        }
    }
    unsafe { LocalFree(values.cast()) };
    Some(result)
}

/// Another process's environment is not read on Windows. A daemon-served Codex
/// TUI therefore has no terminal target from a listening port.
pub(crate) fn codex_cli_on_port(
    _port: u16,
    _wanted_env: &[&str],
) -> Option<(Vec<String>, HashMap<String, String>)> {
    None
}

const WINDOWS_TO_UNIX_EPOCH_SECS: u64 = 11_644_473_600;
const TICKS_PER_SECOND: u64 = 10_000_000;

struct ProcessHandle(windows_sys::Win32::Foundation::HANDLE);
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a valid non-null handle returned by OpenProcess.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

fn filetime_to_unix_seconds(ft: windows_sys::Win32::Foundation::FILETIME) -> Option<i64> {
    let ticks = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
    let secs = ticks / TICKS_PER_SECOND;
    let unix = secs.checked_sub(WINDOWS_TO_UNIX_EPOCH_SECS)?;
    i64::try_from(unix).ok()
}

fn creation_unix_seconds(handle: windows_sys::Win32::Foundation::HANDLE) -> Option<i64> {
    use windows_sys::Win32::{Foundation::FILETIME, System::Threading::GetProcessTimes};
    let mut created = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exited = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut kernel = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut user = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    // SAFETY: `handle` is a valid process handle with PROCESS_QUERY_LIMITED_INFORMATION.
    let ok = unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) };
    (ok != 0)
        .then(|| filetime_to_unix_seconds(created))
        .flatten()
}

/// Whether a process still runs on `pid` and started when `started` says (in
/// Unix seconds, +/- 2 s tolerance for PID-reuse checks). Matches Unix `pids`:
/// `pid <= 0` or an unopenable elevated/other-user process (`ERROR_ACCESS_DENIED`)
/// is taken on trust; exited or missing processes return `false`.
pub(crate) fn process_runs(pid: i32, started: Option<i64>) -> bool {
    use windows_sys::Win32::{
        Foundation::{GetLastError, ERROR_ACCESS_DENIED, STILL_ACTIVE},
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    if pid <= 0 {
        return true;
    }
    // SAFETY: OpenProcess only requests limited query access to `pid`.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32) };
    if handle.is_null() {
        return unsafe { GetLastError() } == ERROR_ACCESS_DENIED;
    }
    let handle = ProcessHandle(handle);
    let mut code = 0u32;
    if unsafe { GetExitCodeProcess(handle.0, &mut code) } == 0 || code != STILL_ACTIVE as u32 {
        return false;
    }
    match (started, creation_unix_seconds(handle.0)) {
        (Some(said), Some(began)) => (said - began).abs() <= 2,
        _ => true,
    }
}

/// When the running process on `pid` started, in Unix seconds.
pub(crate) fn process_start(pid: i32) -> Option<i64> {
    use windows_sys::Win32::{
        Foundation::STILL_ACTIVE,
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    if pid <= 0 {
        return None;
    }
    // SAFETY: OpenProcess only requests limited query access to `pid`.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32) };
    if handle.is_null() {
        return None;
    }
    let handle = ProcessHandle(handle);
    let mut code = 0u32;
    if unsafe { GetExitCodeProcess(handle.0, &mut code) } == 0 || code != STILL_ACTIVE as u32 {
        return None;
    }
    creation_unix_seconds(handle.0)
}

/// Not asked on Windows yet: no process counts as reading a terminal, so a
/// hook that waits only beside one leaves at once.
pub(crate) fn terminal_io(_pid: i32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestry_keeps_complete_records_before_a_partial_record() {
        let first = br#"{"executable":"C:\\first.exe","command":"first.exe"}"#;
        let second = br#"{"executable":"C:\\second.exe","command":"second.exe"}"#;
        let mut bytes = first.to_vec();
        bytes.extend_from_slice(b"\r\n");
        bytes.extend_from_slice(second);
        bytes.push(b'\n');
        // A syntactically valid JSON object without its line terminator is
        // still incomplete: the query may have been cut off during output.
        bytes.extend_from_slice(first);
        let parsed = parents(&bytes).collect::<Vec<_>>();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].executable, PathBuf::from(r"C:\first.exe"));
        assert_eq!(parsed[1].command, "second.exe");
        assert_eq!(parents(&bytes[..bytes.len() - 5]).count(), 2);
    }

    #[test]
    fn ancestry_stops_at_invalid_json_or_utf8_without_skipping_ancestors() {
        let valid = b"{\"executable\":\"parent.exe\",\"command\":\"parent.exe\"}\n";
        for invalid in [b"{broken}\n".as_slice(), b"\xff\n"] {
            let mut bytes = valid.to_vec();
            bytes.extend_from_slice(invalid);
            bytes.extend_from_slice(valid);
            assert_eq!(parents(&bytes).count(), 1);
        }
        assert_eq!(parents(&valid.repeat(17)).count(), 16);
    }

    #[test]
    fn native_argv_preserves_unicode_spaces_empty_values_and_backslashes() {
        assert_eq!(
            arguments(
                r#""C:\Program Files\Claude\claude.exe" --model "模型 one" "" "C:\my project\\""# // l10n-ignore: test data
            ),
            Some(vec![
                r"C:\Program Files\Claude\claude.exe".into(),
                "--model".into(),
                "模型 one".into(), // l10n-ignore: test data
                "".into(),
                "C:\\my project\\".into(),
            ])
        );
        assert!(arguments("").is_none());
        assert!(arguments("codex\0daemon").is_none());
    }

    #[test]
    fn ancestry_json_strings_preserve_windows_paths_quotes_and_control_characters() {
        // Execute the probe's own encoder without CIM or another module. The
        // native ancestry test below covers the complete bounded process read.
        let script = include_str!("../../../scripts/windows/parent-processes.ps1");
        let functions = script.split_once("\ntrap {").unwrap().0;
        let input = "C:\\用户\\my project\\agent.exe \"quoted\"\r\n\t\u{1}"; // l10n-ignore: test data
        let system = std::env::var_os("SystemRoot").unwrap();
        let output = Command::new(
            PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
        )
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
        .arg(format!(
            "{functions}\n[Console]::Out.WriteLine((Json-String $env:AGENT_HUD_TEST_JSON_STRING))"
        ))
        .env("AGENT_HUD_TEST_JSON_STRING", input)
        .output()
        .unwrap();
        assert!(output.status.success());
        assert_eq!(
            serde_json::from_slice::<String>(&output.stdout).unwrap(),
            input
        );
    }

    #[test]
    fn ancestry_reads_the_same_user_parent_in_a_real_windows_process() {
        if std::env::var_os("AGENT_HUD_TEST_ANCESTRY_CHILD").is_some() {
            let parents = parent_processes();
            let parent = parents
                .first()
                .expect("the same-user test parent must be visible");
            assert_eq!(
                parent.executable.canonicalize().unwrap(),
                std::env::current_exe().unwrap().canonicalize().unwrap()
            );
            assert!(!parent.arguments.is_empty());
            return;
        }
        let name = format!(
            "{}::ancestry_reads_the_same_user_parent_in_a_real_windows_process",
            module_path!().split_once("::").unwrap().1
        );
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &name, "--nocapture"])
            .env("AGENT_HUD_TEST_ANCESTRY_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child status: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn filetime_converts_epoch_and_modern_timestamps() {
        use windows_sys::Win32::Foundation::FILETIME;
        let from_ticks = |ticks: u64| FILETIME {
            dwLowDateTime: ticks as u32,
            dwHighDateTime: (ticks >> 32) as u32,
        };
        assert_eq!(filetime_to_unix_seconds(from_ticks(0)), None);
        assert_eq!(
            filetime_to_unix_seconds(from_ticks(WINDOWS_TO_UNIX_EPOCH_SECS * TICKS_PER_SECOND)),
            Some(0)
        );
        let sample: u64 = 1_750_000_000;
        assert_eq!(
            filetime_to_unix_seconds(from_ticks(
                (WINDOWS_TO_UNIX_EPOCH_SECS + sample) * TICKS_PER_SECOND
            )),
            Some(sample as i64)
        );
    }

    #[test]
    fn a_pid_runs_while_its_process_does_and_started_when_said() {
        let own = std::process::id() as i32;
        let began = process_start(own).expect("current process start time is readable");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert!(began <= now + 2 && began >= now - 86_400, "began={began}");
        assert!(process_runs(own, None));
        assert!(process_runs(own, Some(began)), "the same process");
        assert!(process_runs(own, Some(began + 2)), "within tolerance");
        assert!(!process_runs(own, Some(began - 86_400)), "a reused pid");
        assert!(process_runs(0, None));
        assert!(process_runs(-1, None));
        assert_eq!(process_start(0), None);
        assert_eq!(process_start(-1), None);

        let system = std::env::var_os("SystemRoot").unwrap();
        let mut child = Command::new(PathBuf::from(system).join("System32/cmd.exe"))
            .args(["/C", "exit", "0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        child.wait().unwrap();
        // While `child` still holds the process handle (exited, exit code 0):
        assert!(!process_runs(pid, None), "exited process with open handle");
        assert_eq!(process_start(pid), None);
        // After closing the handle:
        drop(child);
        assert!(!process_runs(pid, None), "exited process after close");
    }
}
