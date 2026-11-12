//! Doubao desktop. Quotas come from its own subscription API; the sign-in
//! lives in the local Chromium profile.
pub mod account;

use crate::Agent;
use crate::Discovery;

static DISCOVERY: Discovery = Discovery {
    apps: &["Doubao"],
    bundles: &["com.bot.pc.doubao"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "doubao",
    name: "豆包",
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
