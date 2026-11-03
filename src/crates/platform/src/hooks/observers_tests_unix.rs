use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};

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

#[test]
fn a_config_folder_linked_to_nowhere_leaves_the_client_out_and_writes_nothing() {
    // Open #16: ~/.config/opencode a symbolic link to a folder that is gone.
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(home.join(".config")).unwrap();
    let gone = temp.path().join("moved dotfiles/opencode");
    symlink(&gone, home.join(".config/opencode")).unwrap();
    let context = context(&home);

    let error = observer_plan(&context, "opencode", true)
        .err()
        .expect("no plan through a dangling link");
    assert!(error.contains("symbolic link"), "{error}");
    assert!(
        !gone.exists() && !temp.path().join("moved dotfiles").exists(),
        "nothing is made at the link's target"
    );
    // Removing has nothing to remove, so it is no failure.
    assert!(!observer_plan(&context, "opencode", false).unwrap().changed);
}

#[test]
fn a_config_folder_linked_to_a_real_folder_still_takes_the_observer() {
    // A dotfiles checkout linked in as ~/.config/opencode.
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(home.join(".config")).unwrap();
    let real = temp.path().join("dotfiles/opencode");
    fs::create_dir_all(&real).unwrap();
    symlink(&real, home.join(".config/opencode")).unwrap();

    let plan = observer_plan(&context(&home), "opencode", true).unwrap();
    assert!(apply_observer_plan(&plan).unwrap());
    assert!(fs::read_to_string(real.join("plugin/agent-hud.js"))
        .unwrap()
        .starts_with("// Agent HUD OpenCode session observer"));
}

#[test]
fn one_client_that_cannot_take_its_observer_is_no_failure_of_the_switch() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(home.join(".config")).unwrap();
    symlink(temp.path().join("gone"), home.join(".config/opencode")).unwrap();
    // Both clients are on PATH, so both are found.
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    for command in ["opencode", "pi"] {
        fs::write(bin.join(command), "#!/bin/sh\n").unwrap();
        fs::set_permissions(bin.join(command), fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut context = context(&home);
    context
        .environment
        .insert("PATH".into(), bin.to_string_lossy().into_owned());
    let mut clients = super::super::clients::Clients::default();
    clients.delegate(context).unwrap();

    // At launch this error stopped the whole app (Open #16).
    clients
        .install(true)
        .expect("the other clients still go in");
    assert!(home.join(".pi/agent/extensions/agent-hud.ts").is_file());
    let status = clients.status(true, true);
    let notice = status["notice"]
        .as_str()
        .expect("Settings says which client is left out");
    assert!(
        notice.contains("opencode: ")
            && notice.contains("symbolic link")
            && !notice.contains("pi: "),
        "{notice}"
    );
}
