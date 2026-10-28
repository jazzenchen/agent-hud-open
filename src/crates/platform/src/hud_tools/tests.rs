use super::*;
use serde_json::Value;
use std::collections::HashMap;

const APP: &str = "/Applications/Agent HUD.app/Contents/MacOS/Agent HUD";

fn places(home: &Path) -> Places {
    Places::new(home, &HashMap::new())
}

#[test]
fn each_client_reads_the_skill_from_its_own_folder() {
    // Absolute on every OS. "/c" is relative on Windows, where Places would
    // ignore it as a CLAUDE_CONFIG_DIR. Nothing here is created.
    let root = std::env::temp_dir().join("agent-hud-places");
    let (home, claude) = (root.join("u"), root.join("c"));
    let env = HashMap::from([
        (
            "CLAUDE_CONFIG_DIR".to_string(),
            claude.to_string_lossy().into_owned(),
        ),
        ("DSH_HOME".to_string(), "~/d".to_string()),
    ]);
    let places = Places::new(&home, &env);
    let folder = |client: Client| client.skills(&places);
    assert_eq!(folder(Client::ClaudeCode), claude.join("skills"));
    assert_eq!(folder(Client::Codex), home.join(".agents/skills"));
    assert_eq!(folder(Client::Cursor), home.join(".cursor/skills"));
    assert_eq!(folder(Client::DeepSeek), home.join("d/skills"));
    assert_eq!(
        folder(Client::Antigravity),
        home.join(".gemini/config/skills")
    );
    let defaults = Places::new(&home, &HashMap::from([("DSH_HOME".into(), " ".into())]));
    assert_eq!(defaults.claude, home.join(".claude"));
    assert_eq!(defaults.deepseek, home.join(".dsh"));
    for client in CLIENTS {
        assert_eq!(Client::from_id(client.id()), Some(client));
    }
}

#[test]
fn the_skill_is_valid_for_every_client() {
    let front = skill::TEXT
        .strip_prefix("---\n")
        .and_then(|rest| rest.split("\n---\n").next())
        .unwrap();
    let value: Value = serde_saphyr::from_str(front).unwrap();
    // Cursor and the Agent Skills spec: the name is the folder's.
    assert_eq!(value["name"], SKILL_FOLDER);
    let description = value["description"].as_str().unwrap();
    assert!(!description.is_empty() && description.chars().count() <= 1024);
    assert_eq!(value["metadata"]["managed-by"], "agent-hud");
    // DeepSeek Harness: kebab-case names only.
    assert!(SKILL_FOLDER
        .split('-')
        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_lowercase())));
}

#[test]
fn the_skill_is_added_refreshed_and_removed_but_never_someone_elses() {
    let home = tempfile::tempdir().unwrap();
    let places = places(home.path());
    let client = Client::Cursor;
    let file = client.skills(&places).join(SKILL_FOLDER).join("SKILL.md");
    assert_eq!(client.skill_state(&places), SkillState::Missing);
    assert!(client.add_skill(&places).unwrap());
    assert_eq!(fs::read_to_string(&file).unwrap(), skill::TEXT);
    assert_eq!(client.skill_state(&places), SkillState::Current);
    assert!(!client.add_skill(&places).unwrap());

    // An older copy of ours is brought up to date.
    fs::write(
        &file,
        "---\nname: customize-agent-hud\nmetadata:\n  managed-by: agent-hud\n---\nold\n",
    )
    .unwrap();
    assert_eq!(client.skill_state(&places), SkillState::Older);
    assert!(client.add_skill(&places).unwrap());
    assert_eq!(client.skill_state(&places), SkillState::Current);

    client.remove_skill(&places).unwrap();
    assert_eq!(client.skill_state(&places), SkillState::Missing);
    assert!(!file.parent().unwrap().exists());

    // Someone else's skill of the same name stays as it is.
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "---\nname: customize-agent-hud\n---\nmine\n").unwrap();
    assert_eq!(client.skill_state(&places), SkillState::Foreign);
    assert!(client.add_skill(&places).is_err());
    client.remove_skill(&places).unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "---\nname: customize-agent-hud\n---\nmine\n"
    );
}

#[test]
fn a_folder_our_removal_left_behind_is_filled_again() {
    let home = tempfile::tempdir().unwrap();
    let places = places(home.path());
    let client = Client::Codex;
    client.add_skill(&places).unwrap();
    let folder = client.skills(&places).join(SKILL_FOLDER);
    fs::write(folder.join(".DS_Store"), "").unwrap();
    client.remove_skill(&places).unwrap();
    assert!(folder.is_dir());
    assert_eq!(client.skill_state(&places), SkillState::Missing);
    assert!(client.add_skill(&places).unwrap());
    assert_eq!(client.skill_state(&places), SkillState::Current);
    // A file where the folder would go is someone else's.
    let other = Client::Cursor;
    fs::create_dir_all(other.skills(&places)).unwrap();
    fs::write(other.skills(&places).join(SKILL_FOLDER), "mine").unwrap();
    assert_eq!(other.skill_state(&places), SkillState::Foreign);
    assert!(other.add_skill(&places).is_err());
}

#[test]
fn a_file_added_beside_our_skill_keeps_its_folder() {
    let home = tempfile::tempdir().unwrap();
    let places = places(home.path());
    let client = Client::DeepSeek;
    client.add_skill(&places).unwrap();
    let folder = client.skills(&places).join(SKILL_FOLDER);
    fs::write(folder.join("notes.md"), "kept").unwrap();
    client.remove_skill(&places).unwrap();
    assert!(!folder.join("SKILL.md").exists());
    assert_eq!(fs::read_to_string(folder.join("notes.md")).unwrap(), "kept");
}

#[test]
fn cursor_gets_its_server_beside_the_others_and_loses_only_it() {
    let original = "{\n  \"mcpServers\": {\n    \"Playwright\": { \"command\": \"npx\", \"args\": [\"@playwright/mcp\"] }\n  }\n}\n";
    let added = mcp_json::edit("Cursor", Some(original), Some(Path::new(APP)))
        .unwrap()
        .unwrap();
    let value: Value = serde_json::from_str(&added).unwrap();
    assert_eq!(value["mcpServers"]["agent-hud"]["command"], APP);
    assert_eq!(
        value["mcpServers"]["agent-hud"]["args"],
        serde_json::json!(["--mcp"])
    );
    assert_eq!(value["mcpServers"]["Playwright"]["command"], "npx");
    assert!(mcp_json::listed(Some(&added)));
    // Unchanged when it is already there.
    assert_eq!(
        mcp_json::edit("Cursor", Some(&added), Some(Path::new(APP)))
            .unwrap()
            .as_deref(),
        Some(added.as_str())
    );
    let removed = mcp_json::edit("Cursor", Some(&added), None)
        .unwrap()
        .unwrap();
    assert_eq!(removed, original);
    assert!(!mcp_json::listed(Some(&removed)));
}

#[test]
fn cursor_keeps_what_the_person_set_on_our_entry_and_moves_the_command() {
    let original = r#"{"mcpServers":{"agent-hud":{"command":"/old/Agent HUD","args":["--mcp"],"env":{"A":"1"}}}}"#;
    let moved = mcp_json::edit("Cursor", Some(original), Some(Path::new(APP)))
        .unwrap()
        .unwrap();
    let value: Value = serde_json::from_str(&moved).unwrap();
    assert_eq!(value["mcpServers"]["agent-hud"]["command"], APP);
    assert_eq!(value["mcpServers"]["agent-hud"]["env"]["A"], "1");
}

#[test]
fn cursor_without_a_file_gets_one_and_a_file_with_comments_is_left_alone() {
    let created = mcp_json::edit("Cursor", None, Some(Path::new(APP)))
        .unwrap()
        .unwrap();
    assert!(mcp_json::listed(Some(&created)));
    assert_eq!(mcp_json::edit("Cursor", None, None).unwrap(), None);
    let commented = "{\n  // mine\n  \"mcpServers\": {}\n}\n";
    assert!(mcp_json::edit("Cursor", Some(commented), Some(Path::new(APP))).is_err());
    assert!(mcp_json::edit("Cursor", Some("[]"), Some(Path::new(APP))).is_err());
}

#[test]
fn deepseek_without_a_file_gets_one_that_holds_only_our_block() {
    let created = deepseek::edit(None, Some(Path::new(APP))).unwrap().unwrap();
    assert!(created.starts_with("# >>> agent-hud"));
    assert!(deepseek::listed(Some(&created)));
    let items: Value = serde_saphyr::from_str(&created).unwrap();
    let plugin = &items[0]["insert"][0];
    assert_eq!(plugin["id"], "agent-hud");
    assert_eq!(plugin["name"], "@deepseek-ai/dsh-mcp-client");
    assert_eq!(plugin["config"]["serverName"], "agent-hud");
    assert_eq!(plugin["config"]["transport"], "stdio");
    assert_eq!(plugin["config"]["command"], APP);
    assert_eq!(plugin["config"]["args"], serde_json::json!(["--mcp"]));
    // Unchanged when it is already there; removing it removes the file.
    assert_eq!(
        deepseek::edit(Some(&created), Some(Path::new(APP)))
            .unwrap()
            .as_deref(),
        Some(created.as_str())
    );
    assert_eq!(deepseek::edit(Some(&created), None).unwrap(), None);
}

#[test]
fn deepseek_keeps_the_persons_patches_and_comments() {
    let original = "# mine\n- set:\n    foo: 1\n";
    let added = deepseek::edit(Some(original), Some(Path::new(APP)))
        .unwrap()
        .unwrap();
    assert!(added.starts_with(original));
    let items: Value = serde_saphyr::from_str(&added).unwrap();
    assert_eq!(items.as_array().unwrap().len(), 2);
    assert_eq!(
        deepseek::edit(Some(&added), None).unwrap().unwrap(),
        original
    );

    // A new path replaces the block instead of adding a second.
    let moved = deepseek::edit(Some(&added), Some(Path::new("/elsewhere/Agent HUD")))
        .unwrap()
        .unwrap();
    assert_eq!(moved.matches("# >>> agent-hud").count(), 1);
    assert!(moved.contains("\"/elsewhere/Agent HUD\""));
}

#[test]
fn deepseek_never_leaves_a_file_that_stops_harness_from_starting() {
    // An empty flow list is replaced, since block items cannot follow it.
    let added = deepseek::edit(Some("# mine\n[]\n"), Some(Path::new(APP)))
        .unwrap()
        .unwrap();
    assert!(added.starts_with("# mine\n# >>> agent-hud"));
    assert!(!added.contains("[]\n"));
    // Removing it again leaves a list, never a comments-only file.
    assert_eq!(
        deepseek::edit(Some(&added), None).unwrap().unwrap(),
        "# mine\n[]\n"
    );
    let commented = deepseek::edit(Some("# only\n"), Some(Path::new(APP)))
        .unwrap()
        .unwrap();
    assert_eq!(
        deepseek::edit(Some(&commented), None).unwrap().unwrap(),
        "# only\n[]\n"
    );
}

#[test]
fn deepseek_refuses_files_it_cannot_add_to_safely() {
    let app = Some(Path::new(APP));
    assert!(deepseek::edit(Some("[{set: {foo: 1}}]\n"), app).is_err());
    assert!(deepseek::edit(Some("foo: 1\n"), app).is_err());
    assert!(deepseek::edit(
        Some(
            "# >>> agent-hud (added by Agent HUD; remove it in Agent HUD's Settings)\n- set: {}\n"
        ),
        app
    )
    .is_err());
    let by_hand = "- insert:\n    - id: agent-hud\n      name: \"@deepseek-ai/dsh-mcp-client\"\n      config: {}\n";
    assert!(deepseek::listed(Some(by_hand)));
    assert!(deepseek::edit(Some(by_hand), app).is_err());
    assert!(deepseek::edit(Some(by_hand), None).is_err());
    let other_id = "- insert:\n    - id: hud\n      name: \"@deepseek-ai/dsh-mcp-client\"\n      config:\n        serverName: agent-hud\n";
    assert!(deepseek::listed(Some(other_id)));
    assert!(deepseek::edit(Some(other_id), app).is_err());
    // Nothing of ours to remove: the file stays as it is.
    assert_eq!(
        deepseek::edit(Some("- set: {}\n"), None)
            .unwrap()
            .as_deref(),
        Some("- set: {}\n")
    );
}

#[test]
fn a_server_is_set_in_the_settings_file_beside_what_is_there() {
    let home = tempfile::tempdir().unwrap();
    let places = places(home.path());
    let file = home.path().join(".cursor/mcp.json");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "{\"mcpServers\":{\"x\":{\"command\":\"y\"}}}").unwrap();
    assert!(Client::Cursor.present(&places));
    assert!(Client::Cursor
        .set_server(&places, Some(Path::new(APP)))
        .unwrap());
    assert!(Client::Cursor.listed(&places));
    assert!(!Client::Cursor
        .set_server(&places, Some(Path::new(APP)))
        .unwrap());
    assert!(Client::Cursor.set_server(&places, None).unwrap());
    assert!(!Client::Cursor.listed(&places));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "{\"mcpServers\":{\"x\":{\"command\":\"y\"}}}"
    );

    assert!(Client::DeepSeek
        .set_server(&places, Some(Path::new(APP)))
        .unwrap());
    assert!(Client::DeepSeek.listed(&places));
    assert!(Client::DeepSeek.set_server(&places, None).unwrap());
    assert!(!places.deepseek.join("cordis.patch.yml").exists());
    assert!(Client::Codex.set_server(&places, None).is_err());
}

#[test]
fn antigravity_turns_a_server_back_on_and_counts_a_turned_off_one_as_absent() {
    let off = r#"{"mcpServers":{"agent-hud":{"args":["--mcp"],"command":"/old/Agent HUD","disabled":true}}}"#;
    // Turned off in Antigravity, it is still there to remove.
    assert!(mcp_json::listed(Some(off)));
    let on = mcp_json::edit("Antigravity", Some(off), Some(Path::new(APP)))
        .unwrap()
        .unwrap();
    assert!(mcp_json::listed(Some(&on)));
    let value: Value = serde_json::from_str(&on).unwrap();
    assert_eq!(value["mcpServers"]["agent-hud"]["command"], APP);
    assert!(value["mcpServers"]["agent-hud"].get("disabled").is_none());

    let home = tempfile::tempdir().unwrap();
    let places = places(home.path());
    fs::create_dir_all(home.path().join(".gemini/config")).unwrap();
    fs::write(home.path().join(".gemini/config/mcp_config.json"), "").unwrap();
    assert!(Client::Antigravity.present(&places));
    assert!(Client::Antigravity
        .set_server(&places, Some(Path::new(APP)))
        .unwrap());
    assert!(Client::Antigravity.listed(&places));
    assert!(Client::Antigravity.set_server(&places, None).unwrap());
    let left: Value = serde_json::from_str(
        &fs::read_to_string(home.path().join(".gemini/config/mcp_config.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(left, serde_json::json!({"mcpServers": {}}));
}
