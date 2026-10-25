//! Pi.
use crate::hooks::Observer;
use crate::Discovery;
use crate::{Location, Source};
pub mod transcript;

use crate::Agent;

static SOURCES: [Source; 1] = [Source {
    reads_as: "pi",
    location: Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "sessions"),
}];
static DISCOVERY: Discovery = Discovery {
    storage: &[Location::home(
        &["PI_CODING_AGENT_DIR"],
        &[".pi/agent"],
        "sessions",
    )],
    settings: &[
        Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "settings.json"),
        Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "models.json"),
        Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "auth.json"),
    ],
    sessions: &[Location::home(
        &["PI_CODING_AGENT_DIR"],
        &[".pi/agent"],
        "sessions",
    )],
    commands: &["pi"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "pi",
    name: "Pi",
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

/// Pi's extension records each turn's state as it changes.
static OBSERVER: Observer = Observer {
    turns: Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "agent-hud/turns"),
    script: Location::home(
        &["PI_CODING_AGENT_DIR"],
        &[".pi/agent"],
        "extensions/agent-hud.ts",
    ),
    marker: "// Agent HUD Pi session observer\n",
    body: include_str!("observer.ts"),
    at_key: "observedAtMs",
    states: true,
};
