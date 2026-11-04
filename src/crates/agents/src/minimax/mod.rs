//! MiniMax. MiniMax's M Plan (before it, the Token Plan and the Coding
//! Plan) is a configured billing provider, not an installed execution
//! client: Claude Code, Codex, OpenCode and Pi send their calls to it with a
//! plan key, and its quota is read with that key.
pub mod account;
mod routes;

use crate::Agent;
use crate::Discovery;

static DISCOVERY: Discovery = Discovery {
    credentials_env: routes::VARIABLES,
    configured: Some(routes::configured),
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "minimax",
    name: "MiniMax",
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
