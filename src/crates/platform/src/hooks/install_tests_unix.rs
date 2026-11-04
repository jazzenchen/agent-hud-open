use super::*;
use std::os::unix::fs::PermissionsExt;

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
    let path = configuration(&context, source, &description.relative_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap(); // plain directory
    fs::write(&path, SETTINGS).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    let plan = install_plan(&context, source, flag, true).unwrap();
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
    assert!(!install_plan(&context, source, flag, true).unwrap().changed);

    let removal = install_plan(&context, source, flag, false).unwrap();
    assert!(apply_plan(&removal).unwrap());
    assert_eq!(fs::read_to_string(&path).unwrap(), SETTINGS);
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
    let path = configuration(&context, source, &description.relative_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap(); // plain directory
    let odd = "hooks: {pre_llm_call: [{command: mine.sh}]}\n";
    fs::write(&path, odd).unwrap();
    assert!(install_plan(&context, source, flag, true).is_err());
    assert!(!install_plan(&context, source, flag, false).unwrap().changed);
    assert_eq!(fs::read_to_string(&path).unwrap(), odd);
}

fn context_in(home: &Path) -> InstallContext {
    fs::create_dir_all(home).unwrap(); // plain directory
    context(home)
}
