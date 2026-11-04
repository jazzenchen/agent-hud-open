//! GitHub Copilot.
pub mod account;
mod hooks;
pub mod plans;
pub mod process;
pub mod store;
pub mod transcript;

use crate::Agent;
use crate::Discovery;
use crate::{Location, Source};

static SOURCES: [Source; 4] = [
    Source {
        reads_as: "copilot",
        location: Location::home(&["COPILOT_HOME"], &[".copilot"], "session-state"),
    },
    Source {
        reads_as: "copilot",
        location: Location::home(&["COPILOT_HOME"], &[".copilot"], "otel"),
    },
    Source {
        reads_as: "copilot",
        location: Location::named_by(&["COPILOT_OTEL_FILE_EXPORTER_PATH"], ""),
    },
    Source {
        reads_as: "copilot",
        location: Location::home(&["COPILOT_HOME"], &[".copilot"], "session-store.db"),
    },
];
static DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::home(&["COPILOT_HOME"], &[".copilot"], "session-state"),
        Location::home(&["COPILOT_HOME"], &[".copilot"], "config.json"),
    ],
    settings: &[Location::home(
        &["COPILOT_HOME"],
        &[".copilot"],
        "config.json",
    )],
    sessions: &[
        Location::home(&["COPILOT_HOME"], &[".copilot"], "session-state"),
        Location::home(&["COPILOT_HOME"], &[".copilot"], "otel"),
    ],
    usage_files: &[
        Location::named_by(&["COPILOT_OTEL_FILE_EXPORTER_PATH"], ""),
        Location::home(&["COPILOT_HOME"], &[".copilot"], "session-store.db"),
    ],
    commands: &["copilot"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "copilot",
    name: "GitHub Copilot",
    sessions: true,
    hooks: Some(&hooks::HOOKS),
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: Some(&transcript::TURNS),
    items: None,
    sources: crate::everywhere(&SOURCES),
    discovery: &DISCOVERY,
};
