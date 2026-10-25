//! Qwen Code.
mod hooks;
pub mod transcript;

use crate::Agent;
use crate::Discovery;
use crate::{Location, Source};

static SOURCES: [Source; 1] = [Source {
    reads_as: "qwen",
    location: Location::home(&["QWEN_RUNTIME_DIR", "QWEN_HOME"], &[".qwen"], "projects"),
}];
static DISCOVERY: Discovery = Discovery {
    storage: &[Location::home(&["QWEN_HOME"], &[".qwen"], "projects")],
    settings: &[
        Location::home(&["QWEN_HOME"], &[".qwen"], "settings.json"),
        Location::home(&["QWEN_HOME"], &[".qwen"], "oauth_creds.json"),
    ],
    sessions: &[Location::home(&["QWEN_HOME"], &[".qwen"], "projects")],
    commands: &["qwen"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "qwen",
    name: "Qwen Code",
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
