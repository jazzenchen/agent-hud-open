//! Claude Code.
pub mod account;
mod hooks;
pub mod jobs;
pub mod process;
pub mod transcript;

use crate::Discovery;
use crate::{Agent, Replies};
use crate::{Location, Source};

/// The Claude desktop app's questions take their answers as words into the
/// session, through Agent HUD's Claude Code plugin.
pub static REPLIES: Replies = Replies {
    carrier: "Agent HUD's Claude Code plugin",
    setting: "claudeAnswers",
};

static SOURCES: [Source; 3] = [
    Source {
        reads_as: "claude",
        location: Location::home(&[], &[".claude"], "projects"),
    },
    Source {
        reads_as: "claude",
        location: Location::home(&[], &[".config/claude"], "projects"),
    },
    Source {
        reads_as: "claude",
        location: Location::named_by(&["CLAUDE_CONFIG_DIR"], "projects"),
    },
];
static DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::home(&["CLAUDE_CONFIG_DIR"], &[".claude"], ""),
        Location::home(&[], &[".config/claude"], ""),
        Location::app_data(&["Claude"], "claude-code"),
        Location::home(&[], &[".local/share/claude"], "versions"),
    ],
    settings: &[
        Location::home(&["CLAUDE_CONFIG_DIR"], &[".claude"], "settings.json"),
        Location::home(&["CLAUDE_CONFIG_DIR"], &[".claude"], ".credentials.json"),
        Location::home(&[], &[""], ".claude.json"),
    ],
    commands: &["claude"],
    apps: &["Claude"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "claude",
    name: "Claude Code",
    sessions: true,
    hooks: Some(&hooks::HOOKS),
    observer: None,
    channel: None,
    program: Some("claude"),
    replies: Some(&REPLIES),
    turns: Some(&transcript::TURNS),
    items: Some(&transcript::ITEMS),
    sources: crate::everywhere(&SOURCES),
    discovery: &DISCOVERY,
};
