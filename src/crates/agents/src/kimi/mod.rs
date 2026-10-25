//! Kimi.
pub mod transcript;

use crate::Agent;
use crate::Discovery;
use crate::{Location, Source};

static SOURCES: [Source; 2] = [
    Source {
        reads_as: "kimi",
        location: Location::home(&["KIMI_CODE_HOME"], &[".kimi-code"], "sessions"),
    },
    Source {
        reads_as: "kimi",
        location: Location::home(&[], &[".kimi"], "sessions"),
    },
];
static DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::home(&["KIMI_CODE_HOME"], &[".kimi-code"], ""),
        Location::home(&[], &[".kimi"], "sessions"),
    ],
    settings: &[
        Location::home(&["KIMI_CODE_HOME"], &[".kimi-code"], "config.toml"),
        Location::home(&["KIMI_CODE_HOME"], &[".kimi-code"], "credentials"),
    ],
    sessions: &[
        Location::home(&["KIMI_CODE_HOME"], &[".kimi-code"], "sessions"),
        Location::home(&[], &[".kimi"], "sessions"),
    ],
    commands: &["kimi"],
    credentials_env: &["KIMI_CODE_API_KEY", "KIMI_API_KEY"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "kimi",
    name: "Kimi",
    sessions: true,
    hooks: None,
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: Some(&transcript::TURNS),
    items: None,
    sources: crate::everywhere(&SOURCES),
    discovery: &DISCOVERY,
};
