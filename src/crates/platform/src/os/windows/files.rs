//! Owner-only files on Windows: the owner SID and a protected per-user DACL.
use super::pipe;
use std::{
    fs,
    io::{self, Read},
    path::Path,
};

/// A directory only this user can open, created when absent. A reparse point
/// or another user's directory is refused.
pub(crate) fn private_directory(path: &Path) -> Result<(), String> {
    pipe::private_directory(path)
}

pub(crate) fn owned_by_user(path: &Path, _metadata: &fs::Metadata) -> bool {
    pipe::owned_path(path)
}

/// Files inherit the private directory's protected DACL, so ownership is the check.
pub(crate) fn owner_only(path: &Path, metadata: &fs::Metadata) -> bool {
    owned_by_user(path, metadata)
}

/// New files have an explicit current-user owner even in elevated processes.
pub(crate) fn private_file(path: &Path, create_new: bool) -> io::Result<fs::File> {
    pipe::private_file(path, create_new)
}

/// A private file opened at its end for appending. No hook that appends
/// (Cursor's) is launched on Windows yet, so writers do not overlap here.
pub(crate) fn private_append(path: &Path) -> io::Result<fs::File> {
    use std::io::{Seek, SeekFrom};
    let mut file = pipe::private_file(path, false)?;
    file.seek(SeekFrom::End(0))?;
    Ok(file)
}

/// A hook payload from stdin, or Err after five seconds or past `limit` bytes.
/// Windows console and pipe handles cannot be polled, so a thread reads.
///
/// A payload is one JSON object, and clients on Windows (Claude Code among
/// them) may write it without closing stdin, so the payload is taken as soon
/// as it is a complete JSON value instead of waiting out the five seconds.
pub(crate) fn read_stdin(bytes: &mut Vec<u8>, limit: usize) -> Result<(), ()> {
    use std::time::{Duration, Instant};
    let (sender, receiver) = std::sync::mpsc::channel::<Option<Vec<u8>>>();
    std::thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut chunk = vec![0u8; 64 * 1024];
        loop {
            match stdin.read(&mut chunk) {
                Ok(0) | Err(_) => {
                    let _ = sender.send(None);
                    return;
                }
                Ok(count) => {
                    if sender.send(Some(chunk[..count].to_vec())).is_err() {
                        return;
                    }
                }
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut input = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(left) {
            Ok(Some(chunk)) => {
                input.extend_from_slice(&chunk);
                if input.len() > limit {
                    return Err(());
                }
                if complete_json(&input) {
                    break;
                }
            }
            // End of input: whatever arrived is the payload.
            Ok(None) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => return Err(()),
        }
    }
    *bytes = input;
    Ok(())
}

/// Whether the bytes are one whole JSON value, surrounding whitespace aside.
fn complete_json(bytes: &[u8]) -> bool {
    serde_json::from_slice::<serde::de::IgnoredAny>(bytes).is_ok()
}

/// The client's hook entry names the executable directly; no shell quoting.
pub(crate) fn hook_configuration(
    source: &str,
    flag: &str,
    current: &serde_json::Value,
    executable: Option<&Path>,
) -> Result<serde_json::Value, String> {
    agent_hud_agents::patch_configuration_for_windows(
        source,
        flag,
        current,
        executable.and_then(Path::to_str),
    )
}

#[cfg(test)]
mod tests {
    use super::complete_json;

    #[test]
    fn a_payload_is_taken_once_its_json_is_whole() {
        assert!(complete_json(b"{\"a\":{\"b\":[1,2]}}\r\n"));
        assert!(!complete_json(b"{\"a\":{\"b\":[1,2]"));
        assert!(!complete_json(b""));
        assert!(!complete_json(b"{\"text\":\"a } inside\""));
    }
}
