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

/// Whether a permission request from this hook's client stays with the
/// client: answering in the HUD is off for every client, or for this one
/// (Settings → Agents). A caller its agent's rule cannot place stays with the
/// HUD unless every client of its agent is off.
pub(super) fn answers_off(
    data: &Path,
    source: &str,
    home: Option<&Path>,
    env: &HashMap<String, String>,
) -> bool {
    let Some(agent) = super::origins::agent_id(source) else {
        return false;
    };
    let settings = &stored(data)["settings"];
    if settings["answerInHud"] == false {
        return true;
    }
    let off: Vec<&str> = settings["answerOff"]
        .as_array()
        .map(|ids| ids.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let ids = split::ids(agent);
    if !ids.iter().any(|id| off.contains(id)) {
        return false;
    }
    if ids.iter().all(|id| off.contains(id)) {
        return true;
    }
    super::origins::caller_client(source, home, env).is_some_and(|id| off.contains(&id))
}

/// Whether approving automatically is on (Settings → Agents): the app then
/// needs to know which client a permission request comes from.
pub(super) fn auto_approve_on(data: &Path) -> bool {
    stored(data)["settings"]["autoApprove"] == true
}

/// What Settings saved; null when the file is missing or unreadable, where
/// every client is on and answers in the HUD.
fn stored(data: &Path) -> Value {
    std::fs::read(data.join("settings.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .unwrap_or(Value::Null)
}

/// The switches Settings saved, by client id.
fn saved_switches(data: &Path) -> HashMap<String, bool> {
    serde_json::from_value(stored(data)["providers"].clone()).unwrap_or_default()
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

    fn data_with_settings(settings: Value) -> tempfile::TempDir {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(
            data.path().join("settings.json"),
            serde_json::to_vec(&serde_json::json!({"settings":settings,"providers":{}})).unwrap(),
        )
        .unwrap();
        data
    }

    /// Answering in the HUD turned off keeps every request with its client;
    /// turned off for every client of an agent, that agent's.
    #[test]
    fn answering_off_keeps_requests_with_the_client() {
        let env = HashMap::new();
        let all = data_with_settings(serde_json::json!({"answerInHud": false}));
        assert!(answers_off(all.path(), "cursor", None, &env));
        let codex = data_with_settings(
            serde_json::json!({"answerInHud": true, "answerOff": ["codex", "codex-desktop"]}),
        );
        assert!(answers_off(codex.path(), "codex", None, &env));
        assert!(!answers_off(codex.path(), "claude", None, &env));
        let desktop = data_with_settings(serde_json::json!({"answerOff": ["codex-desktop"]}));
        assert!(!answers_off(desktop.path(), "codex", None, &env));
        let none = tempfile::tempdir().unwrap();
        assert!(!answers_off(&none.path().join("data"), "codex", None, &env));
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
