//! DeepSeek.
pub mod account;
mod hooks;
pub mod transcript;

use crate::Discovery;
use crate::{Agent, Carries, Channel, PerOs, Support};
use crate::{Location, Source};

// Harness only shows and focuses its main window; it has no session link.
static APP_NAVIGATION: crate::discovery::AppNavigation = crate::discovery::AppNavigation {
    kind: "deepseek-app",
    url: PerOs {
        macos: Support::Verified("dsh://open"),
        windows: Support::Unsupported("Harness's desktop link is not verified on Windows"),
        linux: Support::Unsupported("Harness's desktop link is not verified on Linux"),
    },
};

/// The questions DeepSeek Harness's web hosts ask over their loopback event
/// stream, and the answers to them. Platform runs it in
/// `platform/src/clients/deepseek/`.
pub static QUESTIONS: Channel = Channel {
    name: "DeepSeek Harness's web question stream",
    carries: &[Carries::Requests],
    support: PerOs {
        macos: Support::Verified(()),
        windows: Support::Unverified(()),
        linux: Support::Unverified(()),
    },
};

static SOURCES: [Source; 1] = [Source {
    reads_as: "deepseek",
    location: Location::home(&["DSH_HOME"], &[".dsh"], "sessions"),
}];
static DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::home(&["DSH_HOME"], &[".dsh"], "profiles"),
        Location::home(&["DSH_HOME"], &[".dsh"], "sessions"),
    ],
    settings: &[Location::home(&["DSH_HOME"], &[".dsh"], "settings.yaml")],
    sessions: &[Location::home(&["DSH_HOME"], &[".dsh"], "sessions")],
    commands: &["dsh"],
    apps: &["DeepSeek", "DeepSeek Harness"],
    app_navigation: Some(&APP_NAVIGATION),
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "deepseek",
    name: "DeepSeek",
    sessions: true,
    hooks: Some(&hooks::HOOKS),
    observer: None,
    channel: Some(&QUESTIONS),
    program: None,
    replies: None,
    turns: Some(&transcript::TURNS),
    items: None,
    sources: crate::everywhere(&SOURCES),
    discovery: &DISCOVERY,
};
