//! CodeBuddy and WorkBuddy, which share one log format.
mod hooks;
pub mod transcript;

use crate::Agent;
use crate::Discovery;
use crate::{Location, Source};

static CODEBUDDY_SOURCES: [Source; 1] = [Source {
    reads_as: "codebuddy",
    location: Location {
        not_if: Some(".workbuddy"),
        ..Location::home(&["CODEBUDDY_CONFIG_DIR"], &[".codebuddy"], "projects")
    },
}];
static WORKBUDDY_SOURCES: [Source; 1] = [Source {
    reads_as: "workbuddy",
    location: Location::home(&[], &[".workbuddy"], "projects"),
}];
static CODEBUDDY_DISCOVERY: Discovery = Discovery {
    storage: &[Location {
        not_if: Some(".workbuddy"),
        ..Location::home(&["CODEBUDDY_CONFIG_DIR"], &[".codebuddy"], "projects")
    }],
    settings: &[Location {
        not_if: Some(".workbuddy"),
        ..Location::home(&["CODEBUDDY_CONFIG_DIR"], &[".codebuddy"], "settings.json")
    }],
    sessions: &[Location {
        not_if: Some(".workbuddy"),
        ..Location::home(&["CODEBUDDY_CONFIG_DIR"], &[".codebuddy"], "projects")
    }],
    commands: &["codebuddy"],
    apps: &["CodeBuddy"],
    ..Discovery::NONE
};
static WORKBUDDY_DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::home(&[], &[".workbuddy"], "projects"),
        Location::home(&[], &[".workbuddy"], "workbuddy.db"),
    ],
    settings: &[
        Location::home(&[], &[".workbuddy"], "settings.json"),
        Location::home(&[], &[".workbuddy"], "models.json"),
    ],
    sessions: &[
        Location::home(&[], &[".workbuddy"], "projects"),
        Location::home(&[], &[".workbuddy"], "workbuddy.db"),
    ],
    commands: &["workbuddy"],
    apps: &["WorkBuddy"],
    ..Discovery::NONE
};

pub static CODEBUDDY: Agent = Agent {
    id: "codebuddy",
    name: "CodeBuddy",
    sessions: true,
    hooks: Some(&hooks::CODEBUDDY),
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&CODEBUDDY_SOURCES),
    discovery: &CODEBUDDY_DISCOVERY,
};

pub static WORKBUDDY: Agent = Agent {
    id: "workbuddy",
    name: "WorkBuddy",
    sessions: true,
    hooks: Some(&hooks::WORKBUDDY),
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&WORKBUDDY_SOURCES),
    discovery: &WORKBUDDY_DISCOVERY,
};
