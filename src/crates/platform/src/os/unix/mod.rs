//! Shared POSIX process transport. Host-specific command sources live beside
//! each host module; provider identity/flags remain in the account adapter.
use super::{Environment, ProcessInfo};
mod files;
mod pids;
pub(crate) use files::{
    hook_configuration, hook_text, owned_by_user, owner_only, private_append, private_directory,
    private_file, read_stdin, user_transcript,
};
pub(crate) use pids::pid_runs;
use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

pub(crate) fn environment_value<'a>(env: &'a Environment, key: &str) -> Option<&'a str> {
    env.get(key).map(String::as_str)
}
pub(crate) fn is_shell(path: &Path) -> bool {
    matches!(path.to_str(), Some("/bin/sh" | "/bin/bash" | "/bin/zsh"))
}
pub(super) fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}
fn script_command(script: &str, args: &[&OsStr]) -> Command {
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", script, "agenthud-os"])
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    command
}
pub(super) fn spawn_script(script: &str, args: &[&OsStr]) -> Result<(), String> {
    // Starting sh only proves the wrapper exists. Report success only after the
    // actual fixed operation exits successfully within the existing bound.
    inspect(script, args)
        .map(|_| ())
        .map_err(|_| "Could not reveal transcript".into())
}
/// Output may contain a local token. It stays in memory, bounded to 2 MiB,
/// and is never copied into errors or diagnostics.
fn inspect(script: &str, args: &[&OsStr]) -> Result<String, String> {
    let mut child = script_command(script, args)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|_| "Local service process inspection is unavailable.".to_owned())?;
    let output = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::sync_channel(1);
    const LIMIT: u64 = 2 * 1024 * 1024;
    if std::thread::Builder::new()
        .name("agenthud-os-inspect".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let result = output.take(LIMIT + 1).read_to_end(&mut bytes);
            let _ = sender.send((result.is_ok(), bytes));
        })
        .is_err()
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Local service process inspection is unavailable.".into());
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let (read, bytes) = receiver
        .recv_timeout(Duration::from_millis(100))
        .map_err(|_| "Local service process inspection did not complete.".to_owned())?;
    if !success || !read || bytes.len() as u64 > LIMIT {
        return Err("Local service process inspection did not complete.".into());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
fn rows(output: &str) -> HashMap<u32, &str> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let at = line.find(char::is_whitespace)?;
            Some((line[..at].parse().ok()?, line[at..].trim()))
        })
        .collect()
}
pub(super) fn processes(
    names_script: &str,
    arguments_script: &str,
) -> Result<Vec<ProcessInfo>, String> {
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    let uid = unsafe { getuid() }.to_string();
    let args = [OsStr::new(&uid)];
    let names = inspect(names_script, &args)?;
    let commands = inspect(arguments_script, &args)?;
    let names = rows(&names);
    Ok(rows(&commands)
        .into_iter()
        .filter_map(|(pid, command)| {
            Some(ProcessInfo {
                pid,
                executable: names.get(&pid)?.to_string(),
                command: command.into(),
            })
        })
        .collect())
}
pub(super) fn listening_ports(script: &str, pid: u32) -> Result<Vec<u16>, String> {
    let pid = pid.to_string();
    let output = inspect(script, &[OsStr::new(&pid)])?;
    let mut ports = output
        .lines()
        .filter(|line| line.starts_with('n'))
        .filter_map(|line| line.rsplit(':').next()?.parse::<u16>().ok())
        .filter(|port| *port != 0)
        .collect::<Vec<_>>();
    ports.sort_unstable();
    ports.dedup();
    Ok(ports)
}
/// Entries of `directory`, newest name first.
/// The visible entries of `directory`, highest version number first:
/// numbers within names compare by value, so 2.0.10 comes before 2.0.9
/// (agent-hud-native-open ClaudeEngineLocator.newestFirst).
pub(super) fn newest(directory: &Path) -> Vec<PathBuf> {
    let mut entries = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| !name.starts_with('.'))
        })
        .collect::<Vec<_>>();
    let name = |path: &PathBuf| {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_owned()
    };
    entries.sort_by(|a, b| numeric_order(&name(b), &name(a)));
    entries
}

/// Compares two names with their runs of digits read as numbers, as
/// Foundation's `.numeric` comparison does.
pub(super) fn numeric_order(a: &str, b: &str) -> std::cmp::Ordering {
    fn runs(text: &str) -> Vec<(bool, &str)> {
        let mut runs = Vec::new();
        let mut start = 0;
        let bytes = text.as_bytes();
        for index in 1..=bytes.len() {
            if index == bytes.len()
                || bytes[index].is_ascii_digit() != bytes[start].is_ascii_digit()
            {
                runs.push((bytes[start].is_ascii_digit(), &text[start..index]));
                start = index;
            }
        }
        runs
    }
    let (left, right) = (runs(a), runs(b));
    for ((digits_a, run_a), (digits_b, run_b)) in left.iter().zip(&right) {
        let order = if *digits_a && *digits_b {
            let (a, b) = (run_a.trim_start_matches('0'), run_b.trim_start_matches('0'));
            a.len().cmp(&b.len()).then_with(|| a.cmp(b))
        } else {
            run_a.cmp(run_b)
        };
        if order != std::cmp::Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

#[cfg(test)]
mod version_tests {
    use super::*;

    #[test]
    fn versions_sort_by_their_numbers_newest_first() {
        let directory = tempfile::tempdir().unwrap();
        for name in ["2.0.9", "2.0.10", "1.9.99", ".partial", "2.1.0"] {
            std::fs::create_dir(directory.path().join(name)).unwrap();
        }
        let names = newest(directory.path())
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, ["2.1.0", "2.0.10", "2.0.9", "1.9.99"]);
    }
}
