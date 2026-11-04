//! Goose, the agent Block started (now the Agentic AI Foundation's): its
//! command line and its desktop app share one session database,
//! `sessions/sessions.db` in Goose's data folder (Goose `v1.57.0`,
//! `crates/goose/src/config/paths.rs`). Goose places its folders the XDG way
//! on macOS and Linux and under the roaming AppData folder on Windows;
//! `GOOSE_PATH_ROOT` moves them all under one folder.
mod hooks;
pub mod transcript;

use crate::{Agent, Discovery, Location, PerOs, Source, Support};

const fn sessions(location: Location) -> Source {
    Source {
        reads_as: "goose",
        location,
    }
}
/// The folder `GOOSE_PATH_ROOT` names holds `data/sessions`.
const MOVED: Source = sessions(Location::named_by(&["GOOSE_PATH_ROOT"], "data/sessions"));
static XDG_SOURCES: [Source; 2] = [MOVED, sessions(Location::data_home(&["goose"], "sessions"))];
static WINDOWS_SOURCES: [Source; 2] = [
    MOVED,
    sessions(Location::app_data(&["Block/goose"], "data/sessions")),
];

static DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::data_home(&["goose"], ""),
        Location::app_data(&["Block/goose"], ""),
    ],
    settings: &[
        Location::config_home(&["goose"], "config.yaml"),
        Location::app_data(&["Block/goose"], "config/config.yaml"),
    ],
    sessions: &[
        Location::data_home(&["goose"], "sessions/sessions.db"),
        Location::app_data(&["Block/goose"], "data/sessions/sessions.db"),
    ],
    commands: &["goose"],
    apps: &["Goose"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "goose",
    name: "Goose",
    sessions: true,
    hooks: Some(&hooks::HOOKS),
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    // Read from Goose's source; not yet checked against a running Goose.
    sources: PerOs {
        macos: Support::Unverified(&XDG_SOURCES),
        windows: Support::Unverified(&WINDOWS_SOURCES),
        linux: Support::Unverified(&XDG_SOURCES),
    },
    discovery: &DISCOVERY,
};
