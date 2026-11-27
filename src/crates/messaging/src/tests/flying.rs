//! A sending call hands its words over with the messages' lock let go
//! (Open #192): other calls answer meanwhile, and what one lock gave stays.
use super::*;

const NOW: i64 = 1_792_600_000_000;

impl World {
    /// The first step of a call named `call` from the Codex session.
    fn begin(&mut self, call: &str, name: &str, arguments: Value) -> Step {
        let query = self.transcripts.query();
        let host = Host {
            snapshot: &self.snapshot,
            query: &query,
            conversations: None,
        };
        self.messaging.begin(
            &json!({
                "name": name,
                "arguments": arguments,
                "call": call,
                "caller": {"client":"codex","sessionId":CODEX,"turnId":null},
            }),
            &host,
            NOW,
        )
    }
    fn deliver(&mut self, step: Step) -> Value {
        let Step::Deliver(pending) = step else {
            panic!("expected a delivery");
        };
        let delivered = pending.deliver(NOW);
        self.messaging.finish(*pending, delivered).unwrap()
    }
}

fn waits(step: &Step) -> bool {
    matches!(step, Step::Wait)
}

#[test]
fn other_calls_answer_while_a_message_is_handed_over() {
    let mut world = World::on();
    world.check_in(QUIET);
    let first = world.begin("c1", "send_message", json!({"to":QUIET,"text":"One"}));
    assert!(matches!(first, Step::Deliver(_)));
    // Listing and sending elsewhere do not wait for it.
    let listed = world.begin("", "list_sessions", json!({}));
    assert!(matches!(listed, Step::Answer(Ok(_))));
    let elsewhere = world.begin(
        "c3",
        "send_message",
        json!({"to":CLAUDE,"text":"Elsewhere"}),
    );
    assert_eq!(world.deliver(elsewhere)["delivered"], true);
    // The same call, its result, and the next message into that session wait.
    assert!(waits(&world.begin(
        "c1",
        "send_message",
        json!({"to":QUIET,"text":"One"})
    )));
    assert!(waits(&world.begin("c1", "call_result", Value::Null)));
    assert!(waits(&world.begin(
        "c2",
        "send_message",
        json!({"to":QUIET,"text":"Two"})
    )));
    assert_eq!(world.deliver(first)["delivered"], true);
}

#[test]
fn a_call_that_comes_again_after_its_delivery_sends_once_and_in_order() {
    let mut world = World::on();
    world.check_in(QUIET);
    let first = world.begin("c1", "send_message", json!({"to":QUIET,"text":"One"}));
    world.deliver(first);
    // Asked again, it answers from the journal and sends nothing more.
    let again = world.begin("c1", "send_message", json!({"to":QUIET,"text":"One"}));
    let Step::Answer(Ok(answer)) = again else {
        panic!("expected the first answer");
    };
    assert_eq!(answer["delivered"], true);
    let Step::Answer(Ok(found)) = world.begin("c1", "call_result", Value::Null) else {
        panic!("expected the call's result");
    };
    assert_eq!(found["found"], true);
    let second = world.begin("c2", "send_message", json!({"to":QUIET,"text":"Two"}));
    world.deliver(second);
    let calls = world
        .messaging
        .journal
        .messages()
        .filter_map(|m| m.call.clone())
        .collect::<Vec<_>>();
    assert_eq!(calls, ["c1", "c2"]);
}

#[test]
fn a_start_runs_outside_the_lock_and_names_its_session() {
    let mut world = World::on();
    let start = world.begin(
        "s1",
        "start_session",
        json!({"client":"claude","text":"Begin"}),
    );
    assert!(matches!(start, Step::Deliver(_)));
    assert!(waits(&world.begin("s1", "call_result", Value::Null)));
    let listed = world.begin("", "list_sessions", json!({}));
    assert!(matches!(listed, Step::Answer(Ok(_))));
    let answer = world.deliver(start);
    let kept = world.messaging.journal.messages().last().unwrap().clone();
    assert_eq!(kept.call.as_deref(), Some("s1"));
    assert_eq!(answer["delivered"], json!(kept.state != "refused"));
}
