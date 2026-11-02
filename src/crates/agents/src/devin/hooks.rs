//! Devin's hooks: permission requests, answered with a bare decision, and
//! completed turns. Its questions and plans stay with the client.
use crate::hooks::{stop, Completed, Completion, Hooks, Layout, Permission, Reply, POSIX_ONLY};
use crate::Location;
use serde_json::Value;

pub static HOOKS: Hooks = Hooks {
    settings: Some(Location::home(&[], &[".config/devin"], "config.json")),
    aliases: &[],
    layout: Layout::Groups,
    permission: Some(Permission {
        timeout: 86_400,
        unanswerable: &["ask_user_question", "exit_plan_mode"],
        rules: false,
        plans: false,
        reviewer_gate: false,
        settles_in_transcript: false,
        unattended: None,
        reply: Reply::Decision,
    }),
    completion: Some(Completion {
        begin: None,
        event: "Stop",
        timeout: 10,
        read: completed,
        enrich: None,
        reply: None,
    }),
    origin: None,
    attention: false,
    compaction: None,
    prefixed_sessions: true,
    launch: POSIX_ONLY,
};

/// Devin's `Stop`: the turn is the callback's prompt.
fn completed(payload: &Value, now_ms: i64) -> Option<Completed> {
    stop(payload, now_ms, true)
}
