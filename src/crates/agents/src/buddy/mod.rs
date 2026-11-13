//! CodeBuddy, CodeBuddy CN and WorkBuddy, Tencent's AI coding clients.
pub mod account;
mod hooks;
mod items;
mod limits;
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
    storage: &[
        Location {
            not_if: Some(".workbuddy"),
            ..Location::home(&["CODEBUDDY_CONFIG_DIR"], &[".codebuddy"], "projects")
        },
        Location::app_data(&["CodeBuddy"], "User/globalStorage/state.vscdb"),
    ],
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
static CODEBUDDY_CN_DISCOVERY: Discovery = Discovery {
    storage: &[Location::app_data(
        &["CodeBuddy CN"],
        "User/globalStorage/state.vscdb",
    )],
    settings: &[Location::app_data(&["CodeBuddy CN"], "User/settings.json")],
    commands: &["codebuddy-cn"],
    apps: &["CodeBuddy CN"],
    ..Discovery::NONE
};
static WORKBUDDY_DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::home(&[], &[".workbuddy"], "projects"),
        Location::home(&[], &[".workbuddy"], "workbuddy.db"),
        Location::app_data(
            &["CodeBuddyExtension"],
            "Data/Public/auth/workbuddy-desktop.info",
        ),
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
    items: Some(&items::ITEMS),
    sources: crate::everywhere(&CODEBUDDY_SOURCES),
    discovery: &CODEBUDDY_DISCOVERY,
};

pub static CODEBUDDY_CN: Agent = Agent {
    id: "codebuddy-cn",
    name: "CodeBuddy CN",
    sessions: false,
    hooks: None,
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&[]),
    discovery: &CODEBUDDY_CN_DISCOVERY,
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
    items: Some(&items::ITEMS),
    sources: crate::everywhere(&WORKBUDDY_SOURCES),
    discovery: &WORKBUDDY_DISCOVERY,
};
