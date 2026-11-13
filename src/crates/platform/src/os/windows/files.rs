//! Owner-only files on Windows: the owner SID and a protected per-user DACL.
use super::acl;
use std::{
    fs,
    io::{self, Read},
    path::Path,
};

/// A directory only this user can open, created when absent. A reparse point
/// or another user's directory is refused.
pub(crate) fn private_directory(path: &Path) -> Result<(), String> {
    acl::private_directory(path)
}

pub(crate) fn owned_by_user(path: &Path, _metadata: &fs::Metadata) -> bool {
    acl::owned_path(path)
}

/// A client's transcript this user may read: owned by this user, or by the
/// owner Windows gives this process's files (for an elevated administrator,
/// the Administrators group), as an elevated Claude Code's transcripts are.
/// Only transcript reads accept that; the app's own folders never do.
pub(crate) fn user_transcript(path: &Path, metadata: &fs::Metadata) -> bool {
    owned_by_user(path, metadata) || acl::owned_by_default_owner(path)
}

/// Files inherit the private directory's protected DACL, so ownership is the check.
pub(crate) fn owner_only(path: &Path, metadata: &fs::Metadata) -> bool {
    owned_by_user(path, metadata)
}

/// New files have an explicit current-user owner even in elevated processes.
pub(crate) fn private_file(path: &Path, create_new: bool) -> io::Result<fs::File> {
    acl::private_file(path, create_new)
}

/// A private file opened at its end for appending. No hook that appends
/// (Cursor's) is launched on Windows yet, so writers do not overlap here.
pub(crate) fn private_append(path: &Path) -> io::Result<fs::File> {
    use std::io::{Seek, SeekFrom};
    let mut file = acl::private_file(path, false)?;
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

/// A client whose settings are text has no Windows launch contract yet:
/// its handlers can only be taken out here.
pub(crate) fn hook_text(
    source: &str,
    flag: &str,
    original: &str,
    executable: Option<&Path>,
) -> Result<String, String> {
    if executable.is_some() {
        return Err(format!(
            "{source} has no verified Windows hook launch contract; its configuration was not changed"
        ));
    }
    agent_hud_agents::patch_text(source, flag, original, None)
}

#[cfg(test)]
mod tests {
    use super::{complete_json, user_transcript};

    /// What this process writes is owned by the user, or for an elevated
    /// process by the Administrators group; a transcript read accepts both.
    #[test]
    fn a_transcript_this_process_writes_is_readable_elevated_or_not() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("t.jsonl");
        std::fs::write(&path, b"{}\n").unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        assert!(user_transcript(&path, &metadata));
        assert!(!user_transcript(&temp.path().join("missing"), &metadata));
    }

    #[test]
    fn a_payload_is_taken_once_its_json_is_whole() {
        assert!(complete_json(b"{\"a\":{\"b\":[1,2]}}\r\n"));
        assert!(!complete_json(b"{\"a\":{\"b\":[1,2]"));
        assert!(!complete_json(b""));
        assert!(!complete_json(b"{\"text\":\"a } inside\""));
    }
}
