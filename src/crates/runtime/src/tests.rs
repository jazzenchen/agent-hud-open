use super::*;
use std::{collections::HashMap, fs, path::Path, process::Command};

#[cfg(unix)]
#[path = "antigravity_cli_tests_unix.rs"]
mod antigravity_cli;
#[cfg(windows)]
#[path = "antigravity_cli_tests_windows.rs"]
mod antigravity_cli;

fn isolated(root: &Path) -> Runtime {
    fs::create_dir_all(root.join("home")).unwrap();
    Runtime::with_sources(Vec::new(), root.join("app-data")).unwrap()
}

fn delegate(runtime: &Runtime, root: &Path) {
    // A regular fixture file satisfies the fixed-owner identity check. It is
    // deliberately never launched; this tests installation and persistence.
    let executable = root.join("fixed hook owner");
    fs::write(&executable, b"fixture only; never execute").unwrap();
    runtime
        .hooks_handle()
        .lock()
        .unwrap()
        .delegate_owner(hooks::InstallContext {
            home: root.join("home"),
            environment: HashMap::new(),
            canonical_executable: Some(executable.clone()),
            executable,
        })
        .unwrap();
}

fn status(runtime: &mut Runtime) -> Value {
    runtime
        .request("antigravity_cli_status", json!({}))
        .unwrap()
}

fn set_paths(runtime: &mut Runtime, value: Value) -> Result<Value, String> {
    runtime.request(
        "set_setting",
        json!({"key":"antigravityCliSettings","value":value}),
    )
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn appearance_survives_a_fresh_runtime_process() {
    const DATA: &str = "AGENTHUD_TEST_APPEARANCE_DATA";
    const SAVE: &str = "AGENTHUD_TEST_APPEARANCE_SAVE";
    const RESULT: &str = "AGENTHUD_TEST_APPEARANCE_RESULT";
    if let Some(data) = std::env::var_os(DATA) {
        // Normal persistence with no source roots, account readers, client
        // observers or delegated hooks; the demo deliberately resets its data.
        let mut runtime = Runtime::with_sources(Vec::new(), data.into()).unwrap();
        let snapshot = match std::env::var(SAVE) {
            Ok(appearance) => runtime
                .request(
                    "set_setting",
                    json!({"key":"appearance","value":appearance}),
                )
                .unwrap(),
            Err(std::env::VarError::NotPresent) => runtime.request("snapshot", json!({})).unwrap(),
            Err(error) => panic!("Invalid test appearance: {error}"),
        };
        fs::write(
            std::env::var_os(RESULT).unwrap(),
            serde_json::to_vec(&snapshot["settings"]["appearance"]).unwrap(),
        )
        .unwrap();
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("app-data");
    let name = format!(
        "{}::appearance_survives_a_fresh_runtime_process",
        module_path!().split_once("::").unwrap().1
    );
    // Reuse the folder so returning to System also replaces a saved Light.
    for appearance in ["dark", "light", "system"] {
        for phase in ["save", "reopen"] {
            let result = temp.path().join(format!("{appearance}-{phase}.json"));
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", &name, "--nocapture"])
                .env(DATA, &data)
                .env(RESULT, &result)
                .env_remove(SAVE);
            if phase == "save" {
                command.env(SAVE, appearance);
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{appearance} {phase}: {}\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            // The result file also proves the child ran, rather than an
            // unmatched --exact filter succeeding with no test selected.
            assert_eq!(read(&result), appearance, "{appearance} {phase}");
            assert_eq!(
                read(&data.join("settings.json"))["settings"]["appearance"],
                appearance
            );
        }
    }
}

#[test]
fn antigravity_cli_rejects_invalid_lists_and_missing_owners_without_file_changes() {
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = isolated(temp.path());
    let path = temp.path().join("home/profile/settings.json");
    assert_eq!(status(&mut runtime)["installable"], false);
    assert!(status(&mut runtime)["notice"]
        .as_str()
        .is_some_and(|text| !text.is_empty()));
    assert!(set_paths(&mut runtime, json!([path])).is_err());
    assert!(!path.exists());

    delegate(&runtime, temp.path());
    for invalid in [
        json!(null),
        json!("settings.json"),
        json!([1]),
        json!([""]),
        json!([" /path"]),
        json!(["/path\0invalid"]),
    ] {
        assert!(set_paths(&mut runtime, invalid).is_err());
        assert!(!path.exists());
        assert!(runtime.core.antigravity_cli_settings().is_empty());
        assert!(!temp.path().join("app-data/settings.json").exists());
    }
}
