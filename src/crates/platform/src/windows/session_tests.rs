use super::{set_ending, spawn, SpawnGate};
use std::{
    io,
    process::{Command, Stdio},
    sync::{mpsc, Arc},
    time::Duration,
};

#[test]
fn pending_shutdown_blocks_launch_and_cancellation_restores_it() {
    let gate = SpawnGate::new();
    assert_eq!(gate.run(|| false, || Ok(7)).unwrap(), 7);
    gate.set_ending(true);
    let error = gate
        .run(|| false, || -> io::Result<()> { panic!("關機後不可啟動") })
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    gate.set_ending(false);
    assert_eq!(gate.run(|| false, || Ok(9)).unwrap(), 9);
}

#[test]
fn windows_shutdown_metric_also_blocks_without_a_window_notification() {
    let gate = SpawnGate::new();
    let error = gate
        .run(
            || true,
            || -> io::Result<()> { panic!("系統關機時不可啟動") },
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(gate.run(|| false, || Ok(())).is_ok());
}

#[test]
fn shutdown_transition_and_actual_launch_share_the_same_lock() {
    let gate = Arc::new(SpawnGate::new());
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let launcher = {
        let gate = Arc::clone(&gate);
        std::thread::spawn(move || {
            gate.run(
                || false,
                || {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                },
            )
            .unwrap();
        })
    };
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (transition_tx, transition_rx) = mpsc::channel();
    let (ending_tx, ending_rx) = mpsc::channel();
    let ending = {
        let gate = Arc::clone(&gate);
        std::thread::spawn(move || {
            transition_tx.send(()).unwrap();
            gate.set_ending(true);
            ending_tx.send(()).unwrap();
        })
    };
    transition_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    // 已經開始的啟動可以完成；關機狀態切換完成後，不可再有新的啟動穿過。
    assert!(matches!(
        ending_rx.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    release_tx.send(()).unwrap();
    launcher.join().unwrap();
    ending_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    ending.join().unwrap();
    assert_eq!(
        gate.run(|| false, || Ok(())).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
}

#[test]
fn configured_command_is_checked_at_spawn_and_can_run_after_cancellation() {
    const CHILD: &str = "AGENT_HUD_TEST_SESSION_CHILD";
    if std::env::var_os(CHILD).is_none() {
        // 全域門閂在獨立測試程序內切換，不影響其他同時執行的帳戶或原生程序測試。
        let name = format!(
            "{}::configured_command_is_checked_at_spawn_and_can_run_after_cancellation",
            module_path!().split_once("::").unwrap().1
        );
        let mut command = Command::new(std::env::current_exe().unwrap());
        crate::os::configure_command(&mut command);
        let output = command
            .args(["--exact", &name, "--nocapture"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("started.txt");
    let name = format!(
        "{}::spawn_probe_writes_a_marker",
        module_path!().split_once("::").unwrap().1
    );
    let mut prepared = Command::new(std::env::current_exe().unwrap());
    crate::os::configure_command(&mut prepared);
    prepared
        .args(["--exact", &name])
        .env("AGENT_HUD_TEST_SESSION_MARKER", &marker)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    set_ending(true);
    assert!(super::is_ending());
    assert_eq!(
        spawn(&mut prepared).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(!marker.exists());
    set_ending(false);
    assert!(!super::is_ending());
    assert!(spawn(&mut prepared).unwrap().wait().unwrap().success());
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "已啟動");
}

#[test]
fn spawn_probe_writes_a_marker() {
    if let Some(path) = std::env::var_os("AGENT_HUD_TEST_SESSION_MARKER") {
        std::fs::write(path, "已啟動").unwrap();
    }
}
