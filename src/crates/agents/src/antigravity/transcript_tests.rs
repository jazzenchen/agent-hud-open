use super::*;
use crate::Log;
use serde_json::json;
use std::path::PathBuf;

#[test]
fn cleans_user_request_tags_and_metadata() {
    let raw = "<USER_REQUEST>\nFix the tests\n</USER_REQUEST>\n<ADDITIONAL_METADATA>\nnow\n</ADDITIONAL_METADATA>";
    assert_eq!(clean_user_content(raw), "Fix the tests");
    let without_wrapper = "Hello world\n<ADDITIONAL_METADATA>\nnow\n</ADDITIONAL_METADATA>";
    assert_eq!(clean_user_content(without_wrapper), "Hello world");
}

#[test]
fn reduces_transcript_lifecycle_tools_and_items() {
    let path = PathBuf::from(
            "/Users/test/.gemini/antigravity/brain/3e144c9b-bb25-4894-af49-508ecf3540c3/.system_generated/logs/transcript.jsonl",
        );
    let mut log = Log::new(path, "antigravity");
    let step0 = json!({
        "step_index": 0,
        "source": "USER_EXPLICIT",
        "type": "USER_INPUT",
        "status": "DONE",
        "created_at": "2026-10-26T05:58:56Z",
        "content": "<USER_REQUEST>\nInspect the workspace\n</USER_REQUEST>"
    });
    log.parse(&step0);
    assert_eq!(log.id, "antigravity:3e144c9b-bb25-4894-af49-508ecf3540c3");
    assert_eq!(log.client.as_deref(), Some("Desktop"));
    assert!(log.live.turns.back().unwrap().running);

    let step1 = json!({
        "step_index": 1,
        "source": "MODEL",
        "type": "PLANNER_RESPONSE",
        "status": "DONE",
        "created_at": "2026-10-26T05:59:00Z",
        "input_tokens": 1000,
        "cache_read_tokens": 500,
        "output_tokens": 120,
        "thinking": "Reading files first.",
        "tool_calls": [{"name": "read_file", "args": {"Path": "/tmp/a"}}]
    });
    log.parse(&step1);
    assert!(log.live.turns.back().unwrap().running);
    assert_eq!(log.usage.input, 1000);
    assert_eq!(log.usage.read, 500);
    assert_eq!(log.usage.output, 120);

    let step2 = json!({
        "step_index": 2,
        "source": "MODEL",
        "type": "PLANNER_RESPONSE",
        "status": "DONE",
        "created_at": "2026-10-26T05:59:10Z",
        "input_tokens": 1200,
        "cache_read_tokens": 1000,
        "output_tokens": 80,
        "content": "All done."
    });
    log.parse(&step2);
    let turn = log.live.turns.back().unwrap();
    assert!(!turn.running);
    assert_eq!(turn.outcome.as_deref(), Some("completed"));
    assert_eq!(turn.message.as_deref(), Some("All done."));

    let items0 = (ITEMS.read)(&step0, 0);
    assert_eq!(items0.len(), 1);
    assert!(items0[0].is_prompt);
    assert_eq!(items0[0].text.as_deref(), Some("Inspect the workspace"));

    let items1 = (ITEMS.read)(&step1, 100);
    assert_eq!(items1.len(), 2);
    assert_eq!(items1[0].kind, LineKind::Thinking);
    assert_eq!(items1[1].kind, LineKind::Tool);
}
