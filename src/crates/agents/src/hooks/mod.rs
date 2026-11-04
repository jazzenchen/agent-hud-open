//! What a client's hooks are: where they are configured, which hooks it has,
//! how its requests are answered and how a hook is launched on each OS. Each
//! client states its own in `<client>/hooks.rs`; the functions here read that
//! data and never name a client.
use crate::{by_id, text, HookEvent, PerOs, Support, ALL};
use serde_json::Value;

pub mod configuration;
pub mod events;
mod layout;
pub mod observer;
mod patch;
mod permission;
pub mod request;
pub mod turn_log;
pub mod windows;

pub use layout::{Handler, Layout, Text, TextEdit};
pub use observer::Observer;
pub use permission::{screen, Files, Permission, Policy, PolicyRule, Reply, RULE_ASKS};

/// A client's hooks.
pub struct Hooks {
    /// The settings file the hooks live in, one per profile for a client
    /// whose profiles each read their own (`Location::profiles`); None when
    /// they have none.
    pub settings: Option<crate::Location>,
    /// Variables of the environment the client runs its hooks in that say
    /// what its payloads leave out (the folder of the Hermes profile that
    /// fired one). The hook adds the ones set to the payload under
    /// `ENVIRONMENT` before reading it.
    pub environment: &'static [&'static str],
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
    /// The hook flags this client is installed with, in install order: all
    /// but the ones that wait for the user to turn them on.
    pub fn flags(&self) -> Vec<&'static str> {
        self.all_flags()
            .into_iter()
            .filter(|(_, opt_in)| !opt_in)
            .map(|(flag, _)| flag)
            .collect()
    }

    /// Every hook flag this client has, in install order, each with whether
    /// it goes in only once the user turns it on (`Permission::opt_in`).
    pub fn all_flags(&self) -> Vec<(&'static str, bool)> {
        let mut flags = Vec::new();
        if let Some(permission) = &self.permission {
            flags.push(("--permission-hook", permission.opt_in));
        }
        if self.completion.is_some() {
            flags.push(("--completion-hook", false));
        }
        if self.attention {
            flags.push(("--attention-hook", false));
        }
        if self.compaction.is_some() {
            flags.push(("--compaction-hook", false));
        }
        if self.origin.is_some() {
            flags.push(("--session-origin-hook", false));
        }
        flags
    }

    /// Whether one of its hooks goes in only once the user turns it on.
    pub fn opt_in(&self) -> bool {
        self.all_flags().iter().any(|(_, opt_in)| *opt_in)
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

/// A step the user takes in a client itself before it runs the hooks Agent
/// HUD put in its settings (TRAE asks to turn its hooks on).
pub struct Setup {
    pub clients: &'static [&'static str],
    /// What to tell the user, given the client's name.
    pub step: fn(&str) -> String,
}

/// Every client that needs such a step.
pub static SETUPS: &[&Setup] = &[&crate::trae::SETUP];

/// What the user does in the client with this id before its hooks run.
pub fn setup_step(id: &str) -> Option<String> {
    let setup = SETUPS.iter().find(|setup| setup.clients.contains(&id))?;
    Some((setup.step)(display(id)))
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

/// Where a hook puts the variables of its environment that the client's
/// `Hooks::environment` names, in the payload it reads.
pub const ENVIRONMENT: &str = "agentHudEnvironment";

/// Adds to `payload` the variables of the hook's environment (`env`) that the
/// client's `Hooks::environment` names and that are set, under
/// `ENVIRONMENT`. A client that names none, or a payload that is no object,
/// is left as it is.
pub fn add_environment(source: &str, payload: &mut Value, env: &dyn Fn(&str) -> Option<String>) {
    let Some(names) = hooks_of(source).ok().map(|(_, hooks)| hooks.environment) else {
        return;
    };
    let set = names
        .iter()
        .filter_map(|name| Some(((*name).to_owned(), Value::String(env(name)?))))
        .collect::<serde_json::Map<_, _>>();
    if let (false, Some(payload)) = (set.is_empty(), payload.as_object_mut()) {
        payload.insert(ENVIRONMENT.into(), Value::Object(set));
    }
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
mod tests;
