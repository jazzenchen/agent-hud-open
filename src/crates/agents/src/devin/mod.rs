//! Devin.
pub mod account;
mod hooks;
pub mod transcript;

use crate::Agent;
use crate::Discovery;
use crate::{Location, Source};

static SOURCES: [Source; 1] = [Source {
    reads_as: "devin",
    location: Location::data_home(&["devin"], "cli"),
}];
static DISCOVERY: Discovery = Discovery {
    storage: &[Location::data_home(&["devin"], "cli/sessions.db")],
    settings: &[Location::config_home(&["devin"], "config.json")],
    sessions: &[Location::data_home(&["devin"], "cli/sessions.db")],
    commands: &["devin"],
    apps: &["Devin", "Windsurf"],
    bundles: &["com.exafunction.windsurf"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "devin",
    name: "Devin",
    sessions: true,
    hooks: Some(&hooks::HOOKS),
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&SOURCES),
    discovery: &DISCOVERY,
};
