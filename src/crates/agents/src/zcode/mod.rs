//! ZCode: its desktop app and its CLI keep their sessions, their calls and
//! their conversations in one database (`~/.zcode/cli/db/db.sqlite`), read
//! by `transcript` and `conversation`, and its plan balance in
//! `~/.zcode/v2/credentials.json` (`account`).
pub mod account;
pub mod conversation;
#[cfg(test)]
mod fixtures;
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
    storage: &[
        Location::home(&[], &[".zcode"], "cli/db/db.sqlite"),
        Location::home(&[], &[".zcode"], "v2/credentials.json"),
    ],
    settings: &[Location::home(&[], &[".zcode"], "cli/config.json")],
    sessions: &[Location::home(&[], &[".zcode"], "cli/db/db.sqlite")],
    commands: &["zcode"],
    apps: &["ZCode"],
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
    turns: Some(&conversation::TURNS),
    items: Some(&conversation::ITEMS),
    sources: crate::everywhere(&SOURCES),
    discovery: &DISCOVERY,
};
