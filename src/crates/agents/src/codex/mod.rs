//! Codex.
pub mod account;
mod hooks;
pub mod process;
pub mod transcript;

use crate::Discovery;
use crate::{Agent, Carries, Channel, PerOs, Support};
use crate::{Location, Source};

/// The Codex desktop app's IPC: the approvals and questions it waits on, and
/// the answers to them. Platform runs it in `platform/src/clients/codex/`.
pub static DESKTOP: Channel = Channel {
    name: "Codex desktop app IPC",
    carries: &[Carries::Requests],
    support: PerOs {
        macos: Support::Verified(()),
        windows: Support::Unsupported(
            "Codex desktop's IPC is a Unix socket; its Windows transport is not implemented, so its requests stay in the app",
        ),
        linux: Support::Unverified(()),
    },
};

static SOURCES: [Source; 3] = [
    Source {
        reads_as: "codex-index",
        location: Location::home(&["CODEX_HOME"], &[".codex"], "session_index.jsonl"),
    },
    Source {
        reads_as: "codex",
        location: Location::home(&["CODEX_HOME"], &[".codex"], "sessions"),
    },
    Source {
        reads_as: "codex",
        location: Location::home(&["CODEX_HOME"], &[".codex"], "archived_sessions"),
    },
];
static DISCOVERY: Discovery = Discovery {
    storage: &[Location::home(&["CODEX_HOME"], &[".codex"], "")],
    settings: &[
        Location::home(&["CODEX_HOME"], &[".codex"], "config.toml"),
        Location::home(&["CODEX_HOME"], &[".codex"], "auth.json"),
        Location::home(&["CODEX_HOME"], &[".codex"], "hooks.json"),
    ],
    commands: &["codex"],
    apps: &["Codex", "ChatGPT"],
    bundles: &["com.openai.codex"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "codex",
    name: "Codex",
    sessions: true,
    hooks: Some(&hooks::HOOKS),
    observer: None,
    channel: Some(&DESKTOP),
    program: Some(process::PROGRAM),
    replies: None,
    turns: Some(&transcript::TURNS),
    items: Some(&transcript::ITEMS),
    sources: crate::everywhere(&SOURCES),
    discovery: &DISCOVERY,
};
