use crate::transcript::Log;
use chrono::{DateTime, Utc};
use serde_json::json;

fn iso(at: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(at)
        .unwrap()
        .to_rfc3339()
}

/// The state holds until `next_change` and moves then.
fn moves_at_next_change(log: &Log, now: i64) -> i64 {
    let next = log
        .live
        .next_change(now, "claude", None)
        .expect("a change falls due");
    let state = log.live.session_state(now, "claude", None);
    assert_eq!(log.live.session_state(next - 1, "claude", None), state);
    assert_ne!(log.live.session_state(next, "claude", None), state);
    next
}

#[test]
fn a_running_turn_changes_when_it_is_abandoned() {
    let now = Utc::now().timestamp_millis();
    let mut log = Log::new("/tmp/next-change.jsonl".into(), "claude");
    log.parse(&json!({"type":"user","sessionId":"s","timestamp":iso(now - 100_000),"uuid":"u1","message":{"role":"user","content":"Fix the build"}}));
    log.parse(&json!({"type":"assistant","sessionId":"s","timestamp":iso(now - 90_000),"message":{"id":"m1","model":"claude-opus-4-5","role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}],"usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}));
    assert_eq!(log.live.session_state(now, "claude", None), "running");
    let last = log.live.last_event().unwrap();
    assert_eq!(moves_at_next_change(&log, now), last + super::ABANDONED_MS);
}

#[test]
fn a_finished_turn_has_no_change_due() {
    let now = Utc::now().timestamp_millis();
    let mut log = Log::new("/tmp/next-change-done.jsonl".into(), "claude");
    log.parse(&json!({"type":"user","sessionId":"s","timestamp":iso(now - 100_000),"uuid":"u1","message":{"role":"user","content":"Fix the build"}}));
    log.parse(&json!({"type":"assistant","sessionId":"s","timestamp":iso(now - 90_000),"message":{"id":"m1","model":"claude-opus-4-5","role":"assistant","content":[{"type":"text","text":"Done."}],"stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}));
    let state = log.live.session_state(now, "claude", None);
    match log.live.next_change(now, "claude", None) {
        Some(next) => {
            assert_eq!(log.live.session_state(next - 1, "claude", None), state);
            assert_ne!(log.live.session_state(next, "claude", None), state);
        }
        None => assert_eq!(
            log.live.session_state(now + 3_600_000, "claude", None),
            state
        ),
    }
}

#[test]
fn a_claude_session_s_last_message_is_its_last_prompt_or_words() {
    let now = Utc::now().timestamp_millis();
    let mut log = Log::new("/tmp/last-message-claude.jsonl".into(), "claude");
    log.parse(&json!({"type":"user","sessionId":"s","timestamp":iso(now - 100_000),"uuid":"u1","message":{"role":"user","content":"Fix the build"}}));
    assert_eq!(log.live.last_message_at, Some(now - 100_000));
    log.parse(&json!({"type":"assistant","sessionId":"s","timestamp":iso(now - 90_000),"message":{"id":"m1","model":"claude-opus-4-5","role":"assistant","content":[{"type":"text","text":"Looking."},{"type":"tool_use","id":"t1","name":"Bash","input":{}}],"usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}));
    assert_eq!(log.live.last_message_at, Some(now - 90_000));
    // A tool call, its result and the stop hook's line are no message.
    log.parse(&json!({"type":"assistant","sessionId":"s","timestamp":iso(now - 80_000),"message":{"id":"m2","model":"claude-opus-4-5","role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Bash","input":{}}],"usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}));
    log.parse(&json!({"type":"user","sessionId":"s","timestamp":iso(now - 70_000),"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t2","content":"ok"}]}}));
    log.parse(&json!({"type":"system","subtype":"stop_hook_summary","sessionId":"s","timestamp":iso(now - 60_000)}));
    assert_eq!(log.live.last_message_at, Some(now - 90_000));
    assert_eq!(log.live.last_event(), Some(now - 60_000));
}

#[test]
fn a_codex_session_s_last_message_is_its_last_prompt_or_words() {
    let now = Utc::now().timestamp_millis();
    let mut log = Log::new("/tmp/last-message-codex.jsonl".into(), "codex");
    log.parse(&json!({"type":"event_msg","timestamp":iso(now - 100_000),"payload":{"type":"task_started","turn_id":"t1"}}));
    log.parse(&json!({"type":"event_msg","timestamp":iso(now - 99_000),"payload":{"type":"user_message","message":"Fix the build"}}));
    assert_eq!(log.live.last_message_at, Some(now - 99_000));
    log.parse(&json!({"type":"event_msg","timestamp":iso(now - 90_000),"payload":{"type":"agent_message","message":"Looking."}}));
    log.parse(&json!({"type":"response_item","timestamp":iso(now - 80_000),"payload":{"type":"function_call","name":"shell","call_id":"c1"}}));
    log.parse(&json!({"type":"event_msg","timestamp":iso(now - 70_000),"payload":{"type":"task_complete","turn_id":"t1"}}));
    assert_eq!(log.live.last_message_at, Some(now - 90_000));
}
