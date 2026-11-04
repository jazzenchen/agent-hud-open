use super::*;

fn provider(status: &str, failure: Option<&str>) -> Value {
    json!({"id":"claude","name":"Claude Code","enabled":true,"accountStatus":{"status":status,"failure":failure,"notice":"Claude Code is signed out","failingSince":null}})
}
fn told(tracker: &mut Tracker, row: Value, now: i64) -> usize {
    tracker.update(&[], &[], &[], &[row], now)["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == "sign-in-needed")
        .count()
}

#[test]
fn a_sign_out_is_told_once_a_day_and_again_after_signing_in() {
    let mut tracker = Tracker::new();
    let day = 86_400_000;
    let now = Utc::now().timestamp_millis();
    // The first look only notes how things stand.
    assert_eq!(told(&mut tracker, provider("ready", None), now), 0);
    let out = || provider("error", Some("signedOut"));
    assert_eq!(told(&mut tracker, out(), now + 1), 1);
    assert_eq!(told(&mut tracker, out(), now + 2), 1, "told once");
    // Signed in again, then out again within the day: told again.
    assert_eq!(told(&mut tracker, provider("ready", None), now + 3), 1);
    assert_eq!(told(&mut tracker, out(), now + 4), 2);
    let event = tracker.events.back().unwrap();
    assert_eq!(event["source"], "claude");
    assert_eq!(event["title"], "Claude Code");
    assert_eq!(event["message"], "Claude Code is signed out");
    assert_eq!(event["islandAlert"], true);
    // A failure that is not a sign-out in between does not count as
    // signing in, so the same day stays quiet.
    assert_eq!(
        told(&mut tracker, provider("error", Some("failed")), now + 5),
        2
    );
    assert_eq!(told(&mut tracker, out(), now + 6), 2);
    assert_eq!(told(&mut tracker, out(), now + day + 7), 2, "still out");
}

#[test]
fn a_reading_that_only_missed_an_answer_is_not_a_sign_out() {
    let mut tracker = Tracker::new();
    let now = Utc::now().timestamp_millis();
    assert_eq!(told(&mut tracker, provider("ready", None), now), 0);
    assert_eq!(
        told(&mut tracker, provider("error", Some("transient")), now + 1),
        0
    );
}
