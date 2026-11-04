use super::watch::changed_paths;
use super::*;
use notify::{Event, EventKind};
use serde_json::json;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn a_session_record_keeps_its_first_surface() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path();
    let at = now_ms() - 10_000;
    let origin = |offset: i64, surface: &str| HookEvent {
        source: "claude".into(),
        session_id: "s1".into(),
        kind: "origin".into(),
        at_ms: at + offset,
        data: json!({"host":"cli","surface":surface}),
    };
    for (offset, surface) in [(0, "cli:iterm"), (5, "cli:ghostty"), (9, "cli:warp")] {
        record_event(data, &origin(offset, surface)).unwrap();
    }
    let path = data
        .join("hooks/inbox")
        .join(format!("{}.json", key(&origin(0, ""))));
    let (kept, _) = read(&path).unwrap();
    assert_eq!(kept.data["surface"], "cli:warp");
    assert_eq!(kept.data["first"], json!({"surface":"cli:iterm","at":at}));
    // An older report changes nothing.
    record_event(data, &origin(-5, "cli:terminal")).unwrap();
    assert_eq!(read(&path).unwrap().0.data, kept.data);
    let surface = agent_hud_agents::naming::surfaces::Surface::parse("mobile:iphone").unwrap();
    record_delivery(data, "claude", "s1", &surface, at).unwrap();
    record_delivery(data, "claude", "s1", &surface, at + 1).unwrap();
    let delivery = HookEvent {
        kind: "delivery".into(),
        ..origin(0, "")
    };
    let path = data
        .join("hooks/inbox")
        .join(format!("{}.json", key(&delivery)));
    let (kept, _) = read(&path).unwrap();
    assert_eq!(
        kept.data["first"],
        json!({"surface":"mobile:iphone","at":at})
    );
}

#[test]
fn a_session_keeps_the_title_its_first_prompt_gave() {
    let root = tempfile::tempdir().unwrap();
    let data = &root.path().join("data");
    let at = now_ms() - 10_000;
    let observation = |offset: i64, title: Option<&str>| HookEvent {
        source: "trae".into(),
        session_id: "trae:c1".into(),
        kind: "session-observation".into(),
        at_ms: at + offset,
        data: json!({"turnId":"c1","state":"running","title":title}),
    };
    record_event(data, &observation(0, Some("Fix the login"))).unwrap();
    record_event(data, &observation(5, None)).unwrap();
    let path = data
        .join("hooks/inbox")
        .join(format!("{}.json", key(&observation(0, None))));
    let (kept, _) = read(&path).unwrap();
    assert_eq!(kept.at_ms, at + 5);
    assert_eq!(kept.data["title"], "Fix the login");
    // A later prompt does not rename it.
    record_event(data, &observation(9, Some("Add tests"))).unwrap();
    assert_eq!(read(&path).unwrap().0.data["title"], "Fix the login");
}

#[test]
fn concurrent_account_callbacks_keep_the_newest_report() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path();
    let directory = data.join("hooks/inbox");
    prepare(&data.join("hooks")).unwrap();
    prepare(&directory).unwrap();
    let lock = account_lock(&directory).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let at = now_ms() - 1_000;
    let mut workers = Vec::new();
    for index in 0..16 {
        let data = data.to_path_buf();
        let started = started_tx.clone();
        let done = done_tx.clone();
        workers.push(std::thread::spawn(move || {
            let event = HookEvent {
                source: "antigravity".into(),
                session_id: "one-account".into(),
                kind: "account".into(),
                at_ms: at + index,
                data: json!({"sample": index}),
            };
            started.send(()).unwrap();
            record_event(&data, &event).unwrap();
            done.send(()).unwrap();
        }));
    }
    for _ in 0..16 {
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    assert!(
        matches!(
            done_rx.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ),
        "callbacks must wait for the other writer's timestamp/rename transaction"
    );
    drop(lock);
    for worker in workers {
        worker.join().unwrap();
    }
    let records = fs::read_dir(&directory)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_record(path))
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 1);
    let (event, _) = read(&records[0]).unwrap();
    assert_eq!(event.at_ms, at + 15);
    assert_eq!(event.data["sample"], 15);
    assert!(changed_paths(
        &directory,
        Ok(Event::new(EventKind::Any).add_path(directory.join(ACCOUNT_LOCK)))
    )
    .is_none());
}
