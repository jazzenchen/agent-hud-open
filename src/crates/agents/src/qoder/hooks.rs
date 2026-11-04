//! The Qoder builds' permission hooks. They apply permanent rules; their
//! questions and plans stay with the client.
use crate::hooks::{Hooks, Layout, Permission, Reply, POSIX_ONLY};
use crate::Location;

pub static QODER: Hooks = Hooks {
    settings: Some(Location::home(&[], &[".qoder"], "settings.json")),
    aliases: &[],
    environment: &[],
    layout: Layout::Groups,
    permission: Some(Permission {
        timeout: 86_400,
        unanswerable: &["AskUserQuestion", "ExitPlanMode"],
        rules: true,
        plans: false,
        reviewer_gate: false,
        settles_in_transcript: true,
        unattended: None,
        opt_in: false,
        reply: Reply::HookSpecificOutput,
    }),
    completion: None,
    origin: None,
    attention: false,
    compaction: None,
    prefixed_sessions: false,
    launch: POSIX_ONLY,
};

pub static QODER_CN: Hooks = Hooks {
    settings: Some(Location::home(&[], &[".qoder-cn"], "settings.json")),
    aliases: &["qoderCN"],
    environment: &[],
    layout: Layout::Groups,
    permission: Some(Permission {
        timeout: 86_400,
        unanswerable: &["AskUserQuestion", "ExitPlanMode"],
        rules: true,
        plans: false,
        reviewer_gate: false,
        settles_in_transcript: true,
        unattended: None,
        opt_in: false,
        reply: Reply::HookSpecificOutput,
    }),
    completion: None,
    origin: None,
    attention: false,
    compaction: None,
    prefixed_sessions: false,
    launch: POSIX_ONLY,
};

pub static QODER_WORK: Hooks = Hooks {
    settings: Some(Location::home(&[], &[".qoderwork"], "settings.json")),
    aliases: &["qoderWork"],
    environment: &[],
    layout: Layout::Groups,
    permission: Some(Permission {
        timeout: 86_400,
        unanswerable: &["AskUserQuestion", "ExitPlanMode"],
        rules: true,
        plans: false,
        reviewer_gate: false,
        settles_in_transcript: true,
        unattended: None,
        opt_in: false,
        reply: Reply::HookSpecificOutput,
    }),
    completion: None,
    origin: None,
    attention: false,
    compaction: None,
    prefixed_sessions: false,
    launch: POSIX_ONLY,
};
