//! Antigravity's completion and session-origin hooks, kept in Agent HUD's own
//! `agent-hud` entry. Its permissions are observed natively (`permissions.rs`),
//! not through hooks.
use crate::hooks::{Completed, Completion, Hooks, Layout, POSIX_ONLY};
use crate::{text, HookEvent, Location};
use serde_json::{json, Value};

pub static HOOKS: Hooks = Hooks {
    settings: Some(Location::home(
        &["GEMINI_CLI_HOME"],
        &[".gemini"],
        "config/hooks.json",
    )),
    aliases: &[],
    environment: &[],
    layout: Layout::OwnedEntry,
    permission: None,
    completion: Some(Completion {
        begin: None,
        event: "Stop",
        timeout: 5,
        read: completed,
        enrich: Some(enrich),
        reply: Some(r#"{"decision":"stop"}"#),
    }),
    origin: Some(super::process::origin),
    attention: false,
    compaction: None,
    prefixed_sessions: true,
    launch: POSIX_ONLY,
};

/// A conversation that stopped without a tool call, fully idle and without
/// an error. Its turn is the callback's time.
fn completed(payload: &Value, now_ms: i64) -> Option<Completed> {
    if payload["terminationReason"] != "NO_TOOL_CALL"
        || payload["fullyIdle"] != true
        || payload["error"].as_str().is_some_and(|s| !s.is_empty())
    {
        return None;
    }
    let id = text(&payload["conversationId"])?;
    Some(Completed {
        session: id.to_owned(),
        turn: format!("stop-{now_ms}"),
        workspace: payload["workspacePaths"][0].as_str().map(str::to_owned),
        model: payload["modelName"].as_str().map(str::to_owned),
        transcript_path: text(&payload["transcriptPath"]).map(|s| json!(s)),
        usage: None,
    })
}

/// Antigravity's `Stop` payload omits its final assistant message. When the
/// platform reads the tail of `transcript.jsonl`, fill in the latest completed
/// `PLANNER_RESPONSE` step's message and step id.
fn enrich(event: &mut HookEvent, transcript_tail: &[u8]) -> bool {
    if event.kind != "completion" || !event.session_id.starts_with("antigravity:") {
        return false;
    }
    let tail = &transcript_tail[transcript_tail.len().saturating_sub(1024 * 1024)..];
    for line in tail.split(|byte| *byte == b'\n').rev() {
        let Ok(record) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if record["type"] == "USER_INPUT" && record["source"] == "USER_EXPLICIT" {
            return false;
        }
        if record["type"] != "PLANNER_RESPONSE" || record["status"] != "DONE" {
            continue;
        }
        let Some(content) = record["content"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        if let Some(idx) = record["step_index"].as_u64() {
            event.data["turnId"] = json!(format!("step-{idx}"));
        }
        event.data["message"] = json!(content.chars().take(4096).collect::<String>());
        return true;
    }
    false
}
