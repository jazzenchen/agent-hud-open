use super::*;
use crate::iso;
use serde_json::json;
use std::collections::HashSet;

fn observation(state: &str, at: i64) -> HookEvent {
    HookEvent {
        source: "pi".into(),
        session_id: "pi:session".into(),
        kind: "session-observation".into(),
        at_ms: at,
        data: json!({"state":state,"turnId":"turn","startedAtMs":at-1000,"outcome":null}),
    }
}

#[test]
fn a_cursor_prompt_makes_its_session_run_until_the_stop_hook() {
    let at = 1_792_470_000_000i64;
    let event = |kind: &str, at: i64, data: Value| HookEvent {
        source: "cursor".into(),
        session_id: "cursor:c1".into(),
        kind: kind.into(),
        at_ms: at,
        data,
    };
    let mut evidence = HookEvidence::default();
    assert!(evidence.ingest(
        &[event(
            "session-observation",
            at,
            json!({"state":"running","turnId":"g1","startedAtMs":at,"outcome":null}),
        )],
        at,
    ));
    let state = |evidence: &HookEvidence, now: i64| {
        let mut session =
            json!({"source":"cursor","id":"cursor:c1","state":"idle","updatedAt":iso(at - 60_000)});
        evidence.overlay(&mut session, now, None, None);
        session["state"].as_str().unwrap().to_owned()
    };
    assert_eq!(state(&evidence, at + 1_000), "running");
    // A prompt without a folder keeps the destination the log gave.
    let folder = json!({"kind":"cursor-agent","id":"c1","cwd":"/p"});
    let mut session = json!({"source":"cursor","id":"cursor:c1","state":"idle","updatedAt":iso(at - 60_000),"navigationTarget":folder});
    evidence.overlay(&mut session, at + 1_000, None, None);
    assert_eq!(session["navigationTarget"], folder);
    assert!(evidence.ingest(
        &[event("completion", at + 5_000, json!({"turnId":"g1"}))],
        at + 5_000,
    ));
    assert_eq!(state(&evidence, at + 6_000), "idle");
}

#[test]
fn a_turn_that_asked_for_the_user_waits_past_its_heartbeat() {
    let at = 1_792_470_000_000i64;
    let event = |kind: &str, at: i64, data: Value| HookEvent {
        source: "trae".into(),
        session_id: "trae:c1".into(),
        kind: kind.into(),
        at_ms: at,
        data,
    };
    let running = json!({"state":"running","turnId":"c1","startedAtMs":at,"outcome":null,"title":"Fix the login"});
    let mut evidence = HookEvidence::default();
    evidence.ingest(&[event("session-observation", at, running.clone())], at);
    let asked = at + 10_000;
    evidence.ingest(
        &[event(
            "attention",
            asked,
            json!({"message":"RunCommand needs your confirmation"}),
        )],
        asked,
    );
    let row = |evidence: &HookEvidence, now: i64| {
        evidence.extra_sessions(now, 1, &HashSet::new())[0].clone()
    };
    let waiting = row(&evidence, at + PI_HEARTBEAT + 60_000);
    assert_eq!(waiting["state"], "waiting");
    assert_eq!(waiting["title"], "Fix the login");
    assert_eq!(
        waiting["liveFacts"]["attention"]["message"],
        "RunCommand needs your confirmation"
    );
    // The answer runs the tool: the turn goes on.
    let answered = at + PI_HEARTBEAT + 90_000;
    evidence.ingest(&[event("session-observation", answered, running)], answered);
    assert_eq!(row(&evidence, answered + 1_000)["state"], "running");
    // Unanswered, it is quiet once the wait is over, and that is a change due.
    let mut evidence = HookEvidence::default();
    evidence.ingest(
        &[
            event(
                "session-observation",
                at,
                json!({"state":"running","turnId":"c1","startedAtMs":at,"outcome":null}),
            ),
            event("attention", asked, json!({"message":"Allow?"})),
        ],
        asked,
    );
    assert_eq!(
        evidence.next_change(at + PI_HEARTBEAT + 1),
        Some(asked + ASKED)
    );
    assert_eq!(row(&evidence, asked + ASKED)["state"], "idle");
}

#[test]
fn a_running_observation_falls_due_when_its_heartbeat_lapses() {
    let at = 1_792_470_000_000i64;
    let mut evidence = HookEvidence::default();
    evidence.ingest(&[observation("running", at)], at);
    let due = evidence.next_change(at + 1).unwrap();
    assert_eq!(due, at + PI_HEARTBEAT);
    assert_eq!(
        evidence.clock_fingerprint(due - 1),
        evidence.clock_fingerprint(at + 1)
    );
    assert_ne!(
        evidence.clock_fingerprint(due),
        evidence.clock_fingerprint(at + 1)
    );
}

#[test]
fn pi_heartbeat_expires_at_two_minutes_and_invalidates_session_revision() {
    let at = 1_792_470_000_000i64;
    let directory = tempfile::tempdir().unwrap();
    for state in ["running", "waiting"] {
        let mut core = crate::AgentHudCore::new(vec![], directory.path().join(state));
        core.hook_events.ingest(&[observation(state, at)], at);
        let before = core
            .hook_events
            .extra_sessions(at + PI_HEARTBEAT - 1, 1, &HashSet::new());
        assert_eq!(before[0]["state"], state);
        assert!(before[0]["liveFacts"]["endedAt"].is_null());
        core.refresh_sessions_revision(at + PI_HEARTBEAT - 1);
        let revision = core.sessions_revision;

        let after = core
            .hook_events
            .extra_sessions(at + PI_HEARTBEAT, 1, &HashSet::new());
        assert_eq!(after[0]["state"], "idle");
        assert_eq!(after[0]["liveFacts"]["endedAt"], iso(at));
        assert!(after[0]["liveFacts"]["outcome"].is_null());
        core.refresh_sessions_revision(at + PI_HEARTBEAT);
        assert_ne!(core.sessions_revision, revision);

        let renewed = at + PI_HEARTBEAT;
        core.hook_events
            .ingest(&[observation(state, renewed)], renewed);
        let rows = core.hook_events.extra_sessions(renewed, 1, &HashSet::new());
        assert_eq!(rows[0]["state"], state);
        assert!(rows[0]["liveFacts"]["endedAt"].is_null());
    }
}

#[test]
fn completed_observation_keeps_its_recorded_outcome_after_heartbeat_age() {
    let at = 1_792_470_000_000i64;
    let mut event = observation("completed", at);
    event.data["outcome"] = json!("completed");
    let mut evidence = HookEvidence::default();
    evidence.ingest(&[event], at);
    let rows = evidence.extra_sessions(at + PI_HEARTBEAT, 1, &HashSet::new());
    assert_eq!(rows[0]["state"], "idle");
    assert_eq!(rows[0]["liveFacts"]["outcome"], "completed");
    assert_eq!(rows[0]["liveFacts"]["endedAt"], iso(at));
}
