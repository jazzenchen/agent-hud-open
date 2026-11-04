use crate::{patch_text, text_settings};

const EXE: &str = "/Applications/Agent HUD.app/Contents/MacOS/agent-hud";
const OLD: &str = "/Volumes/Agent HUD/Agent HUD.app/Contents/MacOS/agent-hud";
const EVENTS: [&str; 5] = [
    "on_session_end",
    "pre_llm_call",
    "pre_tool_call",
    "pre_approval_request",
    "post_approval_response",
];

fn command(exe: &str) -> String {
    format!("'{exe}' --completion-hook hermes")
}

fn install(text: &str) -> Result<String, String> {
    patch_text("hermes", "--completion-hook", text, Some(&command(EXE)))
}

fn remove(text: &str) -> Result<String, String> {
    patch_text("hermes", "--completion-hook", text, None)
}

fn yaml(text: &str) -> serde_json::Value {
    serde_saphyr::from_str(text).unwrap()
}

/// A Hermes settings file with comments, a hook of the user's own and
/// keys on either side of `hooks:`.
const CONFIG: &str = include_str!("../../tests/fixtures/hermes/macos/config.yaml");

#[test]
fn hermes_settings_are_text() {
    assert!(text_settings("hermes"));
    assert!(!text_settings("claude"));
    let refused =
        crate::patch_configuration("hermes", "--completion-hook", &serde_json::json!({}), None);
    assert!(refused.is_err());
}

#[test]
fn handlers_go_in_beside_the_users_own_and_every_other_line_stays() {
    let installed = install(CONFIG).unwrap();
    let value = yaml(&installed);
    let hooks = &value["hooks"];
    for event in EVENTS {
        let ours = hooks[event]
            .as_array()
            .unwrap()
            .iter()
            .filter(|h| h["command"] == command(EXE))
            .count();
        assert_eq!(ours, 1, "{event}");
        assert_eq!(
            hooks[event].as_array().unwrap().last().unwrap()["timeout"],
            5
        );
    }
    // The user's own hook is first, untouched.
    assert_eq!(
        hooks["pre_tool_call"][0]["command"],
        "~/.hermes/agent-hooks/block-rm-rf.sh"
    );
    assert_eq!(hooks["pre_tool_call"][0]["matcher"], "terminal");
    // Every line of the original is still there, in order.
    let mut kept = installed.lines();
    for line in CONFIG.lines() {
        assert!(kept.any(|l| l == line), "lost {line:?}");
    }
    let original = yaml(CONFIG);
    for key in ["model", "toolsets", "hooks_auto_accept", "display"] {
        assert_eq!(value[key], original[key], "{key}");
    }
    // Installing again changes nothing.
    assert_eq!(install(&installed).unwrap(), installed);
    // Taking them out gives back the file as it was.
    assert_eq!(remove(&installed).unwrap(), CONFIG);
}

#[test]
fn a_file_without_hooks_gets_a_mapping_of_its_own() {
    let original = "model:\n  default: hermes-4-405b\n# the end\n";
    let installed = install(original).unwrap();
    assert!(installed.starts_with(original));
    assert!(installed.contains(&format!(
        "hooks:\n  on_session_end:\n    - command: \"{}\"\n      timeout: 5\n",
        command(EXE)
    )));
    assert_eq!(remove(&installed).unwrap(), original);
    // An empty or missing file.
    let created = install("").unwrap();
    assert_eq!(yaml(&created)["hooks"]["pre_llm_call"][0]["timeout"], 5);
    assert_eq!(remove(&created).unwrap(), "");
    // No final newline.
    let bare = install("model: x").unwrap();
    assert!(bare.starts_with("model: x\nhooks:\n"));
}

#[test]
fn an_empty_hooks_mapping_and_empty_events_take_handlers() {
    for original in ["hooks: {}\nother: 1\n", "hooks:\nother: 1\n"] {
        let installed = install(original).unwrap();
        assert!(installed.starts_with("hooks:\n  "), "{installed}");
        assert!(installed.ends_with("other: 1\n"));
        assert_eq!(yaml(&installed)["other"], 1);
        assert_eq!(remove(&installed).unwrap(), "other: 1\n");
    }
    let original = "hooks:\n  pre_llm_call: []\n  on_session_end:\n";
    let installed = install(original).unwrap();
    let value = yaml(&installed);
    assert_eq!(value["hooks"]["pre_llm_call"].as_array().unwrap().len(), 1);
    assert_eq!(
        value["hooks"]["on_session_end"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn hermes_own_style_and_an_older_executable_are_recognised() {
    // Hermes saves its settings with PyYAML: other quotes, no indent
    // before a sequence.
    let old = command(OLD).replace('\'', "''");
    let saved = format!(
        "hooks:\n  pre_llm_call:\n  - command: '{old}'\n    timeout: 5\n  - command: other.sh\n"
    );
    let installed = install(&saved).unwrap();
    let value = yaml(&installed);
    let list = value["hooks"]["pre_llm_call"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["command"], command(EXE), "it keeps its place");
    assert_eq!(list[1]["command"], "other.sh");
    assert!(installed.contains("  - command: other.sh\n"));
    let removed = remove(&installed).unwrap();
    assert_eq!(
        yaml(&removed)["hooks"]["pre_llm_call"],
        serde_json::json!([{"command": "other.sh"}])
    );
    assert!(!removed.contains("agent-hud"));
}

#[test]
fn windows_line_ends_stay() {
    let original = "model: x\r\nhooks:\r\n  pre_tool_call:\r\n    - command: mine.sh\r\n";
    let installed = install(original).unwrap();
    assert!(!installed.replace("\r\n", "").contains('\n'));
    assert_eq!(remove(&installed).unwrap(), original);
}

#[test]
fn layouts_it_cannot_follow_are_refused() {
    for original in [
        "hooks: {pre_llm_call: [{command: mine.sh}]}\n",
        "\"hooks\":\n  pre_llm_call: []\n",
        "hooks:\n  \"pre_llm_call\":\n    - command: mine.sh\n",
        "hooks:\n  pre_llm_call: [{command: mine.sh}]\n",
        "hooks: [1, 2]\n",
        "- a list\n",
        "model: [unclosed\n",
    ] {
        assert!(install(original).is_err(), "{original}");
    }
    // Nothing of Agent HUD's to take out leaves any file alone.
    let odd = "hooks: {pre_llm_call: [{command: mine.sh}]}\n";
    assert_eq!(remove(odd).unwrap(), odd);
}

#[test]
fn a_parse_error_never_quotes_the_file() {
    let error = install("api_key: sk-secret\n  bad: [\n").unwrap_err();
    assert!(!error.contains("sk-secret"), "{error}");
}
