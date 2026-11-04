//! Factory Droid: Factory's coding agent, its CLI (`droid`) and the Factory
//! App, which keep their sessions in `~/.factory/sessions`.
//!
//! Its sessions and limits are known from Droid 1.3.0's own code and
//! docs.factory.com, not yet from a machine that runs it: every OS's entry
//! is unverified.
pub mod account;
pub mod transcript;

use crate::Agent;
use crate::Discovery;
use crate::{Location, PerOs, Source, Support};

static SOURCES: [Source; 1] = [Source {
    reads_as: "droid",
    location: Location::home(&[], &[".factory"], "sessions"),
}];
static DISCOVERY: Discovery = Discovery {
    storage: &[Location::home(&[], &[".factory"], "sessions")],
    settings: &[
        Location::home(&[], &[".factory"], "settings.json"),
        Location::home(&[], &[".factory"], "hooks.json"),
    ],
    sessions: &[Location::home(&[], &[".factory"], "sessions")],
    commands: &["droid"],
    apps: &["Factory"],
    credentials_env: &["FACTORY_API_KEY"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "droid",
    name: "Factory Droid",
    sessions: true,
    hooks: None,
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: PerOs {
        macos: Support::Unverified(&SOURCES),
        windows: Support::Unverified(&SOURCES),
        linux: Support::Unverified(&SOURCES),
    },
    discovery: &DISCOVERY,
};
