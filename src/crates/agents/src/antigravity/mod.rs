//! Antigravity.
pub mod account;
mod cli;
mod hooks;
pub mod permissions;
pub mod proto;
pub mod statusline;

use crate::Discovery;
use crate::{Agent, Carries, Channel, PerOs, Support};
use crate::{Location, Source};

/// Antigravity's running language server, found and called on loopback: its
/// quota, and the permission requests it waits on. Platform runs it in
/// `platform/src/clients/antigravity/`.
pub static LOCAL_SERVICE: Channel = Channel {
    name: "Antigravity's local language server",
    carries: &[Carries::Requests, Carries::Quota],
    support: PerOs {
        macos: Support::Verified(()),
        windows: Support::Unsupported(
            "Antigravity local-service discovery is not implemented on Windows",
        ),
        linux: Support::Unverified(()),
    },
};

static SOURCES: [Source; 3] = [
    Source {
        reads_as: "antigravity",
        location: Location::home(
            &["GEMINI_CLI_HOME"],
            &[".gemini"],
            "antigravity-cli/conversations",
        ),
    },
    Source {
        reads_as: "antigravity",
        location: Location::home(&["GEMINI_CLI_HOME"], &[".gemini"], "antigravity"),
    },
    Source {
        reads_as: "antigravity",
        location: Location::home(
            &["GEMINI_CLI_HOME"],
            &[".gemini"],
            "antigravity/conversations",
        ),
    },
];
static DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::home(&["GEMINI_CLI_HOME"], &[".gemini"], "antigravity"),
        Location::home(&["GEMINI_CLI_HOME"], &[".gemini"], "antigravity-cli"),
        Location::home(&["GEMINI_CLI_HOME"], &[".gemini"], "antigravity-ide"),
    ],
    settings: &[Location::home(
        &["GEMINI_CLI_HOME"],
        &[".gemini"],
        "config/hooks.json",
    )],
    sessions: &[
        Location::home(
            &["GEMINI_CLI_HOME"],
            &[".gemini"],
            "antigravity/conversations",
        ),
        Location::home(
            &["GEMINI_CLI_HOME"],
            &[".gemini"],
            "antigravity-cli/conversations",
        ),
    ],
    commands: &["agy", "antigravity"],
    apps: &["Antigravity", "Antigravity IDE"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "antigravity",
    name: "Antigravity",
    sessions: true,
    hooks: Some(&hooks::HOOKS),
    observer: None,
    channel: Some(&LOCAL_SERVICE),
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&SOURCES),
    discovery: &DISCOVERY,
};
