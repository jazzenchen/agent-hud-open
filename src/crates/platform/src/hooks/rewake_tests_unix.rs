use super::*;
use crate::delivery::{
    DeliveryState, LocalClientPaths, LocalClients, Operation, PromptForm, Provider, SendRequest,
    SessionTarget,
};
use std::{
    process::{Child, Command, Stdio},
    sync::mpsc,
};

/// A process that runs until the test kills it, standing in for a client
/// or another waiter.
fn stand_in() -> Child {
    Command::new("/bin/sleep")
        .arg("120")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .unwrap()
}

fn process(child: &Child) -> Process {
    Process::of(child.id() as i32)
}

fn quit(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Agent HUD's senders, over a temporary data folder only.
fn clients(data: &Path, sessions: &[&str]) -> LocalClients {
    let targets = sessions
        .iter()
        .map(|session| SessionTarget {
            provider: Provider::Qoder,
            session_id: (*session).into(),
            thread_id: None,
            parent_id: None,
        })
        .collect();
    LocalClients::new(
        LocalClientPaths {
            claude_channel: data.join("claude-channel"),
            codex_data: data.join("codex"),
            receipts: data.join("receipts"),
        },
        targets,
    )
    .unwrap()
    .recording_deliveries(Some(data.to_owned()))
}

fn send(clients: &LocalClients, session: &str, request: &str, text: &str) {
    let receipt = clients
        .send(SendRequest {
            provider: Provider::Qoder,
            session_id: session.into(),
            request_id: request.into(),
            text: text.into(),
            form: PromptForm::Own,
            operation: Operation::Enqueue,
            expected_turn_id: None,
        })
        .unwrap();
    assert_eq!(receipt.state, DeliveryState::Waiting);
}

fn state(clients: &LocalClients, session: &str, request: &str) -> DeliveryState {
    clients
        .receipt(Provider::Qoder, session, request)
        .unwrap()
        .unwrap()
        .state
}

#[test]
fn a_newer_waiter_replaces_the_older_one_for_its_session() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path();
    let (client, first, second) = (stand_in(), stand_in(), stand_in());
    let now = SystemTime::now();
    let older = Waiter::stand(
        data,
        ("qoder", "s1"),
        process(&first),
        process(&client),
        1_000,
    )
    .unwrap();
    assert_eq!(older.look(now), Look::Wait);
    assert!(standing(data, "qoder--s1"));
    let newer = Waiter::stand(
        data,
        ("qoder", "s1"),
        process(&second),
        process(&client),
        2_000,
    )
    .unwrap();
    assert_eq!(older.look(now), Look::Leave);
    assert!(
        !older.file().exists(),
        "the older waiter takes its file away"
    );
    assert_eq!(newer.look(now), Look::Wait);
    assert!(standing(data, "qoder--s1"));

    // The newer one takes the message, once, and leaves.
    let clients = clients(data, &["s1"]);
    send(&clients, "s1", "r1", "Hello from Codex");
    assert_eq!(newer.look(now), Look::Hand("Hello from Codex".into()));
    assert_eq!(state(&clients, "s1", "r1"), DeliveryState::Started);
    assert!(!standing(data, "qoder--s1"));
    // The session reads as running the message it was handed.
    let inbox = fs::read_dir(data.join("hooks/inbox")).unwrap();
    let noted = inbox
        .flatten()
        .filter_map(|entry| serde_json::from_slice::<Value>(&fs::read(entry.path()).ok()?).ok())
        .any(|record| record["data"]["delivered"] == "r1" && record["sessionId"] == "s1");
    assert!(noted);
    for child in [client, first, second] {
        quit(child);
    }
}

#[test]
fn a_waiter_leaves_once_its_client_quits() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path();
    let (client, waiter) = (stand_in(), stand_in());
    let standing_waiter = Waiter::stand(
        data,
        ("qoder", "s1"),
        process(&waiter),
        process(&client),
        1_000,
    )
    .unwrap();
    assert_eq!(standing_waiter.look(SystemTime::now()), Look::Wait);
    // Qoder leaves its background hooks running when it quits.
    quit(client);
    assert!(!standing(data, "qoder--s1"));
    assert_eq!(standing_waiter.look(SystemTime::now()), Look::Leave);
    assert!(!standing_waiter.file().exists());
    // A message sent now waits for the session's own hooks.
    let clients = clients(data, &["s1"]);
    send(&clients, "s1", "r1", "Anyone there?");
    assert_eq!(state(&clients, "s1", "r1"), DeliveryState::Waiting);
    quit(waiter);
}

#[test]
fn a_waiter_leaves_when_its_client_moves_to_another_session() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path();
    let (client, first, second, other_client, other) =
        (stand_in(), stand_in(), stand_in(), stand_in(), stand_in());
    let old = Waiter::stand(
        data,
        ("qoder", "s1"),
        process(&first),
        process(&client),
        1_000,
    )
    .unwrap();
    // Another Qoder's session keeps its waiter.
    let elsewhere = Waiter::stand(
        data,
        ("qoder", "s9"),
        process(&other),
        process(&other_client),
        1_500,
    )
    .unwrap();
    // `/clear` in the same Qoder starts another session.
    let new = Waiter::stand(
        data,
        ("qoder", "s2"),
        process(&second),
        process(&client),
        2_000,
    )
    .unwrap();
    assert_eq!(old.look(SystemTime::now()), Look::Leave);
    assert!(!standing(data, "qoder--s1"));
    assert_eq!(new.look(SystemTime::now()), Look::Wait);
    assert_eq!(elsewhere.look(SystemTime::now()), Look::Wait);
    assert!(standing(data, "qoder--s9"));
    for child in [client, first, second, other_client, other] {
        quit(child);
    }
}

#[test]
fn a_waiter_and_a_stop_hook_never_both_take_a_message() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().to_owned();
    let clients = clients(&data, &["s1"]);
    for i in 0..20 {
        send(&clients, "s1", &format!("r{i}"), &format!("message {i}"));
    }
    let (sender, taken) = mpsc::channel();
    let takers = (0..8)
        .map(|_| {
            let (data, sender) = (data.clone(), sender.clone());
            std::thread::spawn(move || {
                let channel = crate::claude_plugin::ClaudeChannel::new(data.join("hook-channel"));
                while let Some((id, _)) = channel.take_next("qoder--s1", SystemTime::now()).unwrap()
                {
                    sender.send(id).unwrap();
                }
            })
        })
        .collect::<Vec<_>>();
    drop(sender);
    for taker in takers {
        taker.join().unwrap();
    }
    let mut ids = taken.iter().collect::<Vec<_>>();
    ids.sort();
    let mut expected = (0..20).map(|i| format!("r{i}")).collect::<Vec<_>>();
    expected.sort();
    assert_eq!(ids, expected, "each message taken once");
}

/// Runs the hook for session `s1` on its own thread, as Qoder would, with
/// `client` as the process that ran it; its exit code and stderr arrive on
/// the channel.
fn hook(data: &Path, client: Client) -> mpsc::Receiver<(i32, String)> {
    let (sender, result) = mpsc::channel();
    let data = data.to_owned();
    std::thread::spawn(move || {
        let payload = json!({"session_id":"s1","hook_event_name":"Stop","cwd":"/w"});
        let mut stderr = Vec::new();
        let code = run(
            &data,
            "qoder",
            &payload,
            (&HashMap::new(), Some(client)),
            Duration::from_millis(20),
            &mut stderr,
        );
        let _ = sender.send((code, String::from_utf8(stderr).unwrap()));
    });
    result
}

fn interactive(process: Process) -> Client {
    Client {
        process,
        arguments: vec!["node".into(), "/opt/homebrew/bin/qodercli".into()],
        terminal: true,
    }
}

fn until(what: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !what() {
        assert!(std::time::Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn the_hook_hands_a_message_to_an_idle_session_by_exiting_with_the_wake_code() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path();
    let client = stand_in();
    let result = hook(data, interactive(process(&client)));
    until(|| standing(data, "qoder--s1"));
    let clients = clients(data, &["s1"]);
    assert!(clients.hook_waiting(Provider::Qoder, "s1"));
    send(&clients, "s1", "r1", "Please review the patch");
    let (code, stderr) = result.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!((code, stderr.as_str()), (2, "Please review the patch"));
    assert_eq!(state(&clients, "s1", "r1"), DeliveryState::Started);
    assert!(!clients.hook_waiting(Provider::Qoder, "s1"));
    quit(client);
}

#[test]
fn the_hook_ends_once_its_client_quits() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path();
    let client = stand_in();
    let result = hook(data, interactive(process(&client)));
    until(|| standing(data, "qoder--s1"));
    quit(client);
    let (code, stderr) = result.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert!(!waiters(data).join("qoder--s1").exists());
}

#[test]
fn the_hook_leaves_at_once_where_its_client_would_wait_for_it() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path();
    let client = stand_in();
    // Not on a terminal, printing a prompt (`-p`), or without a client:
    // Qoder would wait for the hook to end, so it never waits.
    let piped = Client {
        terminal: false,
        ..interactive(process(&client))
    };
    let mut printing = interactive(process(&client));
    printing.arguments.extend(["-p".into(), "hello".into()]);
    for client in [piped, printing] {
        let (code, _) = hook(data, client)
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(code, 0);
    }
    let mut stderr = Vec::new();
    let payload = json!({"session_id":"s1","hook_event_name":"Stop"});
    let none = (&HashMap::new(), None);
    assert_eq!(run(data, "qoder", &payload, none, LOOK, &mut stderr), 0);
    // A client without the hook never waits either.
    let kimi = interactive(process(&client));
    let kimi = run(
        data,
        "kimi",
        &payload,
        (&HashMap::new(), Some(kimi)),
        LOOK,
        &mut stderr,
    );
    assert_eq!(kimi, 0);
    assert!(!waiters(data).exists());
    quit(client);
}
