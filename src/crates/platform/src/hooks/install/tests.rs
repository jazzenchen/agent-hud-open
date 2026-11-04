use super::*;
use std::{fs, path::Path};

fn context(home: &Path, env: &[(&str, &str)]) -> InstallContext {
    InstallContext {
        home: home.into(),
        environment: env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        executable: PathBuf::new(),
        canonical_executable: None,
    }
}

#[test]
fn droid_hooks_copy_the_settings_hooks_they_would_shadow() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let factory = home.join(".factory");
    fs::create_dir_all(&factory).unwrap(); // plain directory
    fs::write(
        factory.join("settings.json"),
        r#"{
  // Droid's own settings, with a comment
  "model": "auto",
  "hooks": {"PreToolUse": [{"matcher": "Execute", "hooks": [{"type": "command", "command": "/opt/audit.sh"}]}]}
}"#,
    )
    .unwrap();
    let context = context(&home, &[]);
    let current = json!({});
    let seeded = with_shadowed(&context, "droid", "--permission-hook", &current).unwrap();
    assert_eq!(
        seeded["PreToolUse"][0]["hooks"][0]["command"],
        "/opt/audit.sh"
    );
    assert_eq!(
        with_shadowed(&context, "droid", "--completion-hook", &current).unwrap(),
        json!({})
    );
    assert_eq!(
        with_shadowed(&context, "claude", "--permission-hook", &current).unwrap(),
        current
    );
    assert_eq!(
        configurations(&context, "droid", ".factory/hooks.json"),
        [factory.join("hooks.json")]
    );
}

#[test]
fn settings_follow_each_clients_folder_variable() {
    let home = Path::new("/h");
    let path = |source: &str, env: &[(&str, &str)]| {
        let relative = agent_hud_agents::configuration(source, "--permission-hook")
            .or_else(|_| agent_hud_agents::configuration(source, "--completion-hook"))
            .unwrap()
            .relative_path;
        configurations(&context(home, env), source, &relative).remove(0)
    };
    assert_eq!(path("claude", &[]), home.join(".claude/settings.json"));
    assert_eq!(
        path("claude", &[("CLAUDE_CONFIG_DIR", "/c")]),
        Path::new("/c/settings.json")
    );
    assert_eq!(
        path("codex", &[("CODEX_HOME", " /x ")]),
        Path::new("/x/hooks.json")
    );
    assert_eq!(
        path("antigravity", &[("GEMINI_CLI_HOME", "/g")]),
        Path::new("/g/config/hooks.json")
    );
    assert_eq!(
        path("codebuddy", &[("CODEBUDDY_CONFIG_DIR", "/h/.workbuddy")]),
        home.join(".codebuddy/settings.json"),
        "WorkBuddy's folder is not CodeBuddy's"
    );
    assert_eq!(
        path("workbuddy", &[]),
        home.join(".workbuddy/settings.json")
    );
    assert_eq!(
        path("qoder-work", &[("QWEN_HOME", "/q")]),
        home.join(".qoderwork/settings.json")
    );
}
