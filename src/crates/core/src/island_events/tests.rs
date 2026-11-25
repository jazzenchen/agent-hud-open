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

#[test]
fn a_subagent_completion_does_not_create_a_new_reply_alert() {
    let mut tracker = Tracker::new();
    let at = tracker.started + 30_000;
    let completed = |id: &str, parent: Option<&str>| {
        json!({
            "id": id,
            "source": "codex",
            "title": id,
            "model": "gpt-5.5",
            "parentId": parent,
            "liveFacts": {
                "outcome": "completed",
                "endedAt": iso(at),
                "turnId": "turn-1"
            }
        })
    };

    let events = tracker.update(
        &[completed("main", None), completed("child", Some("main"))],
        &[],
        &[],
        &[],
        at,
    );
    let completions = events["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == "completion")
        .collect::<Vec<_>>();

    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0]["sessionId"], "main");
}

fn codex(hooks: &[&str]) -> Value {
    json!({"id":"codex","name":"Codex","enabled":true,"accountStatus":{"status":"ready","unreviewedHooks":hooks}})
}
fn reviews(tracker: &mut Tracker, row: Value, now: i64) -> Vec<Value> {
    tracker.update(&[], &[], &[], &[row], now)["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == "hooks-review-needed")
        .cloned()
        .collect()
}

#[test]
fn hooks_waiting_for_the_user_are_told_after_launch_then_once_a_day() {
    let mut tracker = Tracker::new();
    let day = 86_400_000;
    let start = tracker.started;
    let waiting = || codex(&["permissionRequest", "stop"]);
    // Right after launch the island would take it as already seen.
    assert!(reviews(&mut tracker, waiting(), start + 1_000).is_empty());
    let told = reviews(&mut tracker, waiting(), start + 30_000);
    assert_eq!(told.len(), 1);
    assert_eq!(told[0]["source"], "codex");
    assert_eq!(told[0]["title"], "Codex");
    assert_eq!(told[0]["islandAlert"], true);
    assert_eq!(told[0]["hooks"], json!(["permissionRequest", "stop"]));
    assert!(told[0]["message"].as_str().unwrap().contains("Codex"));
    // Where to allow them, as Codex itself names it.
    assert!(told[0]["step"].as_str().unwrap().contains("Allow all"));
    assert_eq!(reviews(&mut tracker, waiting(), start + 60_000).len(), 1);
    assert_eq!(
        reviews(&mut tracker, waiting(), start + 30_000 + day).len(),
        2
    );
    // Allowed, then changed again: told at once.
    assert_eq!(
        reviews(&mut tracker, codex(&[]), start + 30_001 + day).len(),
        2
    );
    assert_eq!(
        reviews(&mut tracker, waiting(), start + 30_002 + day).len(),
        3
    );
}

#[test]
fn only_the_permission_hook_keeps_approvals_from_the_hud() {
    let mut tracker = Tracker::new();
    let at = tracker.started + 30_000;
    let approvals = reviews(&mut tracker, codex(&["permissionRequest"]), at);
    let mut tracker = Tracker::new();
    let at = tracker.started + 30_000;
    let progress = reviews(&mut tracker, codex(&["userPromptSubmit"]), at);
    assert_ne!(approvals[0]["message"], progress[0]["message"]);
}

#[test]
fn a_failed_or_limited_turn_and_a_closed_session_alert_but_an_interrupted_one_does_not() {
    let mut tracker = Tracker::new();
    let at = tracker.started + 30_000;
    let ended = |id: &str, outcome: &str, turn: Option<&str>| {
        json!({"id":id,"source":"claude","title":id,"parentId":null,
            "liveFacts":{"outcome":outcome,"endedAt":iso(at),"turnId":turn}})
    };
    let sessions = [
        ended("failed", "failed", Some("t1")),
        ended("limited", "limited", Some("t1")),
        ended("closed", "closed", None),
        ended("interrupted", "interrupted", Some("t1")),
        ended("no-turn", "completed", None),
    ];
    let outcomes = |events: Value| {
        let mut told = events["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "completion")
            .map(|event| {
                format!(
                    "{}:{}",
                    event["sessionId"].as_str().unwrap(),
                    event["outcome"].as_str().unwrap()
                )
            })
            .collect::<Vec<_>>();
        told.sort();
        told
    };
    assert_eq!(
        outcomes(tracker.update(&sessions, &[], &[], &[], at)),
        ["closed:closed", "failed:failed", "limited:limited"]
    );
    // Told once.
    assert_eq!(
        outcomes(tracker.update(&sessions, &[], &[], &[], at + 1)).len(),
        3
    );
}
