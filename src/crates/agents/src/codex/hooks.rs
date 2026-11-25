//! Codex's hooks: permission requests, which wait on the reviewer gate, and
//! session origin.
use crate::hooks::steer::{Event, Kind, Steer};
use crate::hooks::{Hooks, Launch, Layout, Permission, Reply, Setup};
use crate::l10n::tr;
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
    rewake: None,
    steer: Some(STEER),
    prefixed_sessions: false,
    launch: PerOs {
        macos: Support::Verified(Launch::Command),
        windows: Support::Unverified(Launch::PowerShell),
        linux: Support::Unverified(Launch::Command),
    },
};

/// The desktop app takes no steer from outside since ChatGPT.app 26.1117
/// (#178), but a hook's `additionalContext` joins the turn's next model
/// request, and a blocked `Stop` carries the turn on. A hook that waits is
/// one-shot: the next tool call, the read of the message included, starts
/// another. Codex clamps `Interrupt` to 3 seconds.
const STEER: Steer = Steer {
    events: &[
        wait("UserPromptSubmit"),
        wait("PreToolUse"),
        Event {
            name: "Stop",
            kind: Kind::End,
            timeout: 10,
            background: false,
            matcher: None,
        },
        Event {
            name: "Interrupt",
            kind: Kind::Interrupted,
            timeout: 3,
            background: false,
            matcher: None,
        },
        Event {
            name: "SessionStart",
            kind: Kind::Compacted,
            timeout: 10,
            background: false,
            matcher: Some("compact"),
        },
    ],
    on: PerOs {
        macos: Support::Verified(()),
        windows: Support::Unsupported("not checked on Windows"),
        linux: Support::Unsupported("not checked on Linux"),
    },
};

const fn wait(name: &'static str) -> Event {
    Event {
        name,
        kind: Kind::Wait,
        timeout: 86_400,
        background: true,
        matcher: None,
    }
}

/// Codex runs a hook only once the user allowed its exact command, and asks
/// again when the command changes; until then it skips the hook without a
/// word (`hook_review` reads which ones wait).
pub static SETUP: Setup = Setup {
    clients: &["codex"],
    step,
};

fn step(name: &str) -> String {
    tr!(
        "{{name}} runs these hooks once you allow them: click Review hooks above its message box, then Allow all. It asks again whenever they change.",
        name = name
    )
}
