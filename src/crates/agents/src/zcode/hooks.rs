//! ZCode's permission and completion hooks, in its nested settings layout.
//! Its plans stay with the client.
use crate::hooks::{
    stop, Began, Begin, Completed, Completion, Hooks, Layout, Permission, Reply, POSIX_ONLY,
};
use crate::{text, Location};
use serde_json::Value;

pub static HOOKS: Hooks = Hooks {
    settings: Some(Location::home(&[], &[".zcode/cli"], "config.json")),
    aliases: &[],
    environment: &[],
    layout: Layout::NestedGroups,
    permission: Some(Permission {
        timeout: 86_400,
        unanswerable: &["ExitPlanMode"],
        rules: true,
        plans: false,
        reviewer_gate: false,
        settles_in_transcript: false,
        unattended: None,
        policy: None,
        opt_in: false,
        reply: Reply::HookSpecificOutput,
    }),
    completion: Some(Completion {
        event: "Stop",
        timeout: 5,
        read: completed,
        enrich: None,
        reply: None,
        begin: Some(Begin {
            events: &["UserPromptSubmit", "PostToolUse"],
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

fn began(payload: &Value) -> Option<Began> {
    let session = text(&payload["session_id"])?;
    Some(Began {
        session: session.into(),
        turn: session.into(),
        workspace: text(&payload["cwd"]).map(str::to_owned),
        waiting: false,
    })
}

fn completed(payload: &Value, now_ms: i64) -> Option<Completed> {
    stop(payload, now_ms, false)
}
