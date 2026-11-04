use super::*;

#[test]
fn flags_follow_each_clients_hooks() {
    let flags = |id: &str| hooks_by_id(id).unwrap().flags();
    assert_eq!(
        flags("claude"),
        [
            "--permission-hook",
            "--attention-hook",
            "--compaction-hook",
            "--session-origin-hook"
        ]
    );
    assert_eq!(
        flags("codex"),
        ["--permission-hook", "--session-origin-hook"]
    );
    assert_eq!(
        flags("antigravity"),
        ["--completion-hook", "--session-origin-hook"]
    );
    assert_eq!(
        flags("codebuddy"),
        ["--permission-hook", "--completion-hook"]
    );
    assert_eq!(flags("workbuddy"), ["--permission-hook"]);
    // Copilot's approvals wait for the user to turn them on; it is the
    // only client whose hook does.
    let copilot = hooks_by_id("copilot").unwrap();
    assert_eq!(flags("copilot"), ["--completion-hook"]);
    assert_eq!(
        copilot.all_flags(),
        [("--permission-hook", true), ("--completion-hook", false)]
    );
    let opt_in: Vec<_> = ALL
        .iter()
        .filter(|agent| agent.hooks.is_some_and(Hooks::opt_in))
        .map(|agent| agent.id)
        .collect();
    assert_eq!(opt_in, ["copilot"]);
    // Goose's hooks wait for a check against a running Goose.
    let goose = hooks_by_id("goose").unwrap();
    for os in [crate::Os::MacOs, crate::Os::Windows, crate::Os::Linux] {
        assert!(goose.launch.on(os).value().is_none(), "{os:?}");
    }
    for agent in ALL {
        for (flag, _) in agent.hooks.map(Hooks::all_flags).unwrap_or_default() {
            assert!(crate::HOOK_FLAGS.contains(&flag), "{} {flag}", agent.id);
        }
    }
}

#[test]
fn a_hook_adds_only_the_variables_its_client_names() {
    let env = |name: &str| {
        ["HERMES_HOME", "CLAUDE_CONFIG_DIR", "SECRET"]
            .contains(&name)
            .then(|| format!("{name} value"))
    };
    let mut hermes = serde_json::json!({"session_id": "s"});
    add_environment("hermes", &mut hermes, &env);
    assert_eq!(
        hermes[ENVIRONMENT],
        serde_json::json!({"HERMES_HOME": "HERMES_HOME value"})
    );
    // A client that names none, or none that is set, is left as it was.
    let payload = serde_json::json!({"session_id": "s"});
    for (source, env) in [
        ("claude", &env as &dyn Fn(&str) -> Option<String>),
        ("hermes", &|_: &str| None),
    ] {
        let mut left = payload.clone();
        add_environment(source, &mut left, env);
        assert_eq!(left, payload, "{source}");
    }
}

#[test]
fn completion_replies_are_json() {
    for agent in ALL {
        if let Some(reply) = agent
            .hooks
            .and_then(|hooks| hooks.completion.as_ref())
            .and_then(|completion| completion.reply)
        {
            assert!(serde_json::from_str::<Value>(reply).is_ok(), "{}", agent.id);
        }
    }
    let antigravity = hooks_by_id("antigravity").unwrap().completion.as_ref();
    assert_eq!(antigravity.unwrap().reply, Some(r#"{"decision":"stop"}"#));
}

#[test]
fn claude_style_transcripts_settle_requests() {
    let settles = |name: &str| {
        hooks_of(name)
            .unwrap()
            .1
            .permission
            .as_ref()
            .is_some_and(|permission| permission.settles_in_transcript)
    };
    for name in [
        "claude",
        "qoder",
        "qoder-cn",
        "qoder-work",
        "qoderWork",
        "qoderCN",
    ] {
        assert!(settles(name), "{name}");
    }
    for name in ["codex", "codebuddy", "zcode", "qwen", "devin", "droid"] {
        assert!(!settles(name), "{name}");
    }
}
