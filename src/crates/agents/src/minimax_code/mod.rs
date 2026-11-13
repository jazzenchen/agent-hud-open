//! MiniMax Code (`id: "minimax-code"`), MiniMax's standalone desktop coding
//! client. Kept separate from the API-key MiniMax Coding Plan (`minimax`).
pub mod account;

use crate::{Agent, Discovery, Location};

static DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::home(
            &["MINIMAX_DATA_DIR", "MAVIS_DATA_DIR"],
            &[".minimax"],
            "auth/prod/cn/mcode-public/auth.json",
        ),
        Location::home(
            &["MINIMAX_DATA_DIR", "MAVIS_DATA_DIR"],
            &[".minimax"],
            "auth/prod/en/mcode-public/auth.json",
        ),
        Location::home(
            &["MINIMAX_DATA_DIR", "MAVIS_DATA_DIR"],
            &[".minimax"],
            "auth",
        ),
    ],
    commands: &["minimax-code", "mcode"],
    apps: &["MiniMax Code", "MCode"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "minimax-code",
    name: "MiniMax Code",
    sessions: false,
    hooks: None,
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&[]),
    discovery: &DISCOVERY,
};
