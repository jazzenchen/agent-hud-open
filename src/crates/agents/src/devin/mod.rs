//! Devin: the Devin CLI, and Devin Local, the same CLI that Devin Desktop
//! bundles and runs over ACP (`devin acp`). Both keep their sessions in one
//! `sessions.db`, read the same hooks and cache the same account status, so
//! they are one agent whose sessions say which client drives them.
pub mod account;
mod hooks;
pub mod process;
pub mod transcript;

pub use hooks::IMPORTS;

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
    program: Some(process::PROGRAM),
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&SOURCES),
    discovery: &DISCOVERY,
};
