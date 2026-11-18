//! A command line and a desktop app of one agent are switched apart
//! (`agents::split`): turning one off hides only its sessions, and the
//! agent's data is read while either is on.
use super::*;

fn client<'a>(snapshot: &'a Value, id: &str) -> &'a Value {
    snapshot["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id)
        .unwrap()
}

fn session_ids(snapshot: &Value) -> Vec<String> {
    let mut ids = snapshot["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

#[test]
fn a_desktop_app_is_switched_apart_from_its_command_line() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".claude/projects/test");
    fs::create_dir_all(&root).unwrap();
    for (id, entrypoint) in [("cli-1", "cli"), ("desk-1", "claude-desktop")] {
        let row = json!({"sessionId":id,"type":"assistant","entrypoint":entrypoint,"timestamp":Utc::now().to_rfc3339(),"message":{"id":format!("m-{id}"),"usage":{"input_tokens":10,"output_tokens":3}}});
        fs::write(root.join(format!("{id}.jsonl")), format!("{row}\n")).unwrap();
    }
    let mut backend = fixture(temp.path());
    let snapshot = backend.request("snapshot", json!({})).unwrap();
    assert_eq!(session_ids(&snapshot), ["cli-1", "desk-1"]);
    let desktop = snapshot["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "desk-1")
        .unwrap();
    assert_eq!(desktop["clientId"], "claude-desktop");
    assert_eq!(client(&snapshot, "claude-desktop")["detected"], true);
    assert_eq!(client(&snapshot, "claude-desktop")["group"], "anthropic");

    let snapshot = backend
        .request(
            "set_provider",
            json!({"id":"claude-desktop","enabled":false}),
        )
        .unwrap();
    assert_eq!(session_ids(&snapshot), ["cli-1"]);
    assert_eq!(client(&snapshot, "claude-desktop")["enabled"], false);
    assert_eq!(client(&snapshot, "claude")["enabled"], true);
    let provider = snapshot["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "claude")
        .unwrap();
    assert_eq!(provider["enabled"], true);

    // Turning the command line off keeps the desktop app where it is: off.
    let snapshot = backend
        .request("set_provider", json!({"id":"claude","enabled":false}))
        .unwrap();
    assert!(session_ids(&snapshot).is_empty());
    let snapshot = backend
        .request(
            "set_provider",
            json!({"id":"claude-desktop","enabled":true}),
        )
        .unwrap();
    assert_eq!(session_ids(&snapshot), ["desk-1"]);

    // The switches outlive the app.
    let mut reopened = fixture(temp.path());
    let snapshot = reopened.request("snapshot", json!({})).unwrap();
    assert_eq!(session_ids(&snapshot), ["desk-1"]);
    assert!(reopened
        .request(
            "set_provider",
            json!({"id":"claude-mobile","enabled":false})
        )
        .is_err());
}

/// A switch saved before the split, for the agent, holds for every client
/// split off it until that client is set itself.
#[test]
fn a_client_split_off_starts_from_its_agents_switch() {
    let temp = tempfile::tempdir().unwrap();
    let app = temp.path().join("app");
    fs::create_dir_all(&app).unwrap();
    fs::write(
        app.join("settings.json"),
        json!({"settings":{},"providers":{"codex":false}}).to_string(),
    )
    .unwrap();
    let mut backend = fixture(temp.path());
    let snapshot = backend.request("snapshot", json!({})).unwrap();
    assert_eq!(client(&snapshot, "codex-desktop")["enabled"], false);
    let snapshot = backend
        .request("set_provider", json!({"id":"codex","enabled":true}))
        .unwrap();
    assert_eq!(client(&snapshot, "codex")["enabled"], true);
    assert_eq!(client(&snapshot, "codex-desktop")["enabled"], false);
}
