//! Cursor.
pub mod account;
mod events;
mod hooks;
mod sessions;

use crate::Agent;
use crate::{Discovery, Location};

static DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::app_data(&["Cursor"], ""),
        Location::home(&[], &[".cursor"], ""),
    ],
    settings: &[
        Location::app_data(&["Cursor"], "User/settings.json"),
        Location::app_data(&["Cursor"], "User/globalStorage/state.vscdb"),
    ],
    sessions: &[
        Location::home(&[], &[".cursor"], "chats"),
        Location::app_data(&["Cursor"], "User/workspaceStorage"),
    ],
    commands: &["cursor", "cursor-agent"],
    apps: &["Cursor"],
    bundles: &["com.todesktop.230313mzl4w4u92"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "cursor",
    name: "Cursor",
    sessions: false,
    hooks: Some(&hooks::HOOKS),
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&[]),
    discovery: &DISCOVERY,
};
