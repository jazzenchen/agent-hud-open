//! Whether a hook comes from a client the person turned off in Settings
//! (`agents::split`). The switches are read from the file Agent HUD saves them
//! in, so a client that is off passes through its hooks with no opinion and
//! nothing recorded, whether or not the app is running.
use agent_hud_agents::split;
use serde_json::Value;
use std::{collections::HashMap, path::Path};

pub(super) fn turned_off(
    data: &Path,
    source: &str,
    home: Option<&Path>,
    env: &HashMap<String, String>,
) -> bool {
    let Some(agent) = super::origins::agent_id(source) else {
        return false;
    };
    let saved = saved_switches(data);
    let switch = |id: &str| saved.get(id).copied();
    if split::ids(agent).iter().all(|id| split::is_on(id, &switch)) {
        return false;
    }
    if !split::agent_on(agent, &switch) {
        return true;
    }
    // Some of the agent's clients are off: the caller's own client decides,
    // and one its rule cannot tell stays with the agent, which is on.
    super::origins::caller_client(source, home, env).is_some_and(|id| !split::is_on(id, &switch))
}

/// The switches Settings saved, by client id; none when the file is missing
/// or unreadable, where every client is on.
fn saved_switches(data: &Path) -> HashMap<String, bool> {
    std::fs::read(data.join("settings.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|stored| serde_json::from_value(stored["providers"].clone()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_with(providers: Value) -> tempfile::TempDir {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(
            data.path().join("settings.json"),
            serde_json::to_vec(&serde_json::json!({"settings":{},"providers":providers})).unwrap(),
        )
        .unwrap();
        data
    }

    /// An agent that is off passes its hooks through; one that is on, or
    /// unknown, does not; with no settings every client is on.
    #[test]
    fn an_agent_turned_off_passes_its_hooks() {
        let env = HashMap::new();
        let off = data_with(serde_json::json!({"cursor": false}));
        assert!(turned_off(off.path(), "cursor", None, &env));
        assert!(!turned_off(off.path(), "kimi", None, &env));
        let none = tempfile::tempdir().unwrap();
        assert!(!turned_off(&none.path().join("data"), "cursor", None, &env));
    }

    /// A split agent with every client off passes; with only another client
    /// off, a caller its rule cannot place stays on.
    #[test]
    fn a_split_agent_passes_only_for_its_clients_that_are_off() {
        let env = HashMap::new();
        let all = data_with(serde_json::json!({"claude": false}));
        assert!(turned_off(all.path(), "claude", None, &env));
        let desktop = data_with(serde_json::json!({"claude-desktop": false}));
        assert!(!turned_off(desktop.path(), "claude", None, &env));
    }
}
