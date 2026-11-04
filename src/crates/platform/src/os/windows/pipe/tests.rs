use super::super::acl::{owned_path, private_directory, private_file};
use super::client::connect;
use super::*;
use serde_json::json;
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

#[test]
fn missing_private_directory_ancestors_keep_user_ownership() {
    let temp = tempfile::tempdir().unwrap();
    let outer = temp.path().join("outer");
    let inner = outer.join("inner");
    let data = inner.join("private");
    private_directory(&data).unwrap();
    for directory in [&outer, &inner, &data] {
        assert!(owned_path(directory));
        private_directory(directory).unwrap();
    }
}

#[test]
fn private_files_keep_user_ownership_and_reopening_preserves_contents() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("private");
    private_directory(&directory).unwrap();
    let path = directory.join("account.lock");
    private_file(&path, true)
        .unwrap()
        .write_all(b"preserved")
        .unwrap();
    assert!(owned_path(&path));
    assert!(private_file(&path, false).is_ok());
    assert_eq!(std::fs::read(&path).unwrap(), b"preserved");
    assert_eq!(
        private_file(&path, true).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );

    // Hosted runners are elevated: files made with default security can
    // belong to Administrators. Never repair those by taking ownership.
    let other = temp.path().join("default-owner");
    std::fs::write(&other, b"untouched").unwrap();
    if !owned_path(&other) {
        assert_eq!(
            private_file(&other, false).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(!owned_path(&other));
        assert_eq!(std::fs::read(&other).unwrap(), b"untouched");
    }
}

#[test]
fn private_pipe_round_trip_and_exclusive_owner() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("private");
    let handler: Handler = Arc::new(|message, _| Some(message));
    let server = Server::start(&data, "test", 1024, 1024, handler.clone()).unwrap();
    assert!(server.is_listening());
    assert!(owned_path(&data));
    assert!(Server::start(&data, "test", 1024, 1024, handler).is_err());
    let message = json!({"text": "请求 C:\\my project", "empty": ""});
    assert_eq!(
        request(
            &data,
            "test",
            &message,
            1024,
            1024,
            Duration::from_secs(2),
            || false
        )
        .unwrap(),
        Some(message)
    );
}

#[test]
fn saturated_pipe_shutdown_cancels_waiting_approvals() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("private");
    let started = Arc::new(AtomicUsize::new(0));
    let count = started.clone();
    let handler: Handler = Arc::new(move |_, active| {
        count.fetch_add(1, Ordering::Release);
        while active.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    });
    let server = Server::start(&data, "saturated", 1024, 1024, handler).unwrap();
    let name = name(&data, "saturated").unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _clients = runtime.block_on(async {
        let mut clients = Vec::new();
        for _ in 0..64 {
            let mut client = connect(&name, Duration::from_secs(5)).await.unwrap();
            client.write_all(b"{}\n").await.unwrap();
            clients.push(client);
        }
        clients
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while started.load(Ordering::Acquire) < 63 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(started.load(Ordering::Acquire), 63);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        drop(server);
        let _ = sender.send(());
    });
    receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("saturated server must stop");
}
