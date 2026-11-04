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

#[test]
fn each_fork_takes_its_parents_observer_in_its_own_folder_and_reports_as_itself() {
    use serde_json::json;
    use std::sync::Arc;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let context = context(&home);
    for (source, script, marker) in [
        (
            "kilo",
            ".config/kilo/plugins/agent-hud.js",
            "// Agent HUD Kilo Code session observer\nconst SOURCE = \"kilo\"",
        ),
        (
            "mimocode",
            ".config/mimocode/plugins/agent-hud.js",
            "// Agent HUD MiMo Code session observer\nconst SOURCE = \"mimocode\"",
        ),
        (
            "omp",
            ".omp/agent/extensions/agent-hud.ts",
            "// Agent HUD Oh My Pi session observer\nconst SOURCE = \"omp\"",
        ),
    ] {
        let plan = observer_plan(&context, source, true).unwrap();
        assert_eq!(plan.path, home.join(script));
        assert!(apply_observer_plan(&plan).unwrap());
        assert!(fs::read_to_string(home.join(script))
            .unwrap()
            .starts_with(marker));
        assert!(!observer_plan(&context, source, true).unwrap().changed);
    }

    // What the plugins write comes back as their own clients' turns.
    let now = super::super::now_ms();
    let mut observers = Observers::new(&home, &HashMap::new(), Arc::new(|| {}));
    for (folder, record) in [
        (
            ".local/share/kilo/agent-hud/turns",
            json!({"version":1,"sessionID":"kilo:ses_1","turnID":"msg_1","startedAtMs":now - 5_000,"completedAtMs":now - 1_000}),
        ),
        (
            ".omp/agent/agent-hud/turns",
            json!({"version":1,"sessionID":"omp:s1","turnID":"t1","state":"running","startedAtMs":now - 5_000,"observedAtMs":now - 1_000}),
        ),
    ] {
        fs::create_dir_all(home.join(folder)).unwrap();
        fs::write(
            home.join(folder).join(format!("{}.json", "a".repeat(64))),
            record.to_string(),
        )
        .unwrap();
    }
    let mut events = observers
        .drain()
        .into_iter()
        .map(|event| {
            let state = event.data["state"].as_str().unwrap_or_default().to_owned();
            (event.source, event.session_id, state)
        })
        .collect::<Vec<_>>();
    events.sort();
    assert_eq!(
        events,
        [
            ("kilo".into(), "kilo:ses_1".into(), "completed".into()),
            ("omp".into(), "omp:s1".into(), "running".into()),
        ]
    );
}
