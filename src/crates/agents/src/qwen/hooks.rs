//! Qwen Code's hooks: permission requests and completed turns. It counts
//! timeouts in milliseconds; its questions and plans stay with the client.
use crate::hooks::{stop, Completed, Completion, Hooks, Layout, Permission, Reply, POSIX_ONLY};
use crate::Location;
use serde_json::Value;

pub static HOOKS: Hooks = Hooks {
    settings: Some(Location::home(&["QWEN_HOME"], &[".qwen"], "settings.json")),
    aliases: &[],
    layout: Layout::Groups,
    permission: Some(Permission {
        timeout: 86_400_000,
        unanswerable: &["ask_user_question", "exit_plan_mode"],
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
        timeout: 5000,
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

/// Qwen Code's `Stop`: the turn is the callback's prompt.
fn completed(payload: &Value, now_ms: i64) -> Option<Completed> {
    stop(payload, now_ms, true)
}
