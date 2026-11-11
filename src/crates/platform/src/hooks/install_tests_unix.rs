use super::*;
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

fn context(home: &Path) -> InstallContext {
    let executable = home.join("fixed owner");
    fs::write(&executable, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    InstallContext {
        home: home.to_owned(),
        environment: HashMap::new(),
        canonical_executable: Some(executable.clone()),
        executable,
    }
}

/// The client whose hook settings are text (Hermes's YAML today).
fn text_client() -> &'static str {
    agent_hud_agents::ALL
        .iter()
        .map(|agent| agent.id)
        .find(|id| agent_hud_agents::text_settings(id))
        .expect("a client with text settings")
}

const SETTINGS: &str =
    "# mine\nmodel:\n  default: x\nhooks:\n  pre_tool_call:\n    - command: mine.sh\n";

#[test]
fn text_settings_take_the_hooks_in_place_and_give_them_back() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let context = context_in(&home);
    let source = text_client();
    let flag = agent_hud_agents::hooks::hooks_by_id(source)
        .unwrap()
        .flags()[0];
    let description = agent_hud_agents::configuration(source, flag).unwrap();
    let path = configurations(&context, source, &description.relative_path).remove(0);
    fs::create_dir_all(path.parent().unwrap()).unwrap(); // plain directory
    fs::write(&path, SETTINGS).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    let plan = only(install_plans(&context, source, flag, true)).unwrap();
    assert!(plan.changed);
    assert!(apply_plan(&plan).unwrap());
    let installed = fs::read_to_string(&path).unwrap();
    assert!(installed.starts_with(
        "# mine\nmodel:\n  default: x\nhooks:\n  pre_tool_call:\n    - command: mine.sh\n"
    ));
    assert!(installed.contains(&format!(
        "'{}' {flag} {source}",
        context.executable.display()
    )));
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600,
        "the file keeps its permissions"
    );
    // Installed: nothing more to change.
    assert!(
        !only(install_plans(&context, source, flag, true))
            .unwrap()
            .changed
    );

    let removal = only(install_plans(&context, source, flag, false)).unwrap();
    assert!(apply_plan(&removal).unwrap());
    assert_eq!(fs::read_to_string(&path).unwrap(), SETTINGS);
}

#[test]
fn kimi_takes_its_hooks_in_config_toml_and_gives_them_back() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let context = context_in(&home);
    let path = home.join(".kimi-code/config.toml");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = "default_model = \"kimi-code/kimi-for-coding\"\n\n[[hooks]]\nevent = \"Stop\"\ncommand = \"say done\"\n";
    fs::write(&path, original).unwrap();

    let plan = only(install_plans(&context, "kimi", "--completion-hook", true)).unwrap();
    assert!(apply_plan(&plan).unwrap());
    let installed = fs::read_to_string(&path).unwrap();
    assert!(installed.starts_with(original));
    let command = format!("'{}' --completion-hook kimi", context.executable.display());
    for event in ["Stop", "UserPromptSubmit"] {
        assert!(installed.contains(&format!(
            "[[hooks]]\nevent = \"{event}\"\ncommand = \"{command}\"\ntimeout = 10\n"
        )));
    }
    assert!(
        !only(install_plans(&context, "kimi", "--completion-hook", true))
            .unwrap()
            .changed
    );
    let removal = only(install_plans(&context, "kimi", "--completion-hook", false)).unwrap();
    assert!(apply_plan(&removal).unwrap());
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn qoder_takes_its_rewake_hook_beside_its_own_and_gives_it_back() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let context = context_in(&home);
    let path = home.join(".qoder/settings.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = r#"{
  "model": "auto",
  "hooks": {
    "Stop": [
      { "hooks": [ { "type": "command", "command": "say done" } ] }
    ]
  }
}
"#;
    fs::write(&path, original).unwrap();

    let plan = only(install_plans(&context, "qoder", "--rewake-hook", true)).unwrap();
    assert!(apply_plan(&plan).unwrap());
    let installed: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let command = format!("'{}' --rewake-hook qoder", context.executable.display());
    let handler = json!({"type":"command","command":command,"timeout":86400,"asyncRewake":true});
    assert_eq!(
        installed["hooks"]["SessionStart"],
        json!([{"hooks":[handler]}])
    );
    // The user's own Stop hook stays first, Agent HUD's waits beside it.
    assert_eq!(
        installed["hooks"]["Stop"],
        json!([{"hooks":[{"type":"command","command":"say done"}]},{"hooks":[handler]}])
    );
    assert_eq!(installed["model"], "auto");
    assert!(
        !only(install_plans(&context, "qoder", "--rewake-hook", true))
            .unwrap()
            .changed
    );
    // The completion hook's Stop handler joins without touching it.
    let completion = only(install_plans(&context, "qoder", "--completion-hook", true)).unwrap();
    assert!(apply_plan(&completion).unwrap());
    assert!(
        !only(install_plans(&context, "qoder", "--rewake-hook", true))
            .unwrap()
            .changed
    );
    for flag in ["--completion-hook", "--rewake-hook"] {
        let removal = only(install_plans(&context, "qoder", flag, false)).unwrap();
        assert!(apply_plan(&removal).unwrap());
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn text_settings_it_cannot_follow_are_left_alone() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let context = context_in(&home);
    let source = text_client();
    let flag = agent_hud_agents::hooks::hooks_by_id(source)
        .unwrap()
        .flags()[0];
    let description = agent_hud_agents::configuration(source, flag).unwrap();
    let path = configurations(&context, source, &description.relative_path).remove(0);
    fs::create_dir_all(path.parent().unwrap()).unwrap(); // plain directory
    let odd = "hooks: {pre_llm_call: [{command: mine.sh}]}\n";
    fs::write(&path, odd).unwrap();
    assert!(only(install_plans(&context, source, flag, true)).is_err());
    assert!(
        !only(install_plans(&context, source, flag, false))
            .unwrap()
            .changed
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), odd);
}

#[test]
fn text_settings_go_in_every_profile_that_has_them_and_come_out_of_each() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let context = context_in(&home);
    let source = text_client();
    let flag = agent_hud_agents::hooks::hooks_by_id(source)
        .unwrap()
        .flags()[0];
    let description = agent_hud_agents::configuration(source, flag).unwrap();
    let own = configurations(&context, source, &description.relative_path).remove(0);
    let folder = own.parent().unwrap().to_owned();
    let profile = |name: &str| folder.join("profiles").join(name).join("config.yaml");
    let odd = "hooks: {pre_llm_call: [{command: mine.sh}]}\n";
    for (file, text) in [
        (own.clone(), SETTINGS),
        (profile("coder"), SETTINGS),
        (profile("odd"), odd),
    ] {
        fs::create_dir_all(file.parent().unwrap()).unwrap(); // plain directory
        fs::write(&file, text).unwrap();
    }
    // A profile not set up yet has no settings to change.
    fs::create_dir_all(folder.join("profiles/fresh")).unwrap(); // plain directory

    let plans = install_plans(&context, source, flag, true);
    assert_eq!(plans.len(), 3, "the client's own file and each profile's");
    let failed = plans.iter().filter_map(|plan| plan.as_ref().err());
    assert_eq!(
        failed
            .map(|error| error.ends_with(&format!("({})", profile("odd").display())))
            .collect::<Vec<_>>(),
        [true],
        "the file that cannot be changed names itself"
    );
    for plan in plans.into_iter().flatten() {
        assert!(apply_plan(&plan).unwrap());
    }
    for file in [&own, &profile("coder")] {
        let installed = fs::read_to_string(file).unwrap();
        assert!(installed.starts_with(SETTINGS), "{}", file.display());
        assert!(installed.contains(&format!("{flag} {source}")));
    }
    assert_eq!(fs::read_to_string(profile("odd")).unwrap(), odd);
    assert!(!profile("fresh").exists(), "nothing is made in it");

    // Taking them out reads the odd profile too, and finds nothing of ours.
    for plan in install_plans(&context, source, flag, false) {
        apply_plan(&plan.unwrap()).unwrap();
    }
    for file in [&own, &profile("coder")] {
        assert_eq!(
            fs::read_to_string(file).unwrap(),
            SETTINGS,
            "{}",
            file.display()
        );
    }
}

/// The one plan of a client with one settings file.
fn only(mut plans: Vec<Result<InstallPlan, String>>) -> Result<InstallPlan, String> {
    assert_eq!(plans.len(), 1);
    plans.remove(0)
}

fn context_in(home: &Path) -> InstallContext {
    fs::create_dir_all(home).unwrap(); // plain directory
    context(home)
}

#[test]
fn a_cursor_file_with_only_our_stop_takes_the_prompt_events_too() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let context = context_in(&home);
    let command = format!(
        "'{}' --completion-hook cursor",
        context.executable.display()
    );
    let path = home.join(".cursor/hooks.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mine = serde_json::json!({"command":"notify.sh","timeout":3});
    fs::write(
        &path,
        serde_json::json!({"hooks":{"stop":[mine.clone(),{"command":command,"timeout":5}]},"version":1}).to_string(),
    )
    .unwrap();
    let plan = only(install_plans(&context, "cursor", "--completion-hook", true)).unwrap();
    assert!(plan.changed);
    assert!(apply_plan(&plan).unwrap());
    let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    for event in [
        "stop",
        "beforeSubmitPrompt",
        "postToolUse",
        "afterAgentResponse",
    ] {
        let ours = value["hooks"][event]
            .as_array()
            .unwrap()
            .iter()
            .filter(|h| h["command"] == command.as_str())
            .count();
        assert_eq!(ours, 1, "{event}");
    }
    assert_eq!(value["hooks"]["stop"][0], mine);
    let removal = only(install_plans(
        &context,
        "cursor",
        "--completion-hook",
        false,
    ))
    .unwrap();
    assert!(apply_plan(&removal).unwrap());
    let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(value["hooks"], serde_json::json!({"stop":[mine]}));
}
