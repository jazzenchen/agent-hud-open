//! Hermes.
pub mod transcript;

use crate::Agent;
use crate::Discovery;
use crate::{Location, Source};

static SOURCES: [Source; 1] = [Source {
    reads_as: "hermes",
    location: Location::home(&["HERMES_HOME"], &[".hermes"], ""),
}];
static DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::home(&["HERMES_HOME"], &[".hermes"], "state.db"),
        Location::home(&["HERMES_HOME"], &[".hermes"], "profiles"),
    ],
    settings: &[
        Location::home(&["HERMES_HOME"], &[".hermes"], "config.yaml"),
        Location::home(&["HERMES_HOME"], &[".hermes"], "config.json"),
    ],
    sessions: &[
        Location::home(&["HERMES_HOME"], &[".hermes"], "state.db"),
        Location::home(&["HERMES_HOME"], &[".hermes"], "profiles"),
    ],
    commands: &["hermes"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "hermes",
    name: "Hermes",
    sessions: true,
    hooks: None,
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&SOURCES),
    discovery: &DISCOVERY,
};
