//! ZCode.
mod hooks;
pub mod transcript;

use crate::Agent;
use crate::Discovery;
use crate::{Location, Source};

static SOURCES: [Source; 1] = [Source {
    reads_as: "zcode",
    location: Location::home(&[], &[".zcode"], "cli/db"),
}];
static DISCOVERY: Discovery = Discovery {
    storage: &[Location::home(&[], &[".zcode"], "cli/db/db.sqlite")],
    settings: &[Location::home(&[], &[".zcode"], "cli/config.json")],
    sessions: &[Location::home(&[], &[".zcode"], "cli/db/db.sqlite")],
    commands: &["zcode"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "zcode",
    name: "ZCode",
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
