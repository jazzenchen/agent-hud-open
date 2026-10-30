//! Antigravity.
pub mod account;
mod cli;
mod hooks;
pub mod permissions;
pub mod process;
pub mod proto;
pub mod statusline;
pub mod transcript;

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

/// Account reads are independently qualified from the permission observer and
/// remote control operations. Windows currently supports read-only discovery.
pub static ACCOUNT_SERVICE: Channel = Channel {
    name: "Antigravity's local account service",
    carries: &[Carries::Quota],
    support: PerOs {
        macos: Support::Verified(()),
        windows: Support::Unverified(()),
        linux: Support::Unverified(()),
    },
};

static SOURCES: [Source; 5] = [
    Source {
        reads_as: "antigravity",
        location: Location::home(&["GEMINI_CLI_HOME"], &[".gemini"], "antigravity-cli"),
    },
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
    Source {
        reads_as: "antigravity",
        location: Location::home(&["GEMINI_CLI_HOME"], &[".gemini"], "antigravity-ide"),
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
        Location::home(&["GEMINI_CLI_HOME"], &[".gemini"], "antigravity/brain"),
        Location::home(
            &["GEMINI_CLI_HOME"],
            &[".gemini"],
            "antigravity-cli/conversations",
        ),
        Location::home(&["GEMINI_CLI_HOME"], &[".gemini"], "antigravity-cli/brain"),
        Location::home(
            &["GEMINI_CLI_HOME"],
            &[".gemini"],
            "antigravity-ide/conversations",
        ),
        Location::home(&["GEMINI_CLI_HOME"], &[".gemini"], "antigravity-ide/brain"),
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
    program: Some(process::PROGRAM),
    replies: None,
    turns: Some(&transcript::TURNS),
    items: Some(&transcript::ITEMS),
    sources: crate::everywhere(&SOURCES),
    discovery: &DISCOVERY,
};
