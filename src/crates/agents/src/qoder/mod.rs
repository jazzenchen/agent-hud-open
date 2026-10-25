//! Qoder and its CN and Work builds, which share one hook format. Their
//! sessions are not read.
mod hooks;

use crate::Agent;
use crate::{Discovery, Location};

static QODER_DISCOVERY: Discovery = Discovery {
    storage: &[Location::home(&[], &[".qoder"], "")],
    settings: &[Location::home(&[], &[".qoder"], "settings.json")],
    commands: &["qoder", "qodercli"],
    apps: &["Qoder"],
    ..Discovery::NONE
};
static QODER_CN_DISCOVERY: Discovery = Discovery {
    storage: &[Location::home(&[], &[".qoder-cn"], "")],
    settings: &[Location::home(&[], &[".qoder-cn"], "settings.json")],
    commands: &["qoder-cn"],
    apps: &["Qoder CN"],
    ..Discovery::NONE
};
static QODER_WORK_DISCOVERY: Discovery = Discovery {
    storage: &[Location::home(&[], &[".qoderwork"], "")],
    settings: &[Location::home(&[], &[".qoderwork"], "settings.json")],
    commands: &["qoderwork"],
    apps: &["QoderWork"],
    ..Discovery::NONE
};

pub static QODER: Agent = Agent {
    id: "qoder",
    name: "Qoder",
    sessions: false,
    hooks: Some(&hooks::QODER),
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&[]),
    discovery: &QODER_DISCOVERY,
};

pub static QODER_CN: Agent = Agent {
    id: "qoder-cn",
    name: "Qoder CN",
    sessions: false,
    hooks: Some(&hooks::QODER_CN),
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&[]),
    discovery: &QODER_CN_DISCOVERY,
};

pub static QODER_WORK: Agent = Agent {
    id: "qoder-work",
    name: "QoderWork",
    sessions: false,
    hooks: Some(&hooks::QODER_WORK),
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&[]),
    discovery: &QODER_WORK_DISCOVERY,
};
