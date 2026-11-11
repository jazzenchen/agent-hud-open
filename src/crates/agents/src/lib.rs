//! Every client Agent HUD knows: its descriptor, its transcript parser, its
//! account reader and its formats. It reads only the logs and databases the
//! engine names. Account readers reach files, the network and credential
//! stores only through the engine's `Host`; processes, sockets,
//! operating-system conditions and native desktop messaging do not belong in
//! this crate.
use serde_json::Value;

pub mod account;
mod agent;
mod change;
pub mod channel;
pub mod discovery;
pub mod endpoints;
pub mod hooks;
mod location;
pub mod naming;
mod os;
pub mod plans;
pub mod process;
pub mod protobuf;
pub mod relays;
pub mod replies;
pub mod sqlite;
pub mod transcript;

pub use account::{Answer, Host, Reading};
pub use agent::{account_provider, by_id, session_target, Agent, ACCOUNTS, ALL};
pub use agent_hud_l10n as l10n;
pub use change::{Change, Why};
pub use channel::{Carries, Channel};
pub use discovery::{Discovery, Look, SessionTarget};
pub use location::{everywhere, Base, Location, Places, Profiles, Source};
pub use os::{Os, PerOs, Support};
pub use replies::Replies;
pub use transcript::{
    number, LiveLog, Log, RecentCall, TimelineBucket, Usage, DATA_VERSION, DATA_VERSIONS,
    MAX_TURNS, PARSER_VERSION,
};

// One folder per client, or per family of clients that share a format.
pub mod antigravity;
pub mod ark;
pub mod buddy;
pub mod claude;
pub mod cline;
pub mod codex;
pub mod coding_plans;
pub mod copilot;
pub mod cursor;
pub mod deepseek;
pub mod devin;
pub mod droid;
pub mod glm;
pub mod goose;
pub mod grok;
pub mod hermes;
pub mod kimi;
pub mod minimax;
pub mod openclaw;
pub mod opencode;
pub mod pi;
pub mod qoder;
pub mod qwen;
pub mod trae;
pub mod zcode;

pub use antigravity::permissions::{antigravity_permissions, AntigravityPermission};
pub use hooks::configuration::{
    configuration, hook_commands, patch_configuration, patch_text, text_settings, HookConfiguration,
};
pub use hooks::events::{enrich_codebuddy_completion, parse_event, HookEvent};
pub use hooks::observer::parse_observation;
pub use hooks::request::{parse_request, Request};
pub use hooks::windows::{patch_configuration_for_windows, windows_hook_handler};

/// The clients with a permission hook, from their descriptors (see the test).
pub const PERMISSION_SOURCES: &[&str] = &[
    "claude",
    "codex",
    "qoder",
    "qoder-cn",
    "qoder-work",
    "codebuddy",
    "workbuddy",
    "zcode",
    "qwen",
    "devin",
    "copilot",
    "droid",
];
/// The clients with a completion hook, from their descriptors (see the test).
pub const COMPLETION_SOURCES: &[&str] = &[
    "antigravity",
    "cursor",
    "copilot",
    "qoder",
    "qoder-cn",
    "qoder-work",
    "codebuddy",
    "workbuddy",
    "zcode",
    "qwen",
    "devin",
    "hermes",
    "droid",
    "goose",
    "trae",
    "trae-cn",
    "kimi",
];
pub const HOOK_FLAGS: &[&str] = &[
    "--permission-hook",
    "--completion-hook",
    "--attention-hook",
    "--session-origin-hook",
    "--compaction-hook",
    "--rewake-hook",
];

pub fn iso(at: i64) -> Option<String> {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(at).map(|date| date.to_rfc3339())
}
pub fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| !s.is_empty())
}
pub fn trimmed(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(2048).collect())
}
/// The platform bounds a hook payload's bytes where it reads them; here it
/// only has to be an object.
pub fn validate_payload(value: &Value) -> Result<(), String> {
    if !value.is_object() {
        return Err("Hook payload must be a JSON object".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_source_lists_match_the_descriptors() {
        let with = |has: fn(&hooks::Hooks) -> bool| {
            let mut ids = ALL
                .iter()
                .filter(|agent| agent.hooks.is_some_and(has))
                .map(|agent| agent.id)
                .collect::<Vec<_>>();
            ids.sort_unstable();
            ids
        };
        let sorted = |list: &[&'static str]| {
            let mut ids = list.to_vec();
            ids.sort_unstable();
            ids
        };
        assert_eq!(
            with(|hooks| hooks.permission.is_some()),
            sorted(PERMISSION_SOURCES)
        );
        assert_eq!(
            with(|hooks| hooks.completion.is_some()),
            sorted(COMPLETION_SOURCES)
        );
    }
}
