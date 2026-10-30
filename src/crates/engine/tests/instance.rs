use agent_hud_runtime::instance;
use std::process::{Command, Stdio};

fn engine(directory: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_agent-hud-engine"));
    command
        .env("AGENTHUD_DATA_DIR", directory)
        .env("HOME", directory.join("home"))
        .env("USERPROFILE", directory.join("home"))
        .stdin(Stdio::null());
    command
}

#[test]
fn standalone_refuses_an_owned_data_directory_before_opening_its_database() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("data");
    let _owner = instance::claim_collector(&directory.join("instance.lock"), None, None).unwrap();
    let output = engine(&directory).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty(), "stdout stays protocol-only");
    assert!(!directory.join("ingestion.sqlite").exists());
    assert!(!directory.join("settings.json").exists());
}

#[test]
fn standalone_refuses_an_unavailable_lock_before_opening_its_database() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("instance.lock")).unwrap();
    let output = engine(directory.path()).output().unwrap();
    assert!(!output.status.success());
    assert!(!directory.path().join("ingestion.sqlite").exists());
}

#[test]
fn app_channel_stays_a_relay_while_a_collector_owns_the_data() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("data");
    let _owner = instance::claim_collector(&directory.join("instance.lock"), None, None).unwrap();
    let output = engine(&directory).arg("--app-channel").output().unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!directory.join("ingestion.sqlite").exists());
    assert!(!directory.join("settings.json").exists());
}
