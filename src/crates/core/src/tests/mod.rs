//! Core tests that drive `AgentHudCore` through its logs, settings and requests,
//! grouped by what they cover.
use super::*;

mod claude_desktop;
mod conversation_feed;
mod ingestion;
mod quota_history;
mod sessions;
mod workbuddy;

#[test]
fn launch_at_login_reads_saved_and_imported_choices_before_any_snapshot() {
    let home = tempfile::tempdir().unwrap();
    let mut core = fixture(home.path());
    assert!(core.launch_at_login_enabled());
    core.seed_initial_preferences(&json!({"launchAtLogin": false}))
        .unwrap();
    assert!(
        !core.launch_at_login_enabled(),
        "an imported choice is immediately available"
    );
    drop(core);

    let mut core = fixture(home.path());
    assert!(
        !core.launch_at_login_enabled(),
        "startup keeps the saved false choice"
    );
    core.apply_setting("launchAtLogin", &json!(true)).unwrap();
    drop(core);
    assert!(fixture(home.path()).launch_at_login_enabled());
}

fn fixture(home: &Path) -> AgentHudCore {
    let mut backend = AgentHudCore::new(vec![], home.join("app"));
    backend.provider_roots = vec![
        (home.join(".claude/projects"), "claude"),
        (home.join(".codex/sessions"), "codex"),
    ];
    backend
}
