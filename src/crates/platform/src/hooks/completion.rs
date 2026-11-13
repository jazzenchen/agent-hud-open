use agent_hud_agents::HookEvent;
use serde_json::{json, Value};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::SystemTime,
};

/// How a client's stop hook carries its session on with a queued message:
/// the reply that does it, and how many times in a row Agent HUD asks.
struct Carry {
    reply: fn(&str) -> Value,
    /// Continuations in one turn of the user's, at most. Kept under the
    /// client's own cap, so a claimed message is never one it then drops.
    most: u64,
}

fn block(text: &str) -> Value {
    json!({"decision":"block","reason":text})
}

fn carry(source: &str) -> Option<Carry> {
    Some(match source {
        // `followup_message` is sent as the next user message
        // (cursor.com/docs/hooks). Cursor runs the hook only while
        // `loop_count` is under `loop_limit` (5 by default): the fifth is
        // left to the hook that reports the last continuation's end.
        "cursor" => Carry {
            reply: |text| json!({"followup_message":text}),
            most: 4,
        },
        // The reason "will be used as a new user query"; 5 per query by
        // default (docs.trae.ai/ide/hook-configuration-reference).
        "trae" | "trae-cn" => Carry {
            reply: block,
            most: 4,
        },
        // ZCode carries a turn on three times at most.
        "zcode" => Carry {
            reply: block,
            most: 3,
        },
        // The engine turns the reason into "Stop hook feedback"; it sets no
        // cap of its own, and Agent HUD adds none: what ends a chain is the
        // limit on passes between agents (`agentMessagesLimit`).
        "workbuddy" | "codebuddy" => Carry {
            reply: block,
            most: u64::MAX,
        },
        // Qoder documents `decision: "deny"` with a reason for Stop.
        "qoder" | "qoder-cn" | "qoder-work" => Carry {
            reply: |text| json!({"decision":"deny","reason":text}),
            most: 3,
        },
        // Kimi appends the reason as a user message and runs its Stop hook
        // only while the turn has not been carried on yet.
        "kimi" => Carry {
            reply: |text| json!({"hookSpecificOutput":{"permissionDecision":"deny","permissionDecisionReason":text}}),
            most: 1,
        },
        _ => return None,
    })
}

/// Text a hook record keeps of a prompt or a reply. A record holds at most
/// 64 KiB (`inbox`), and one that is refused loses the session's state with
/// it, so a long text keeps its start.
pub(super) fn bounded(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// When the session's completion hook fires, hands it the oldest message
/// queued for it (`delivery`, the hook channel) as the client's
/// continuation reply, and notes the session running again with that
/// message's id. Nothing is claimed when the turn was already carried on as
/// often as the client allows: the message waits for the next stop.
pub(super) fn claim_prompt(data: &Path, event: &HookEvent) -> Option<Value> {
    if event.kind != "completion" {
        return None;
    }
    let provider = crate::delivery::Provider::parse(&event.source)?;
    if !provider.uses_hook_channel() {
        return None;
    }
    let carry = carry(&event.source)?;
    let key = crate::delivery::hook_session_key(provider, &event.session_id)?;
    let folder = data.join("hook-channel");
    let chain = folder.join("chains").join(format!("{key}.json"));
    let carried = carried(&chain, event);
    let claimed = (carried < carry.most)
        .then(|| {
            crate::claude_plugin::ClaudeChannel::new(folder.clone())
                .take_next(&key, SystemTime::now())
                .ok()
                .flatten()
        })
        .flatten();
    let Some((id, text)) = claimed else {
        // The next stop ends a turn the user began.
        let _ = std::fs::remove_file(&chain);
        return None;
    };
    let _ = crate::delivery::atomic_json(
        &chain,
        &json!({"count":carried + 1,"at":event.at_ms,"source":event.source}),
    );
    note_delivered(
        data,
        (&event.source, &event.session_id),
        event.at_ms.saturating_add(1),
        &id,
        &text,
    );
    Some((carry.reply)(&text))
}

/// Notes the session (`source`, its id as Agent HUD keeps it) running again
/// from `at_ms` with the queued message `id` as its prompt, once a hook
/// handed the message over.
pub(super) fn note_delivered(
    data: &Path,
    (source, session): (&str, &str),
    at_ms: i64,
    id: &str,
    text: &str,
) {
    let followup = HookEvent {
        kind: "session-observation".into(),
        source: source.into(),
        session_id: session.into(),
        at_ms,
        data: json!({
            "state":"running",
            "startedAtMs":at_ms,
            "prompt":bounded(text, PROMPT_LIMIT),
            "delivered":id,
        }),
    };
    let _ = super::record_event(data, &followup);
}

/// The prompt a hook record keeps, and the reply a completion keeps.
pub(super) const PROMPT_LIMIT: usize = 8 * 1024;
pub(super) const MESSAGE_LIMIT: usize = 40 * 1024;

/// How many times this turn was already carried on from a stop hook: the
/// client's own count where it gives one, else Agent HUD's chain, which a
/// stop that says it follows no stop hook ends.
fn carried(chain: &Path, event: &HookEvent) -> u64 {
    if let Some(count) = event.data["loopCount"].as_u64() {
        return count;
    }
    if event.source == "kimi" || event.data["stopHookActive"] == false {
        return 0;
    }
    let recorded = std::fs::read(chain)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|chain| {
            chain["at"]
                .as_i64()
                .is_some_and(|at| event.at_ms.saturating_sub(at) < 6 * 3_600_000)
        })
        .and_then(|chain| chain["count"].as_u64());
    match recorded {
        Some(count) => count,
        // Carried on by a stop hook of someone else's: at least once.
        None if event.data["stopHookActive"] == true => 1,
        None => 0,
    }
}

/// Keeps a record's prompt and reply text within what a hook record holds.
pub(super) fn bound(event: &mut HookEvent) {
    for (key, limit) in [("prompt", PROMPT_LIMIT), ("message", MESSAGE_LIMIT)] {
        if let Some(text) = event.data[key].as_str().filter(|text| text.len() > limit) {
            event.data[key] = json!(bounded(text, limit));
        }
    }
}

/// The platform supplies only bounded local bytes; the client's descriptor
/// owns its final-message schema and completion identity (CodeBuddy's today).
pub(super) fn enrich(event: &mut HookEvent, payload: &Value) {
    if event.kind == "session-observation" {
        if let Some(prompt) = payload["prompt"].as_str().filter(|s| !s.trim().is_empty()) {
            event.data["prompt"] = json!(prompt);
        }
        if let Some(text) = payload["text"].as_str().filter(|s| !s.trim().is_empty()) {
            event.data["message"] = json!(text);
        }
        return;
    }
    if event.kind != "completion" {
        return;
    }
    if event.data["message"].is_null() {
        if let Some(msg) = payload["last_assistant_message"]
            .as_str()
            .or_else(|| payload["message"].as_str())
            .or_else(|| payload["text"].as_str())
            .filter(|s| !s.trim().is_empty())
        {
            event.data["message"] = json!(msg);
        }
    }
    let enriches = agent_hud_agents::hooks::hooks_by_id(&event.source)
        .and_then(|hooks| hooks.completion.as_ref())
        .is_some_and(|completion| completion.enrich.is_some());
    if !enriches {
        return;
    }
    let Some(path) = payload["transcript_path"]
        .as_str()
        .or_else(|| payload["transcriptPath"].as_str())
    else {
        return;
    };
    let Ok(mut file) = File::open(path) else {
        return;
    };
    let Ok(metadata) = file.metadata() else {
        return;
    };
    if !metadata.is_file() {
        return;
    }
    if !crate::os::owned_by_user(std::path::Path::new(path), &metadata) {
        return;
    }
    let start = metadata.len().saturating_sub(1024 * 1024);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return;
    }
    let mut bytes = Vec::new();
    if file.take(1024 * 1024).read_to_end(&mut bytes).is_err() {
        return;
    }
    let tail = if start > 0 {
        bytes
            .iter()
            .position(|b| *b == b'\n')
            .map(|end| &bytes[end + 1..])
            .unwrap_or(&[])
    } else {
        &bytes
    };
    agent_hud_agents::enrich_codebuddy_completion(event, tail);
}
