//! Pi, and Oh My Pi, its fork, which keeps Pi's session logs under a folder of
//! its own and loads Pi's extensions.
use crate::hooks::Observer;
use crate::Discovery;
use crate::{Location, Source};
pub mod transcript;

use crate::Agent;

/// Every client that writes Pi's session logs, Pi first.
pub static FAMILY: [&Agent; 2] = [&AGENT, &OH_MY_PI];

/// Whether `source` writes Pi's session logs.
pub fn family(source: &str) -> bool {
    FAMILY.iter().any(|agent| agent.id == source)
}

/// Whether a log that continues or forks another session starts with a copy
/// of that session's entries, older than its own header: Oh My Pi's, when a
/// session another process holds moves to a sibling file, or is forked.
pub fn copies_history(source: &str) -> bool {
    source == OH_MY_PI.id
}

/// Whether hidden folders among the client's session logs hold copies that
/// are no sessions of their own: Oh My Pi's `.omp-gc-compress-*`, where
/// `omp gc --compress` stages a transcript before it replaces the original.
pub fn hides_copies(source: &str) -> bool {
    source == OH_MY_PI.id
}

static SOURCES: [Source; 1] = [Source {
    reads_as: "pi",
    location: Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "sessions"),
}];
static DISCOVERY: Discovery = Discovery {
    storage: &[Location::home(
        &["PI_CODING_AGENT_DIR"],
        &[".pi/agent"],
        "sessions",
    )],
    settings: &[
        Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "settings.json"),
        Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "models.json"),
        Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "auth.json"),
    ],
    sessions: &[Location::home(
        &["PI_CODING_AGENT_DIR"],
        &[".pi/agent"],
        "sessions",
    )],
    commands: &["pi"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "pi",
    name: "Pi",
    sessions: true,
    hooks: None,
    observer: Some(&OBSERVER),
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&SOURCES),
    discovery: &DISCOVERY,
};

/// Pi's extension records each turn's state as it changes. Oh My Pi loads the
/// same extension, written for its own folder and id (`observer.ts`).
static OBSERVER: Observer = Observer {
    turns: Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "agent-hud/turns"),
    script: Location::home(
        &["PI_CODING_AGENT_DIR"],
        &[".pi/agent"],
        "extensions/agent-hud.ts",
    ),
    marker: "// Agent HUD Pi session observer\n",
    body: concat!(
        "// Agent HUD Pi session observer\n",
        "const SOURCE = \"pi\", NAME = \"Pi\", FOLDER = \".pi\", VARIABLE = \"PI_CODING_AGENT_DIR\", ENDS = \"agent_settled\";\n",
        "const CHANNEL = /*agent-hud:channel*/ null;\n",
        include_str!("../hooks/mailbox.js"),
        include_str!("observer.ts")
    ),
    at_key: "observedAtMs",
    states: true,
};

// Oh My Pi keeps its sessions in `~/.omp/agent/sessions` and loads extensions
// from `~/.omp/agent/extensions` (can1357/oh-my-pi 33ea58a,
// packages/utils/src/dirs.ts, docs/extension-loading.md). It honours Pi's
// `PI_CODING_AGENT_DIR` too, which is left to Pi, so one folder is never read
// as both; its profiles (`~/.omp/profiles/<name>/agent`) and its XDG folders
// (`$XDG_DATA_HOME/omp`, once `omp config init-xdg` moved them) are not read.

static OH_MY_PI_SOURCES: [Source; 1] = [Source {
    reads_as: "omp",
    location: Location::home(&[], &[".omp/agent"], "sessions"),
}];
static OH_MY_PI_DISCOVERY: Discovery = Discovery {
    storage: &[Location::home(&[], &[".omp/agent"], "")],
    settings: &[Location::home(&[], &[".omp/agent"], "agent.db")],
    sessions: &[Location::home(&[], &[".omp/agent"], "sessions")],
    commands: &["omp"],
    ..Discovery::NONE
};

pub static OH_MY_PI: Agent = Agent {
    id: "omp",
    name: "Oh My Pi",
    sessions: true,
    hooks: None,
    observer: Some(&OH_MY_PI_OBSERVER),
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&OH_MY_PI_SOURCES),
    discovery: &OH_MY_PI_DISCOVERY,
};

static OH_MY_PI_OBSERVER: Observer = Observer {
    turns: Location::home(&[], &[".omp/agent"], "agent-hud/turns"),
    script: Location::home(&[], &[".omp/agent"], "extensions/agent-hud.ts"),
    marker: "// Agent HUD Oh My Pi session observer\n",
    body: concat!(
        "// Agent HUD Oh My Pi session observer\n",
        "const SOURCE = \"omp\", NAME = \"Oh My Pi\", FOLDER = \".omp\", VARIABLE = \"\", ENDS = \"agent_end\";\n",
        "const CHANNEL = /*agent-hud:channel*/ null;\n",
        include_str!("../hooks/mailbox.js"),
        include_str!("observer.ts")
    ),
    at_key: "observedAtMs",
    states: true,
};

#[cfg(test)]
mod tests;
