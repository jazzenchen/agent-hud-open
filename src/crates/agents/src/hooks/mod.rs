//! What a client's hooks are: where they are configured, which hooks it has,
//! how its requests are answered and how a hook is launched on each OS. Each
//! client states its own in `<client>/hooks.rs`; the functions here read that
//! data and never name a client.
use crate::{by_id, text, HookEvent, PerOs, Support, ALL};
use serde_json::Value;

pub mod configuration;
pub mod events;
pub mod observer;
pub mod request;
pub mod turn_log;
pub mod windows;

pub use observer::Observer;

/// A client's hooks.
pub struct Hooks {
    /// The settings file the hooks live in; None when they have none.
    pub settings: Option<crate::Location>,
    /// Older names of the client that its callbacks and owned commands may use.
    pub aliases: &'static [&'static str],
    pub layout: Layout,
    pub permission: Option<Permission>,
    pub completion: Option<Completion>,
    /// `SessionStart` and `UserPromptSubmit` say where a session started;
    /// the rule tells the host from the hook's caller.
    pub origin: Option<crate::process::OriginRule>,
    /// `Notification` says a session waits for the user.
    pub attention: bool,
    /// `PreCompact` says a session is being compacted.
    pub compaction: Option<Compaction>,
    /// Its session ids carry the client's id in front (`codebuddy:<id>`).
    pub prefixed_sessions: bool,
    /// How a hook is launched on each OS.
    pub launch: PerOs<Launch>,
}

impl Hooks {
    /// The hook flags this client is installed with, in install order.
    pub fn flags(&self) -> Vec<&'static str> {
        let mut flags = Vec::new();
        if self.permission.is_some() {
            flags.push("--permission-hook");
        }
        if self.completion.is_some() {
            flags.push("--completion-hook");
        }
        if self.attention {
            flags.push("--attention-hook");
        }
        if self.compaction.is_some() {
            flags.push("--compaction-hook");
        }
        if self.origin.is_some() {
            flags.push("--session-origin-hook");
        }
        flags
    }
}

/// How a settings file lays out the hooks.
pub enum Layout {
    /// `hooks.<Event>` holds groups of `{matcher, hooks: [handler]}`.
    Groups,
    /// The same groups under `hooks.events`, beside `hooks.enabled`; a
    /// permission group carries no matcher.
    NestedGroups,
    /// Version 1: `hooks.<event>` lists the handlers themselves.
    Handlers(Handler),
    /// Agent HUD's own `agent-hud` entry, whose `Stop` holds the handler.
    OwnedEntry,
    /// A file in another format than JSON (Hermes's YAML), which the
    /// client's own function edits as text, so every byte that is not Agent
    /// HUD's stays as the user wrote it.
    Text(Text),
    /// A hooks-only file: `<Event>` at the top level holds the groups of
    /// `Groups`. `shadowed` is a settings file whose `hooks` the client reads
    /// for any event this file leaves out; installing copies that event's
    /// groups here first, so they keep running beside Agent HUD's.
    Events { shadowed: Option<crate::Location> },
}

/// A settings file the client's own code edits as text.
pub struct Text {
    /// The file's new text for one hook; Err, saying why, when the file is
    /// not one Agent HUD can change safely.
    pub patch: fn(&str, &TextEdit) -> Result<String, String>,
}

/// One hook's change to a text settings file.
pub struct TextEdit<'a> {
    /// The events the hook's handler is registered for.
    pub events: &'a [String],
    /// How long the client waits for it, in its own unit.
    pub timeout: u64,
    /// Whether a handler's command is Agent HUD's own for this hook, an
    /// older executable's included.
    pub owns: &'a dyn Fn(&str) -> bool,
    /// The command to register; None takes Agent HUD's handlers out.
    pub command: Option<&'a str>,
}

/// A handler written directly under its event.
pub struct Handler {
    /// The key that holds the command.
    pub command_key: &'static str,
    /// Whether a new handler says `"type": "command"`.
    pub typed: bool,
    /// The key that holds the timeout.
    pub timeout_key: &'static str,
}

/// The permission hook.
pub struct Permission {
    /// How long the client waits for a reply, in its own unit.
    pub timeout: u64,
    /// Tools whose question or plan cannot be answered from here; their
    /// requests stay with the client.
    pub unanswerable: &'static [&'static str],
    /// It applies a permanent rule it offered (`always`).
    pub rules: bool,
    /// It takes a plan's approval, or a request to keep planning.
    pub plans: bool,
    /// Its permission policy is no proof the user reviews the turn: the host
    /// confirms the turn's reviewer before holding a request.
    pub reviewer_gate: bool,
    /// Its transcript shows a request answered in the client's own window,
    /// so the host stops waiting for the HUD's answer once it does.
    pub settles_in_transcript: bool,
    /// A run nobody watches, whose requests stay with the client.
    pub unattended: Option<crate::process::UnattendedRule>,
    pub reply: Reply,
}

/// The shape of a permission reply.
pub enum Reply {
    /// `{"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": …}}`.
    HookSpecificOutput,
    /// `{"decision": "approve" | "block"}`.
    Decision,
    /// The decision itself, `{"behavior": "allow" | "deny", "message"?}`:
    /// Copilot CLI's `permissionRequest`, whose callback says `sessionId`,
    /// `toolName` and `toolInput`.
    Behavior,
    /// A `PreToolUse` callback, which runs before every tool call `matcher`
    /// names, answered with `{"hookSpecificOutput": {"hookEventName":
    /// "PreToolUse", "permissionDecision": "allow" | "deny"}}`. Only a call
    /// `asks` says the client would ask about is held; the others pass with
    /// no answer.
    PreToolUse {
        matcher: &'static str,
        asks: fn(&Value) -> bool,
    },
}

impl Reply {
    /// The event the permission hook is registered for.
    pub fn event(&self) -> &'static str {
        match self {
            Self::Behavior => "permissionRequest",
            Self::PreToolUse { .. } => "PreToolUse",
            Self::HookSpecificOutput | Self::Decision => "PermissionRequest",
        }
    }
}

impl Permission {
    /// The event the permission hook is registered for.
    pub fn event(&self) -> &'static str {
        self.reply.event()
    }
}

/// The completion hook.
pub struct Completion {
    /// The event the hook is registered for.
    pub event: &'static str,
    /// How long the client waits, in its own unit.
    pub timeout: u64,
    /// What a callback's payload says about the turn it completed; None
    /// ignores the callback.
    pub read: fn(&Value, i64) -> Option<Completed>,
    /// Fills in the final message from the transcript's tail where the
    /// payload lacks it.
    pub enrich: Option<fn(&mut HookEvent, &[u8]) -> bool>,
    /// What the callback is answered, as JSON text; None answers `{}`. Not a
    /// permission: Antigravity's own entrypoint answers `{"decision":"stop"}`.
    pub reply: Option<&'static str>,
    /// Events that say a turn is in flight, for a client that reports them.
    pub begin: Option<Begin>,
}

/// A client's events that say a turn has started or is still going: the
/// first is its prompt, the others renew it while it works.
pub struct Begin {
    pub events: &'static [&'static str],
    /// What a callback's payload says about the turn; None ignores it.
    pub read: fn(&Value) -> Option<Began>,
}

pub struct Began {
    pub session: String,
    pub turn: String,
    pub workspace: Option<String>,
    /// The turn waits for the person (an approval it asked for) until the
    /// next event says it runs again.
    pub waiting: bool,
}

/// A completed turn, as a completion callback reports it.
pub struct Completed {
    pub session: String,
    pub turn: String,
    pub workspace: Option<String>,
    pub model: Option<String>,
    /// The transcript the platform may read to enrich the event.
    pub transcript_path: Option<Value>,
    /// The tokens the turn spent, where the callback reports them and the
    /// client keeps them nowhere else on disk (Cursor). The platform appends
    /// them to the client's turn log (`hooks::turn_log`).
    pub usage: Option<crate::Usage>,
}

/// The compaction hook.
pub struct Compaction {
    /// An older flag whose command still counts as the compaction hook's.
    pub legacy_flag: Option<&'static str>,
}

/// How a hook's command is written into the settings file.
pub enum Launch {
    /// A POSIX command line: `'<agent-hud>' <flag> <client>`.
    Command,
    /// The executable in `command`, the flag and client in `args`.
    CommandArgs,
    /// A PowerShell line in `commandWindows`, beside the POSIX command.
    PowerShell,
    /// `exec` and `args` in place of the handler's command.
    ExecArgs,
}

/// A client with hooks, by its id or one of its older names.
pub fn hooks_of(name: &str) -> Result<(&'static str, &'static Hooks), String> {
    ALL.iter()
        .find_map(|agent| {
            let hooks = agent.hooks?;
            (agent.id == name || hooks.aliases.contains(&name)).then_some((agent.id, hooks))
        })
        .ok_or_else(|| "Unsupported client hook source".into())
}

/// The client's own name, or the id when it is unknown.
pub fn display(source: &str) -> &str {
    by_id(source).map_or(source, |agent| agent.name)
}

/// The hooks of the client with exactly this id.
pub fn hooks_by_id(id: &str) -> Option<&'static Hooks> {
    by_id(id)?.hooks
}

pub fn session_id(source: &str, raw: &str) -> String {
    if hooks_by_id(source).is_some_and(|hooks| hooks.prefixed_sessions) {
        format!("{source}:{raw}")
    } else {
        raw.into()
    }
}

/// A `Stop` callback as CodeBuddy, Qwen Code and Devin send it. With
/// `prompt_turns` the turn is the callback's `prompt_id`.
pub fn stop(payload: &Value, now_ms: i64, prompt_turns: bool) -> Option<Completed> {
    if payload["hook_event_name"] != "Stop" {
        return None;
    }
    let id = text(&payload["session_id"])?;
    let turn = if prompt_turns {
        text(&payload["prompt_id"])
    } else {
        None
    };
    Some(Completed {
        session: id.into(),
        turn: turn
            .map(str::to_owned)
            .unwrap_or_else(|| format!("stop-{now_ms}")),
        workspace: payload["cwd"].as_str().map(str::to_owned),
        model: None,
        transcript_path: None,
        usage: None,
    })
}

/// A hook launched as a POSIX command on macOS and Linux, and not launched on
/// Windows until its contract there is documented.
pub const POSIX_ONLY: PerOs<Launch> = PerOs {
    macos: Support::Verified(Launch::Command),
    windows: Support::Unsupported("no verified Windows hook launch contract"),
    linux: Support::Unverified(Launch::Command),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_follow_each_clients_hooks() {
        let flags = |id: &str| hooks_by_id(id).unwrap().flags();
        assert_eq!(
            flags("claude"),
            [
                "--permission-hook",
                "--attention-hook",
                "--compaction-hook",
                "--session-origin-hook"
            ]
        );
        assert_eq!(
            flags("codex"),
            ["--permission-hook", "--session-origin-hook"]
        );
        assert_eq!(
            flags("antigravity"),
            ["--completion-hook", "--session-origin-hook"]
        );
        assert_eq!(
            flags("codebuddy"),
            ["--permission-hook", "--completion-hook"]
        );
        assert_eq!(flags("workbuddy"), ["--permission-hook"]);
        for agent in ALL {
            for flag in agent.hooks.map(Hooks::flags).unwrap_or_default() {
                assert!(crate::HOOK_FLAGS.contains(&flag), "{} {flag}", agent.id);
            }
        }
    }

    #[test]
    fn completion_replies_are_json() {
        for agent in ALL {
            if let Some(reply) = agent
                .hooks
                .and_then(|hooks| hooks.completion.as_ref())
                .and_then(|completion| completion.reply)
            {
                assert!(serde_json::from_str::<Value>(reply).is_ok(), "{}", agent.id);
            }
        }
        let antigravity = hooks_by_id("antigravity").unwrap().completion.as_ref();
        assert_eq!(antigravity.unwrap().reply, Some(r#"{"decision":"stop"}"#));
    }

    #[test]
    fn claude_style_transcripts_settle_requests() {
        let settles = |name: &str| {
            hooks_of(name)
                .unwrap()
                .1
                .permission
                .as_ref()
                .is_some_and(|permission| permission.settles_in_transcript)
        };
        for name in [
            "claude",
            "qoder",
            "qoder-cn",
            "qoder-work",
            "qoderWork",
            "qoderCN",
        ] {
            assert!(settles(name), "{name}");
        }
        for name in ["codex", "codebuddy", "zcode", "qwen", "devin", "droid"] {
            assert!(!settles(name), "{name}");
        }
    }
}
