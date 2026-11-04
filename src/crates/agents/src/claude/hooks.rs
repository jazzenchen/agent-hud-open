//! Claude Code's hooks: permission requests with permanent rules, plans and
//! questions; session origin, attention and compaction.
use crate::hooks::{Compaction, Hooks, Launch, Layout, Permission, Reply};
use crate::Location;
use crate::{PerOs, Support};

pub static HOOKS: Hooks = Hooks {
    settings: Some(Location::home(
        &["CLAUDE_CONFIG_DIR"],
        &[".claude"],
        "settings.json",
    )),
    aliases: &[],
    layout: Layout::Groups,
    permission: Some(Permission {
        timeout: 86_400,
        unanswerable: &[],
        rules: true,
        plans: true,
        reviewer_gate: false,
        settles_in_transcript: true,
        unattended: Some(super::process::unattended),
        policy: None,
        opt_in: false,
        reply: Reply::HookSpecificOutput,
    }),
    completion: None,
    origin: Some(super::process::origin),
    attention: true,
    compaction: Some(Compaction {
        // The original compaction hook ran the session-origin entrypoint.
        legacy_flag: Some("--session-origin-hook"),
    }),
    prefixed_sessions: false,
    launch: PerOs {
        macos: Support::Verified(Launch::Command),
        windows: Support::Unverified(Launch::CommandArgs),
        linux: Support::Unverified(Launch::Command),
    },
};
