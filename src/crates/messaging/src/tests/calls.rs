//! A sending call the relay names (`call`): one whose answer never reached
//! the relay is asked about by that name, sends once however often it comes,
//! and sends nothing once the relay gave up on it (Open #118).
use super::*;

impl World {
    /// A relay's call with its own name for it.
    fn named(&mut self, call: &str, name: &str, arguments: Value) -> Result<Value> {
        let query = self.transcripts.query();
        let host = Host {
            snapshot: &self.snapshot,
            query: &query,
            conversations: None,
        };
        self.messaging.call(
            &json!({
                "name": name,
                "arguments": arguments,
                "call": call,
                "caller": {"client":"codex","sessionId":CODEX,"turnId":null},
            }),
            &host,
            1_792_600_000_000,
        )
    }
    fn result_of(&mut self, call: &str) -> Value {
        self.named(call, "call_result", Value::Null).unwrap()
    }
    fn sent(&mut self) -> usize {
        self.messaging.journal.messages_mut().count()
    }
}

#[test]
fn a_call_whose_answer_was_lost_is_found_by_its_name_and_sends_once() {
    let mut world = World::on();
    world.check_in(QUIET);
    let message = json!({"to":QUIET,"text":"Run the tests"});
    let sent = world
        .named("call-1", "send_message", message.clone())
        .unwrap();
    assert_eq!(sent, json!({"delivered": true}));
    // The relay never read that answer: it asks by the call's name.
    let found = world.result_of("call-1");
    assert_eq!(found, json!({"found": true, "result": sent}));
    // The same call again answers as the first time and sends nothing more.
    let again = world
        .named("call-1", "send_message", message.clone())
        .unwrap();
    assert_eq!(again, sent);
    assert_eq!(world.sent(), 1);
    // Another call with the same words is a message of its own.
    let other = world.named("call-2", "send_message", message).unwrap();
    assert_eq!(other, json!({"delivered": true}));
    assert_eq!(world.sent(), 2);
}

#[test]
fn a_call_the_relay_gave_up_on_before_it_began_sends_nothing() {
    let mut world = World::on();
    world.check_in(QUIET);
    // The relay stopped waiting while the call still waited its turn.
    assert_eq!(world.result_of("call-1"), json!({"found": false}));
    let late = world.named(
        "call-1",
        "send_message",
        json!({"to":QUIET,"text":"Run the tests"}),
    );
    assert!(late.unwrap_err().contains("nothing was sent"));
    assert_eq!(world.sent(), 0);
    assert_eq!(world.result_of("call-1"), json!({"found": false}));
}

#[test]
fn a_start_found_by_its_call_names_the_session_it_started() {
    let mut world = World::on();
    let started = world
        .named(
            "call-1",
            "start_session",
            json!({"client":"claude","text":"Run the whole suite"}),
        )
        .unwrap();
    assert_eq!(started, json!({"delivered": true, "sessionId": STARTED}));
    let found = world.result_of("call-1");
    assert_eq!(found["result"], started);
}

#[test]
fn a_message_sent_by_an_unnamed_call_keeps_no_call_in_the_journal() {
    let mut world = World::on();
    world.check_in(QUIET);
    world.send(("codex", CODEX), QUIET, "Run the tests");
    let journal = world.fixture.root.join("data/messaging/messages.json");
    let journal = std::fs::read_to_string(journal).unwrap();
    assert!(!journal.contains("\"call\""), "{journal}");
}

#[test]
fn call_result_and_sent_messages_answer_without_looking_up_live_extras() {
    let mut world = World::on();
    // A Codex desktop thread open outside the 24-hour snapshot: listing
    // sessions looks up its head, which waits on the application's data.
    let old = "01a10000-0000-7000-8000-000000000001";
    let locks = world.fixture.home().join(".codex/thread-writer-locks");
    std::fs::create_dir_all(&locks).unwrap();
    let lock = std::fs::File::create(locks.join(format!("{old}.lock"))).unwrap();
    lock.lock().unwrap();
    world.transcripts.sessions.lock().unwrap().insert(
        old.into(),
        json!({"id": old, "source": "codex", "title": "Older desktop thread", "parentId": null}),
    );
    let calls = world.transcripts.calls.clone();
    let lookups = || calls.load(AtomicOrdering::SeqCst);
    let found = world.result_of("call-unknown");
    assert_eq!(found, json!({"found": false}));
    let sent = world
        .named("call-2", "list_sent_messages", json!({}))
        .unwrap();
    assert_eq!(sent["count"], 0, "{sent}");
    assert_eq!(lookups(), 0, "the journal alone answers both");
    let _ = world.named("call-3", "list_sessions", json!({})).unwrap();
    assert_eq!(lookups(), 1, "listing sessions still looks the thread up");
}
