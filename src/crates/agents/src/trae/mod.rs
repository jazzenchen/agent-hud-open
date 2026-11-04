//! TRAE, ByteDance's AI coding app (an Agent window and an IDE mode in one
//! app), and its China build TRAE CN. Each keeps its conversations encrypted
//! with a key only the running app holds, so no session file is read: a
//! session is what its hooks report (`hooks.rs`), titled by its first
//! prompt, with no tokens. The credits show only on trae.ai and trae.cn
//! (`account.rs`).
pub mod account;
mod hooks;
pub mod process;

pub use hooks::{IMPORTS, SETUP};

use crate::Agent;
use crate::{Discovery, Location};

static TRAE_DISCOVERY: Discovery = Discovery {
    storage: &[Location::app_data(&["Trae", "TRAE"], "")],
    settings: &[
        Location::app_data(&["Trae", "TRAE"], "User/settings.json"),
        Location::home(&[], &[".trae"], "hooks.json"),
    ],
    commands: &["trae"],
    apps: &["Trae", "TRAE"],
    bundles: &[process::BUNDLES[0]],
    ..Discovery::NONE
};
static TRAE_CN_DISCOVERY: Discovery = Discovery {
    storage: &[Location::app_data(&["Trae CN", "TRAE CN"], "")],
    settings: &[
        Location::app_data(&["Trae CN", "TRAE CN"], "User/settings.json"),
        Location::home(&[], &[".trae-cn"], "hooks.json"),
    ],
    commands: &["trae-cn"],
    apps: &["Trae CN", "TRAE CN"],
    bundles: &[process::BUNDLES[1]],
    ..Discovery::NONE
};

pub static TRAE: Agent = Agent {
    id: "trae",
    name: "TRAE",
    sessions: false,
    hooks: Some(&hooks::TRAE),
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&[]),
    discovery: &TRAE_DISCOVERY,
};

pub static TRAE_CN: Agent = Agent {
    id: "trae-cn",
    name: "TRAE CN",
    sessions: false,
    hooks: Some(&hooks::TRAE_CN),
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&[]),
    discovery: &TRAE_CN_DISCOVERY,
};
