use super::*;
use crate::{claude::transcript as claude, codex::transcript as codex};
use serde_json::json;

fn claude_lines() -> Vec<Value> {
    vec![
        json!({"type":"user","uuid":"u1","parentUuid":null,"promptId":"p1","timestamp":"2026-10-25T01:00:00Z","message":{"role":"user","content":"Fix the build"}}),
        json!({"type":"assistant","uuid":"a1","parentUuid":"u1","timestamp":"2026-10-25T01:00:01Z","message":{"role":"assistant","content":[{"type":"text","text":"Looking."},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}),
        json!({"type":"user","uuid":"r1","parentUuid":"a1","timestamp":"2026-10-25T01:00:02Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"Cargo.toml"}]}]}}),
        json!({"type":"user","uuid":"i1","parentUuid":"r1","timestamp":"2026-10-25T01:00:03Z","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}),
        json!({"type":"api-request-blob","message":{"role":"user","content":"Fix the build"}}),
        json!({"type":"system","uuid":"s1","content":"note"}),
    ]
}

fn codex_lines() -> Vec<Value> {
    vec![
        json!({"type":"response_item","timestamp":"2026-10-25T01:00:00Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Fix the build"}]}}),
        json!({"type":"response_item","timestamp":"2026-10-25T01:00:00Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>cwd</environment_context>"}]}}),
        json!({"type":"response_item","timestamp":"2026-10-25T01:00:00Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<turn_aborted>stopped</turn_aborted>"}]}}),
        json!({"type":"response_item","timestamp":"2026-10-25T01:00:01Z","payload":{"type":"message","role":"assistant","id":"m1","content":[{"type":"output_text","text":"Looking."}]}}),
        json!({"type":"response_item","timestamp":"2026-10-25T01:00:02Z","payload":{"type":"function_call","call_id":"c1","name":"shell","arguments":"{\"cmd\":\"ls\"}"}}),
        json!({"type":"response_item","timestamp":"2026-10-25T01:00:03Z","payload":{"type":"function_call_output","call_id":"c1","output":"Cargo.toml"}}),
        json!({"type":"response_item","timestamp":"2026-10-25T01:00:03Z","payload":{"type":"reasoning","summary":[]}}),
        json!({"type":"event_msg","timestamp":"2026-10-25T01:00:04Z","payload":{"type":"agent_message","message":"Looking."}}),
        json!({"type":"message","role":"user","content":[{"type":"input_text","text":"Older rollout"}]}),
    ]
}

#[test]
fn claude_rows_are_its_texts_calls_and_results() {
    let lines = claude_lines();
    let read = |n: usize| (claude::ITEMS.read)(&lines[n], n as u64 * 100);

    let prompt = read(0);
    assert_eq!(prompt.len(), 1);
    assert_eq!(prompt[0].id, "u1");
    assert_eq!(prompt[0].role, LineRole::User);
    assert_eq!(prompt[0].kind, LineKind::Text);
    assert_eq!(prompt[0].text.as_deref(), Some("Fix the build"));
    assert_eq!(prompt[0].timestamp.as_deref(), Some("2026-10-25T01:00:00Z"));
    assert!(prompt[0].is_prompt);

    let reply = read(1);
    assert_eq!(reply.len(), 2);
    assert_eq!(reply[0].id, "a1:0");
    assert_eq!(reply[0].parent.as_deref(), Some("u1"));
    assert_eq!(reply[0].text.as_deref(), Some("Looking."));
    assert_eq!(reply[1].kind, LineKind::Tool);
    assert_eq!(reply[1].tool_call_id.as_deref(), Some("t1"));
    let tool = reply[1].tool.as_ref().unwrap();
    assert_eq!(tool.name, "Bash");
    assert_eq!(tool.arguments, json!({"command":"ls"}));
    assert!(!reply.iter().any(|item| item.is_prompt));

    let result = read(2);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].role, LineRole::Tool);
    assert_eq!(result[0].kind, LineKind::Tool);
    assert_eq!(result[0].tool_call_id.as_deref(), Some("t1"));
    assert_eq!(result[0].text.as_deref(), Some("Cargo.toml"));

    let interruption = read(3);
    assert_eq!(interruption.len(), 1);
    assert_eq!(interruption[0].role, LineRole::User);
    assert!(!interruption[0].is_prompt);

    assert!(read(4).is_empty(), "a request copy shows nothing");
    assert!(read(5).is_empty());
}

#[test]
fn codex_rows_are_its_response_items() {
    let lines = codex_lines();
    let read = |n: usize| (codex::ITEMS.read)(&lines[n], n as u64 * 100);

    let prompt = read(0);
    assert_eq!(prompt.len(), 1);
    assert_eq!(prompt[0].id, "o0");
    assert_eq!(prompt[0].role, LineRole::User);
    assert_eq!(prompt[0].text.as_deref(), Some("Fix the build"));
    assert!(prompt[0].is_prompt);

    assert!(!read(1)[0].is_prompt, "context is not the user's words");
    assert!(!read(2)[0].is_prompt, "an interruption note is no prompt");

    let reply = read(3);
    assert_eq!(reply[0].id, "m1");
    assert_eq!(reply[0].role, LineRole::Assistant);
    assert_eq!(reply[0].kind, LineKind::Text);

    let call = read(4);
    assert_eq!(call[0].kind, LineKind::Tool);
    assert_eq!(call[0].tool_call_id.as_deref(), Some("c1"));
    let tool = call[0].tool.as_ref().unwrap();
    assert_eq!(tool.name, "shell");
    assert_eq!(tool.arguments, json!("{\"cmd\":\"ls\"}"));

    let result = read(5);
    assert_eq!(result[0].role, LineRole::Tool);
    assert_eq!(result[0].tool_call_id.as_deref(), Some("c1"));
    assert_eq!(result[0].text.as_deref(), Some("Cargo.toml"));

    assert!(read(6).is_empty());
    assert!(read(7).is_empty(), "events show nothing");
    let older = read(8);
    assert_eq!(older[0].id, "o800:0");
    assert!(older[0].is_prompt);
}

#[test]
fn a_request_copy_shows_nothing_to_any_rule() {
    let copy = json!({"type":"api-request","message":{"role":"user","content":"Hi"}});
    assert!((SHARED.read)(&copy, 0).is_empty());
    assert!((claude::ITEMS.read)(&copy, 0).is_empty());
    assert!((codex::ITEMS.read)(&copy, 0).is_empty());
}

#[test]
fn the_shared_rule_reads_each_clients_lines_as_its_own_rule_does() {
    for (rule, lines) in [
        (&claude::ITEMS, claude_lines()),
        (&codex::ITEMS, codex_lines()),
    ] {
        for (n, line) in lines.iter().enumerate() {
            let offset = n as u64 * 100;
            assert_eq!(
                (rule.read)(line, offset),
                (SHARED.read)(line, offset),
                "{line}"
            );
        }
    }
    assert!(std::ptr::eq(of("claude"), &claude::ITEMS));
    assert!(std::ptr::eq(of("codex"), &codex::ITEMS));
    assert!(std::ptr::eq(of("pi"), &SHARED));
}
