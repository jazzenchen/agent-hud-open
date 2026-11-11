//! CodeBuddy's and WorkBuddy's hooks. Both ask for permission; CodeBuddy also
//! reports completed turns, whose final message comes from its transcript.
use crate::hooks::{
    stop, Began, Begin, Completed, Completion, Hooks, Layout, Permission, Reply, POSIX_ONLY,
};
use crate::HookEvent;
use crate::{text, Location};
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
        begin: Some(Begin {
            events: &["UserPromptSubmit"],
            read: began,
        }),
        event: "Stop",
        timeout: 5,
        read: completed,
        enrich: Some(enrich),
        reply: None,
    }),
    origin: None,
    attention: false,
    compaction: None,
    rewake: None,
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
    completion: Some(Completion {
        begin: Some(Begin {
            events: &["UserPromptSubmit"],
            read: began,
        }),
        event: "Stop",
        timeout: 5,
        read: completed,
        enrich: Some(enrich),
        reply: None,
    }),
    origin: None,
    attention: false,
    compaction: None,
    rewake: None,
    prefixed_sessions: true,
    launch: POSIX_ONLY,
};

/// A prompt starts a turn. WorkBuddy runs one engine per turn and its MCP
/// servers in the app's own process, so this is also how Agent HUD learns,
/// from a session's first prompt, which session an MCP call comes from.
fn began(payload: &Value) -> Option<Began> {
    if payload["hook_event_name"] != "UserPromptSubmit" {
        return None;
    }
    let session = text(&payload["session_id"])?;
    Some(Began {
        session: session.into(),
        turn: session.into(),
        workspace: text(&payload["cwd"]).map(str::to_owned),
        waiting: false,
    })
}

/// CodeBuddy's and WorkBuddy's `Stop`. Its turn is the callback's time, and
/// the transcript path lets the platform read the final message.
fn completed(payload: &Value, now_ms: i64) -> Option<Completed> {
    stop(payload, now_ms, false).map(|mut done| {
        done.transcript_path = Some(json!(payload["transcript_path"].as_str()));
        done
    })
}

/// CodeBuddy's and WorkBuddy's Stop payload omits its final message identity.
/// The platform may supply at most the last MiB of a verified transcript; the
/// schema stays here. A newer user message is a boundary, not permission to
/// reuse an older reply.
fn enrich(event: &mut HookEvent, transcript_tail: &[u8]) -> bool {
    if event.kind != "completion" {
        return false;
    }
    let Some(session) = event
        .session_id
        .strip_prefix("codebuddy:")
        .or_else(|| event.session_id.strip_prefix("workbuddy:"))
    else {
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

#[cfg(test)]
mod tests {
    use crate::parse_event;
    use serde_json::json;

    #[test]
    fn a_prompt_runs_a_workbuddy_turn_and_a_stop_ends_it() {
        let now = 1_794_381_287_063;
        for source in ["workbuddy", "codebuddy"] {
            let prompt = json!({"hook_event_name":"UserPromptSubmit","session_id":"8a08fa98","prompt":"hi","cwd":"/w","transcript_path":"/t.jsonl"});
            let event = parse_event(source, "--completion-hook", &prompt, now)
                .unwrap()
                .unwrap();
            assert_eq!(event.kind, "session-observation");
            assert_eq!(event.session_id, format!("{source}:8a08fa98"));
            assert_eq!(event.data["state"], "running");
            let stop = json!({"hook_event_name":"Stop","session_id":"8a08fa98","stop_hook_active":false,"cwd":"/w","transcript_path":"/t.jsonl"});
            let event = parse_event(source, "--completion-hook", &stop, now)
                .unwrap()
                .unwrap();
            assert_eq!(event.kind, "completion");
            assert_eq!(event.session_id, format!("{source}:8a08fa98"));
        }
    }
}
