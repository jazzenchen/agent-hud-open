//! Volcengine Ark (火山方舟). Its Coding Plan and Agent Plan are configured
//! billing providers, not installed execution clients: other clients send
//! their calls to Ark, and the plans' quota is read through Volcengine's own
//! command-line program, `arkcli`, which owns the sign-in.
pub mod account;

use crate::Agent;
use crate::Discovery;

static DISCOVERY: Discovery = Discovery {
    commands: &["arkcli"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "ark",
    name: "Volcengine Ark",
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
