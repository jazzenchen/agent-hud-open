//! Cursor's completion hook, in its version-1 settings.
use crate::hooks::{Began, Begin, Completed, Completion, Handler, Hooks, Layout, POSIX_ONLY};
use crate::text;
use crate::{HookEvent, Location, Usage};
use serde_json::Value;

pub static HOOKS: Hooks = Hooks {
    settings: Some(Location::home(&[], &[".cursor"], "hooks.json")),
    aliases: &[],
    environment: &[],
    layout: Layout::Handlers(Handler {
        command_key: "command",
        typed: false,
        timeout_key: "timeout",
    }),
    permission: None,
    completion: Some(Completion {
        event: "stop",
        timeout: 5,
        read: completed,
        enrich: Some(reply),
        // Cursor lets a prompt through unless the hook says `continue: false`;
        // say so rather than rely on an empty answer.
        reply: Some(r#"{"continue":true}"#),
        begin: Some(Begin {
            events: &["beforeSubmitPrompt", "postToolUse", "afterAgentResponse"],
            read: began,
        }),
    }),
    origin: None,
    attention: false,
    compaction: None,
    rewake: None,
    prefixed_sessions: true,
    launch: POSIX_ONLY,
};

/// A generation under way: the prompt that starts it, and the tool calls and
/// replies that show it still going.
fn began(payload: &Value) -> Option<Began> {
    Some(Began {
        session: text(&payload["conversation_id"])?.into(),
        turn: text(&payload["generation_id"])?.into(),
        workspace: payload["workspace_roots"][0].as_str().map(str::to_owned),
        waiting: false,
    })
}

/// The reply of the turn that ended, from the tail of the transcript its
/// `stop` names (`agent-transcripts/<id>/<id>.jsonl`: a `role` and
/// `message.content` parts per line, `turn_ended` between turns), since the
/// payload carries none. Only text after the turn's prompt counts: a tail
/// whose last prompt has no reply yet gives nothing, never an older turn's.
fn reply(event: &mut HookEvent, transcript_tail: &[u8]) -> bool {
    if event.kind != "completion" || !event.data["message"].is_null() {
        return false;
    }
    let mut parts = Vec::new();
    for line in transcript_tail.split(|byte| *byte == b'\n').rev() {
        let Ok(record) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        match record["role"].as_str() {
            Some("user") => break,
            Some("assistant") => {
                let text = record["message"]["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|part| part["type"] == "text")
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("");
                if !text.trim().is_empty() {
                    parts.push(text);
                }
            }
            _ => {}
        }
    }
    let Some(last) = parts.into_iter().next() else {
        return false;
    };
    event.data["message"] = serde_json::json!(last);
    true
}

/// A completed generation: the conversation and the generation that ended.
fn completed(payload: &Value, _now_ms: i64) -> Option<Completed> {
    if payload["hook_event_name"] != "stop" || payload["status"] != "completed" {
        return None;
    }
    let (Some(id), Some(turn)) = (
        text(&payload["conversation_id"]),
        text(&payload["generation_id"]),
    ) else {
        return None;
    };
    Some(Completed {
        session: id.into(),
        turn: turn.into(),
        workspace: payload["workspace_roots"][0].as_str().map(str::to_owned),
        model: payload["model_id"]
            .as_str()
            .or(payload["model"].as_str())
            .map(str::to_owned),
        transcript_path: None,
        usage: usage(payload),
    })
}

/// The generation's tokens, which Cursor 3.27 and later put in the `stop`
/// payload and keeps nowhere else on disk (its messages store 0/0).
/// `input_tokens` counts every prompt token, cache reads included: a turn of
/// about twenty calls whose context ended near 63K reported 740,409 input
/// with 639,616 of them read from the cache, so the fresh input is the
/// difference. Cache writes are taken as reported.
fn usage(payload: &Value) -> Option<Usage> {
    let count = |key: &str| payload[key].as_u64();
    let input = count("input_tokens")?;
    let output = count("output_tokens")?;
    let read = count("cache_read_tokens").unwrap_or(0);
    let write = count("cache_write_tokens").unwrap_or(0);
    if read > input {
        return None;
    }
    let usage = Usage {
        input: input - read,
        output,
        read,
        write,
        reasoning: 0,
    };
    (usage.total() > 0).then_some(usage)
}

#[cfg(test)]
mod reply_tests {
    use super::reply;
    use crate::HookEvent;
    use serde_json::json;

    fn event() -> HookEvent {
        HookEvent {
            source: "cursor".into(),
            session_id: "cursor:8729".into(),
            at_ms: 0,
            kind: "completion".into(),
            data: json!({"turnId":"g2"}),
        }
    }

    #[test]
    fn the_reply_comes_from_the_turns_own_lines_in_the_transcript() {
        let user = |t: &str| {
            json!({"role":"user","message":{"content":[{"type":"text","text":t}]}}).to_string()
        };
        let assistant = |t: &str| {
            json!({"role":"assistant","message":{"content":[{"type":"text","text":t}]}}).to_string()
        };
        let ended = json!({"type":"turn_ended","status":"success"}).to_string();
        let tail = [
            user("one"),
            assistant("ONE"),
            ended.clone(),
            user("two"),
            assistant("Looking."),
            assistant("TWO"),
            ended,
        ]
        .join("\n");
        let mut done = event();
        assert!(reply(&mut done, tail.as_bytes()));
        assert_eq!(done.data["message"], "TWO");
        // The turn's prompt is there but not its reply yet: nothing, not "ONE".
        let early = [user("one"), assistant("ONE"), user("two")].join("\n");
        let mut waiting = event();
        assert!(!reply(&mut waiting, early.as_bytes()));
        assert!(waiting.data["message"].is_null());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stop(extra: Value) -> Value {
        let mut payload = json!({
            "conversation_id": "ca086626-b368-46b5-8492-2c8b493f7e7a",
            "generation_id": "f89c40c7-ff67-435b-9cf3-a395582dd7e6",
            "model": "grok-4.7-high-fast",
            "model_id": "grok-4.7",
            "status": "completed",
            "hook_event_name": "stop",
            "workspace_roots": ["/Users/me/project"],
        });
        payload
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        payload
    }

    #[test]
    fn a_stop_without_tokens_or_with_impossible_ones_reports_none() {
        assert!(completed(&stop(json!({})), 0).unwrap().usage.is_none());
        assert!(completed(
            &stop(json!({"input_tokens": 10, "output_tokens": 1, "cache_read_tokens": 11})),
            0
        )
        .unwrap()
        .usage
        .is_none());
        assert!(
            completed(&stop(json!({"input_tokens": 0, "output_tokens": 0})), 0)
                .unwrap()
                .usage
                .is_none()
        );
    }

    #[test]
    fn a_prompt_or_tool_call_reports_a_running_turn_and_other_events_nothing() {
        let event = |name: &str| {
            crate::hooks::events::parse_event(
                "cursor",
                "--completion-hook",
                &stop(json!({"hook_event_name": name})),
                1_000,
            )
            .unwrap()
        };
        for name in ["beforeSubmitPrompt", "postToolUse", "afterAgentResponse"] {
            let event = event(name).unwrap();
            assert_eq!(event.kind, "session-observation");
            assert_eq!(
                event.session_id,
                "cursor:ca086626-b368-46b5-8492-2c8b493f7e7a"
            );
            assert_eq!(event.data["state"], "running");
            assert_eq!(
                event.data["navigationTarget"],
                json!({"kind":"cursor-agent","id":"ca086626-b368-46b5-8492-2c8b493f7e7a","cwd":"/Users/me/project"})
            );
        }
        assert_eq!(event("stop").unwrap().kind, "completion");
        assert!(event("sessionStart").is_none());
        let events = crate::hooks::configuration::configuration("cursor", "--completion-hook")
            .unwrap()
            .events;
        assert_eq!(
            events,
            [
                "stop",
                "beforeSubmitPrompt",
                "postToolUse",
                "afterAgentResponse"
            ]
        );
    }
}
