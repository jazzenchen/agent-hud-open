//! CodeBuddy's and WorkBuddy's hooks. Both ask for permission; CodeBuddy also
//! reports completed turns, whose final message comes from its transcript.
use crate::hooks::{stop, Completed, Completion, Hooks, Layout, Permission, Reply, POSIX_ONLY};
use crate::HookEvent;
use crate::Location;
use serde_json::{json, Value};

pub static CODEBUDDY: Hooks = Hooks {
    settings: Some(Location {
        not_if: Some(".workbuddy"),
        ..Location::home(&["CODEBUDDY_CONFIG_DIR"], &[".codebuddy"], "settings.json")
    }),
    aliases: &[],
    environment: &[],
    layout: Layout::Groups,
    permission: Some(Permission {
        timeout: 86_400,
        unanswerable: &["AskUserQuestion", "ExitPlanMode"],
        rules: false,
        plans: false,
        reviewer_gate: false,
        settles_in_transcript: false,
        unattended: None,
        policy: None,
        opt_in: false,
        reply: Reply::HookSpecificOutput,
    }),
    completion: Some(Completion {
        begin: None,
        event: "Stop",
        timeout: 5,
        read: completed,
        enrich: Some(enrich),
        reply: None,
    }),
    origin: None,
    attention: false,
    compaction: None,
    prefixed_sessions: true,
    launch: POSIX_ONLY,
};

pub static WORKBUDDY: Hooks = Hooks {
    settings: Some(Location::home(&[], &[".workbuddy"], "settings.json")),
    aliases: &[],
    environment: &[],
    layout: Layout::Groups,
    permission: Some(Permission {
        timeout: 86_400,
        unanswerable: &["AskUserQuestion", "ExitPlanMode"],
        rules: false,
        plans: false,
        reviewer_gate: false,
        settles_in_transcript: false,
        unattended: None,
        policy: None,
        opt_in: false,
        reply: Reply::HookSpecificOutput,
    }),
    completion: None,
    origin: None,
    attention: false,
    compaction: None,
    prefixed_sessions: true,
    launch: POSIX_ONLY,
};

/// CodeBuddy's `Stop`. Its turn is the callback's time, and the transcript
/// path lets the platform read the final message.
fn completed(payload: &Value, now_ms: i64) -> Option<Completed> {
    stop(payload, now_ms, false).map(|mut done| {
        done.transcript_path = Some(json!(payload["transcript_path"].as_str()));
        done
    })
}

/// CodeBuddy's Stop payload omits its final message identity. The platform may
/// supply at most the last MiB of a verified transcript; the schema stays here.
/// A newer user message is a boundary, not permission to reuse an older reply.
fn enrich(event: &mut HookEvent, transcript_tail: &[u8]) -> bool {
    if event.kind != "completion" {
        return false;
    }
    let Some(session) = event.session_id.strip_prefix("codebuddy:") else {
        return false;
    };
    let tail = &transcript_tail[transcript_tail.len().saturating_sub(1024 * 1024)..];
    for line in tail.split(|byte| *byte == b'\n').rev() {
        let Ok(record) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if record["type"] != "message"
            || record["sessionId"].as_str().is_some_and(|id| id != session)
        {
            continue;
        }
        if record["role"] == "user" {
            return false;
        }
        if record["role"] != "assistant" || record["status"] != "completed" {
            continue;
        }
        let nonempty = |value: &Value| {
            value
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
        };
        let Some(id) = nonempty(&record["providerData"]["messageId"]) else {
            return false;
        };
        let model = nonempty(&record["providerData"]["model"])
            .or_else(|| nonempty(&record["providerData"]["requestModelId"]))
            .or_else(|| nonempty(&record["message"]["model"]));
        event.data["turnId"] = serde_json::json!(id);
        event.data["model"] = serde_json::json!(model);
        return true;
    }
    false
}
