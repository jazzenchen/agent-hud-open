//! Devin's hooks: permission requests, answered with a bare decision,
//! completed turns, and where a session runs. Its questions and plans stay
//! with the client. Devin Desktop's Devin Local is the same CLI, reading the
//! same settings file, so these hooks cover both.
use crate::hooks::{stop, Completed, Completion, Hooks, Layout, Permission, Reply, POSIX_ONLY};
use crate::process::Imported;
use crate::Location;
use serde_json::Value;

pub static HOOKS: Hooks = Hooks {
    settings: Some(Location::home(&[], &[".config/devin"], "config.json")),
    aliases: &[],
    environment: &[],
    layout: Layout::Groups,
    permission: Some(Permission {
        timeout: 86_400,
        unanswerable: &["ask_user_question", "exit_plan_mode"],
        rules: false,
        plans: false,
        reviewer_gate: false,
        settles_in_transcript: false,
        unattended: None,
        policy: None,
        opt_in: false,
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
    origin: Some(super::process::origin),
    attention: false,
    compaction: None,
    rewake: None,
    prefixed_sessions: true,
    launch: POSIX_ONLY,
};

/// Devin runs Claude Code's hooks too, from its default import of Claude
/// Code's settings, with its own payloads (`process::runs_imported`). Its own
/// hooks above are the ones that count.
pub static IMPORTS: Imported = Imported {
    by: "devin",
    from: &["claude"],
    rule: super::process::runs_imported,
};

/// Devin's `Stop`: the turn is the callback's prompt.
fn completed(payload: &Value, now_ms: i64) -> Option<Completed> {
    stop(payload, now_ms, true)
}
