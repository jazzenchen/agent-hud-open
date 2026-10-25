//! Grok: the CLI and Grok Bot, two sources.
pub mod account;
pub mod bot;
pub mod cache;
pub mod transcript;

use crate::Agent;
use crate::Discovery;
use crate::{Location, PerOs, Source, Support};

/// Grok Bot's sessions, from its desktop cache where its place is known.
static BOT_SOURCES: [Source; 1] = [Source {
    reads_as: "grok-bot",
    location: account::CACHE_LOCATION,
}];
static CLI_SOURCES: [Source; 2] = [
    Source {
        reads_as: "grok-cli",
        location: Location::home(&["GROK_HOME"], &[".grok"], "sessions"),
    },
    Source {
        reads_as: "grok-cli",
        location: Location::home(&["GROK_HOME"], &[".grok"], "logs"),
    },
];
static CLI_DISCOVERY: Discovery = Discovery {
    storage: &[Location::home(&["GROK_HOME"], &[".grok"], "")],
    settings: &[
        Location::home(&["GROK_HOME"], &[".grok"], "config.json"),
        Location::home(&["GROK_HOME"], &[".grok"], "settings.json"),
    ],
    sessions: &[
        Location::home(&["GROK_HOME"], &[".grok"], "sessions"),
        Location::home(&["GROK_HOME"], &[".grok"], "logs"),
    ],
    commands: &["grok"],
    ..Discovery::NONE
};
static BOT_DISCOVERY: Discovery = Discovery {
    storage: &[Location::app_data(&["Grok Bot"], "")],
    sessions: &[Location::app_data(&["Grok Bot"], "sand-client-persistence")],
    apps: &["Grok Bot"],
    bundles: &["com.anysphere.sand"],
    ..Discovery::NONE
};

pub static CLI: Agent = Agent {
    id: "grok-cli",
    name: "Grok CLI",
    sessions: true,
    hooks: None,
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&CLI_SOURCES),
    discovery: &CLI_DISCOVERY,
};

pub static BOT: Agent = Agent {
    id: "grok-bot",
    name: "Grok Bot",
    sessions: true,
    hooks: None,
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: PerOs {
        macos: Support::Verified(&BOT_SOURCES),
        windows: Support::Unsupported("Grok Bot's cache location on Windows is not known yet"),
        linux: Support::Unsupported("Grok Bot's cache location on Linux is not known yet"),
    },
    discovery: &BOT_DISCOVERY,
};
