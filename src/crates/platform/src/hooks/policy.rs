//! The user's own rules in a client's settings, which a permission hook
//! reads before the call goes to the HUD. The client's rule decides what
//! they say (`agents::hooks::Policy`); this only reads its files, each with a
//! size limit.
use agent_hud_agents::hooks::Files;
use serde_json::Value;
use std::{collections::HashMap, io::Read, path::Path};

/// Applies the client's policy to a permission callback: false when a rule
/// settles the call and the hook answers nothing.
pub(super) fn screen(
    source: &str,
    payload: &mut Value,
    home: Option<&Path>,
    env: &HashMap<String, String>,
) -> bool {
    let read = |path: &Path, limit: u64| read_file(path, limit);
    let files = home.map(|home| Files {
        home,
        env,
        read: &read,
    });
    agent_hud_agents::hooks::screen(source, payload, files.as_ref())
}

/// A plain file's first `limit + 1` bytes, links followed; None for a
/// folder, a pipe whose reader would wait, or a file that cannot be read.
fn read_file(path: &Path, limit: u64) -> Option<Vec<u8>> {
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn files_are_read_up_to_one_byte_past_the_limit() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");
        std::fs::write(&path, b"0123456789").unwrap();
        assert_eq!(read_file(&path, 64).unwrap(), b"0123456789");
        assert_eq!(read_file(&path, 4).unwrap(), b"01234");
        assert!(read_file(&temp.path().join("missing.json"), 64).is_none());
        let folder = temp.path(); // plain directory
        assert!(read_file(folder, 64).is_none());
    }

    #[test]
    fn a_client_without_rules_keeps_its_callback_and_loses_any_mark() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let mark = agent_hud_agents::hooks::RULE_ASKS;
        let mut payload = json!({"session_id": "s1", "tool_name": "Bash", mark: true});
        assert!(screen(
            "unknown-client",
            &mut payload,
            Some(&home),
            &HashMap::new()
        ));
        assert!(payload.get(mark).is_none());
        assert_eq!(payload["tool_name"], "Bash");
    }
}
