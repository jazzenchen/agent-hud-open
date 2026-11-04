//! ZCode's permission hook, in its nested settings layout. Its plans stay
//! with the client.
use crate::hooks::{Hooks, Layout, Permission, Reply, POSIX_ONLY};
use crate::Location;

pub static HOOKS: Hooks = Hooks {
    settings: Some(Location::home(&[], &[".zcode/cli"], "config.json")),
    aliases: &[],
    layout: Layout::NestedGroups,
    permission: Some(Permission {
        timeout: 86_400,
        unanswerable: &["ExitPlanMode"],
        rules: false,
        plans: false,
        reviewer_gate: false,
        settles_in_transcript: false,
        unattended: None,
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
