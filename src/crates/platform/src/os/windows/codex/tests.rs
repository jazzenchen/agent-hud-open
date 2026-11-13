use super::*;

fn file(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"synthetic executable fixture").unwrap();
}
fn environment(root: &Path) -> Environment {
    Environment::from([
        (
            "LOCALAPPDATA".into(),
            root.join("local").to_string_lossy().into_owned(),
        ),
        (
            "ProgramFiles".into(),
            root.join("programs").to_string_lossy().into_owned(),
        ),
    ])
}
fn runtime(root: &Path, hash: &str, modified: u64) -> PathBuf {
    let folder = root.join("local/OpenAI/Codex/bin").join(hash);
    for name in CACHE_FILES {
        file(&folder.join(name));
    }
    let engine = folder.join("codex.exe");
    fs::File::options()
        .write(true)
        .open(&engine)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(modified))
        .unwrap();
    engine
}

#[test]
fn registered_desktop_engine_precedes_normal_install_and_complete_cached_runtimes() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let application = root.join("WindowsApps/OpenAI.Codex_fixture/app/ChatGPT.exe");
    file(&application);
    let bundled = application.parent().unwrap().join("resources/codex.exe");
    file(&bundled);
    let normal = root.join("local/Programs/Codex/resources/codex.exe");
    file(&normal);
    let old = runtime(root, "0123456789abcdef", 1_700_000_000);
    let new = runtime(root, "fedcba9876543210", 1_700_000_100);
    runtime(root, ".staging-1111111111111111-temp", 1_700_000_300);
    file(&root.join("local/OpenAI/Codex/bin/2222222222222222/codex.exe"));
    let found = locate(root, &environment(root), std::slice::from_ref(&application));
    assert!(found.installed);
    assert_eq!(found.engines, [bundled, normal, new, old]);
    assert_eq!(found.applications, [application]);
}

#[test]
fn the_store_app_is_started_before_a_copy_in_the_usual_folders() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let store = root.join("WindowsApps/OpenAI.Codex_fixture/app/Codex.exe");
    file(&store);
    let usual = root.join("local/Programs/Codex/Codex.exe");
    file(&usual);
    file(&root.join("programs/ChatGPT/resources/codex.exe"));
    let found = locate(root, &environment(root), std::slice::from_ref(&store));
    assert_eq!(found.applications, [store, usual]);
}

#[test]
fn a_registered_app_is_detected_even_when_its_cli_is_unavailable() {
    let root = tempfile::tempdir().unwrap();
    let application = root.path().join("package/app/ChatGPT.exe");
    file(&application);
    let found = locate(root.path(), &environment(root.path()), &[application]);
    assert!(found.installed);
    assert!(found.engines.is_empty());
}

#[test]
fn the_registered_runtime_hash_selects_its_copy_before_newer_unrelated_caches() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let application = root.join("package/app/ChatGPT.exe");
    file(&application);
    let resources = application.parent().unwrap().join("resources");
    for name in CACHE_FILES {
        file(&resources.join(name));
    }
    // Independently computed from the fixture bytes with the official at/ct algorithm.
    assert_eq!(
        runtime_hash(&resources).as_deref(),
        Some("6128ded9feafc5e2")
    );
    let matching = runtime(root, "6128ded9feafc5e2", 1_700_000_000);
    let unrelated = runtime(root, "ffffffffffffffff", 1_700_000_100);
    let found = locate(root, &environment(root), &[application]);
    assert_eq!(
        found.engines,
        [matching, resources.join("codex.exe"), unrelated]
    );
}

#[test]
fn expired_or_removed_registered_apps_are_queried_again() {
    let root = tempfile::tempdir().unwrap();
    let application = root.path().join("package/app/ChatGPT.exe");
    file(&application);
    let now = Instant::now();
    let found = Registered {
        checked: now,
        applications: vec![application.clone()],
    };
    assert!(found.reusable(now + Duration::from_secs(299)));
    assert!(!found.reusable(now + Duration::from_secs(300)));
    fs::remove_file(application).unwrap();
    assert!(!found.reusable(now));
    let absent = Registered {
        checked: now,
        applications: vec![],
    };
    assert!(absent.reusable(now + Duration::from_secs(59)));
    assert!(!absent.reusable(now + Duration::from_secs(60)));
}
