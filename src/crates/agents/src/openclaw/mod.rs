//! OpenClaw.
pub mod transcript;

use crate::Agent;
use crate::Discovery;
use crate::{Location, Source};

static SOURCES: [Source; 1] = [Source {
    reads_as: "openclaw",
    location: Location {
        profiles: Some(".openclaw-"),
        ..Location::home(
            &["OPENCLAW_STATE_DIR"],
            &[".openclaw", ".clawdbot"],
            "agents",
        )
    },
}];
static DISCOVERY: Discovery = Discovery {
    storage: &[Location {
        profiles: Some(".openclaw-"),
        ..Location::home(
            &["OPENCLAW_STATE_DIR"],
            &[".openclaw", ".clawdbot"],
            "agents",
        )
    }],
    settings: &[
        Location {
            profiles: Some(".openclaw-"),
            ..Location::home(
                &["OPENCLAW_STATE_DIR"],
                &[".openclaw", ".clawdbot"],
                "openclaw.json",
            )
        },
        Location {
            profiles: Some(".openclaw-"),
            ..Location::home(
                &["OPENCLAW_STATE_DIR"],
                &[".openclaw", ".clawdbot"],
                "clawdbot.json",
            )
        },
    ],
    sessions: &[Location {
        profiles: Some(".openclaw-"),
        ..Location::home(
            &["OPENCLAW_STATE_DIR"],
            &[".openclaw", ".clawdbot"],
            "agents",
        )
    }],
    commands: &["openclaw", "clawdbot"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "openclaw",
    name: "OpenClaw",
    sessions: true,
    hooks: None,
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&SOURCES),
    discovery: &DISCOVERY,
};
