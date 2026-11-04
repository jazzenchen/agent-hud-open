//! Codex's hooks: permission requests, which wait on the reviewer gate, and
//! session origin.
use crate::hooks::{Hooks, Launch, Layout, Permission, Reply};
use crate::Location;
use crate::{PerOs, Support};

pub static HOOKS: Hooks = Hooks {
    settings: Some(Location::home(&["CODEX_HOME"], &[".codex"], "hooks.json")),
    aliases: &[],
    environment: &[],
    layout: Layout::Groups,
    permission: Some(Permission {
        timeout: 86_400,
        unanswerable: &[],
        rules: false,
        plans: false,
        reviewer_gate: true,
        settles_in_transcript: false,
        unattended: None,
        policy: None,
        opt_in: false,
        reply: Reply::HookSpecificOutput,
    }),
    completion: None,
    origin: Some(super::process::origin),
    attention: false,
    compaction: None,
    prefixed_sessions: false,
    launch: PerOs {
        macos: Support::Verified(Launch::Command),
        windows: Support::Unverified(Launch::PowerShell),
        linux: Support::Unverified(Launch::Command),
    },
};
