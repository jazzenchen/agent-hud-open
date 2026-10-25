//! OpenCode.
use crate::hooks::Observer;
use crate::Discovery;
use crate::{Location, Source};
pub mod legacy;
pub mod transcript;

use crate::Agent;

static SOURCES: [Source; 2] = [
    Source {
        reads_as: "opencode",
        location: Location::data_home(&["opencode"], ""),
    },
    Source {
        reads_as: "opencode",
        location: Location::data_home(&["opencode"], "storage/message"),
    },
];
static DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::data_home(&["opencode"], ""),
        Location::config_home(&["opencode"], ""),
    ],
    settings: &[
        Location::config_home(&["opencode"], "opencode.json"),
        Location::config_home(&["opencode"], "opencode.jsonc"),
        Location::data_home(&["opencode"], "auth.json"),
    ],
    sessions: &[
        Location::data_home(&["opencode"], "storage/session"),
        Location::data_home(&["opencode"], "opencode.db"),
    ],
    commands: &["opencode"],
    credentials_env: &["OPENCODE_GO_API_KEY"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "opencode",
    name: "OpenCode",
    sessions: true,
    hooks: None,
    observer: Some(&OBSERVER),
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&SOURCES),
    discovery: &DISCOVERY,
};

/// OpenCode's plugin records each turn when it completes.
static OBSERVER: Observer = Observer {
    turns: Location::data_home(&["opencode"], "agent-hud/turns"),
    script: Location::config_home(&["opencode"], "plugin/agent-hud.js"),
    marker: "// Agent HUD OpenCode session observer\n",
    body: include_str!("observer.js"),
    at_key: "completedAtMs",
    states: false,
};
