use super::*;
use crate::hooks::clients::Clients;
use std::os::unix::fs::symlink;

fn context(home: &Path) -> InstallContext {
    fs::create_dir_all(home).unwrap();
    let executable = home.join("fixed hook owner");
    fs::write(&executable, b"fixture only; never execute").unwrap();
    InstallContext {
        home: home.to_owned(),
        environment: HashMap::new(),
        canonical_executable: Some(executable.clone()),
        executable,
    }
}

fn opencode_parent_is_skipped(dangling: bool) {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let context = context(&home);
    fs::create_dir_all(home.join(".config")).unwrap();
    // A dangling configuration link alone is not enough for discovery.
    fs::create_dir_all(home.join(".local/share/opencode")).unwrap();
    let directory = home.join(".config/opencode");
    let destination = temp.path().join("linked-configuration");
    let script = destination.join("plugin/agent-hud.js");
    let original = b"// Agent HUD OpenCode session observer\n// previous version\n";
    if !dangling {
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        fs::write(&script, original).unwrap();
        fs::write(destination.join("opencode.json"), b"{}").unwrap();
    }
    symlink(&destination, &directory).unwrap();

    for enabled in [true, false] {
        let error = match observer_plan(&context, "opencode", enabled) {
            Err(error) => error,
            Ok(_) => panic!("a linked client directory must be refused"),
        };
        assert_eq!(
            error,
            l10n::text(
                "观察插件的路径是符号链接，客户端文件未作改动",
                "The observer path is a symbolic link; no client file was changed",
            )
        );
    }
    let mut clients = Clients::default();
    clients.delegate(context).unwrap();
    for enabled in [true, false] {
        // This is the installer used by Hooks::set_enabled during startup.
        clients.install(enabled).unwrap();
        let status = clients.status(enabled, enabled);
        let client = status["clients"]
            .as_array()
            .unwrap()
            .iter()
            .find(|client| client["source"] == "opencode")
            .expect("the failing client must still be discovered");
        assert_eq!(client["installed"], false);
        let notice = client["notice"].as_str().unwrap();
        assert!(!notice.is_empty());
        assert!(status["notice"].as_str().unwrap().contains(notice));
        assert_eq!(fs::read_link(&directory).unwrap(), destination);
        if dangling {
            assert!(!destination.exists());
            assert!(!directory.exists());
        } else {
            assert_eq!(fs::read(&script).unwrap(), original);
            assert_eq!(fs::read(destination.join("opencode.json")).unwrap(), b"{}");
            assert_eq!(fs::read_dir(script.parent().unwrap()).unwrap().count(), 1);
        }
    }
}

#[test]
fn a_dangling_opencode_parent_is_nonfatal_and_unchanged() {
    opencode_parent_is_skipped(true);
}

#[test]
fn an_existing_opencode_parent_link_is_nonfatal_and_unchanged() {
    opencode_parent_is_skipped(false);
}

#[test]
fn nested_and_relocated_client_directory_links_are_refused() {
    for (source, relative, variable) in [
        ("opencode", ".config/opencode/plugin", None),
        ("pi", ".pi", None),
        ("pi", "relocated-pi", Some("PI_CODING_AGENT_DIR")),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let mut context = context(&home);
        let directory = if variable.is_some() {
            temp.path().join(relative)
        } else {
            home.join(relative)
        };
        fs::create_dir_all(directory.parent().unwrap()).unwrap();
        let destination = temp.path().join("linked-configuration");
        fs::create_dir_all(&destination).unwrap();
        symlink(&destination, &directory).unwrap();
        if let Some(variable) = variable {
            context
                .environment
                .insert(variable.into(), directory.to_string_lossy().into_owned());
        }
        for enabled in [true, false] {
            assert!(observer_plan(&context, source, enabled).is_err());
            assert_eq!(fs::read_link(&directory).unwrap(), destination);
            assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
        }
    }
}

#[test]
fn home_and_shared_ancestor_links_do_not_block_installation() {
    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real");
    let shared = temp.path().join("shared-link");
    let home = shared.join("home-link");
    fs::create_dir_all(real.join("home")).unwrap();
    symlink(&real, &shared).unwrap();
    symlink(real.join("home"), real.join("home-link")).unwrap();
    fs::create_dir_all(real.join("config")).unwrap();
    symlink(real.join("config"), home.join(".config")).unwrap();
    let context = context(&home);

    for source in ["opencode", "pi"] {
        let plan = observer_plan(&context, source, true).unwrap();
        assert!(apply_observer_plan(&plan).unwrap());
        assert_eq!(fs::read(&plan.path).unwrap(), plan.body.as_bytes());
        assert!(!observer_plan(&context, source, true).unwrap().changed);
        let plan = observer_plan(&context, source, false).unwrap();
        assert!(apply_observer_plan(&plan).unwrap());
        assert!(!plan.path.exists());
    }
    assert_eq!(fs::read_link(&shared).unwrap(), real);
    assert_eq!(fs::read_link(&home).unwrap(), real.join("home"));
    assert_eq!(
        fs::read_link(home.join(".config")).unwrap(),
        real.join("config")
    );
}

#[test]
fn a_parent_link_added_after_planning_is_not_written_or_removed() {
    for enabled in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let context = context(&home);
        let directory = home.join(".config/opencode");
        let script = directory.join("plugin/agent-hud.js");
        let original = b"// Agent HUD OpenCode session observer\n// previous version\n";
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        fs::write(&script, original).unwrap();
        let plan = observer_plan(&context, "opencode", enabled).unwrap();
        assert!(plan.changed);

        let destination = temp.path().join("relocated-configuration");
        fs::rename(&directory, &destination).unwrap();
        symlink(&destination, &directory).unwrap();
        assert!(apply_observer_plan(&plan).is_err());
        assert_eq!(fs::read(&script).unwrap(), original);
        assert_eq!(fs::read_link(&directory).unwrap(), destination);
        assert_eq!(fs::read_dir(script.parent().unwrap()).unwrap().count(), 1);
    }
}
