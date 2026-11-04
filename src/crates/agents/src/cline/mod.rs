//! Cline and Roo Code. Roo Code began as a fork of Cline's VS Code extension
//! and keeps its task format, so both are read here, but they are two
//! products with their own names, as Qoder and QoderWork are: two clients.
//!
//! - Cline 4.0 (June 2026) moved its VS Code extension onto the SDK its CLI
//!   runs on. Both keep each session in `~/.cline/data/sessions/<id>/`: a
//!   manifest `<id>.json` and the messages `<id>.messages.json`, and a
//!   sub-agent's messages beside them (`<agent>.messages.json`).
//! - Cline 3 kept a task in the editor's `globalStorage/saoudrizwan.claude-dev/
//!   tasks/<id>/ui_messages.json`; JetBrains and the first CLI kept the same in
//!   `~/.cline/data/tasks/`. Cline 4 reads those tasks and writes none.
//! - Roo Code keeps the same task files in `globalStorage/
//!   rooveterinaryinc.roo-cline/tasks/`. Its team stopped work on it on
//!   2026-05-15; an installed copy still runs.
//!
//! Kilo Code's first extension wrote the same task files too, but Kilo Code
//! 7 is a new client built on OpenCode's, which copies those tasks into its
//! own database: they belong to that client, which would count them twice
//! if they were read here as well.
pub mod account;
mod session;
mod task;
pub mod transcript;

use crate::{Agent, Discovery, Location, PerOs, Source, Support};

/// `$make!(editor, path)` for each editor whose application data folder
/// holds an extension's `globalStorage`, by the folder's name: VS Code and
/// its Insiders build, VSCodium, and the editors built on them.
macro_rules! in_editors {
    ($make:ident, $path:literal) => {
        [
            $make!("Code", $path),
            $make!("Code - Insiders", $path),
            $make!("VSCodium", $path),
            $make!("Cursor", $path),
            $make!("Windsurf", $path),
            $make!("Positron", $path),
            $make!("Antigravity", $path),
        ]
    };
}
macro_rules! folder {
    ($editor:literal, $path:literal) => {
        Location::app_data(&[$editor], $path)
    };
}

/// Cline's own folder, `~/.cline` unless `CLINE_DIR` names another.
const fn cline_dir(path: &'static str) -> Location {
    Location::home(&["CLINE_DIR"], &[".cline"], path)
}
const fn cline(location: Location) -> Source {
    Source {
        reads_as: "cline",
        location,
    }
}
const fn roo(location: Location) -> Source {
    Source {
        reads_as: "roo-code",
        location,
    }
}

static CLINE_SOURCES: [Source; 11] = {
    let tasks = in_editors!(folder, "User/globalStorage/saoudrizwan.claude-dev/tasks");
    [
        cline(cline_dir("data/sessions")),
        // `CLINE_DATA_DIR` and `CLINE_SESSION_DATA_DIR` move the data folder
        // and the sessions folder.
        cline(Location::named_by(&["CLINE_DATA_DIR"], "sessions")),
        cline(Location::named_by(&["CLINE_SESSION_DATA_DIR"], "")),
        cline(cline_dir("data/tasks")),
        cline(tasks[0]),
        cline(tasks[1]),
        cline(tasks[2]),
        cline(tasks[3]),
        cline(tasks[4]),
        cline(tasks[5]),
        cline(tasks[6]),
    ]
};
static ROO_SOURCES: [Source; 7] = {
    let tasks = in_editors!(
        folder,
        "User/globalStorage/rooveterinaryinc.roo-cline/tasks"
    );
    [
        roo(tasks[0]),
        roo(tasks[1]),
        roo(tasks[2]),
        roo(tasks[3]),
        roo(tasks[4]),
        roo(tasks[5]),
        roo(tasks[6]),
    ]
};

/// Not checked against a running Cline or Roo Code on any system: the
/// layouts come from their sources and every test sample is made up.
const fn unverified(sources: &'static [Source]) -> PerOs<&'static [Source]> {
    PerOs {
        macos: Support::Unverified(sources),
        windows: Support::Unverified(sources),
        linux: Support::Unverified(sources),
    }
}

static CLINE_STORAGE: [Location; 8] = {
    let editors = in_editors!(folder, "User/globalStorage/saoudrizwan.claude-dev");
    [
        cline_dir(""),
        editors[0],
        editors[1],
        editors[2],
        editors[3],
        editors[4],
        editors[5],
        editors[6],
    ]
};
static CLINE_DISCOVERY: Discovery = Discovery {
    storage: &CLINE_STORAGE,
    settings: &[account::PROVIDERS, account::PROVIDERS_MOVED],
    sessions: &[cline_dir("data/sessions")],
    commands: &["cline"],
    credentials_env: &["CLINE_API_KEY"],
    ..Discovery::NONE
};
static ROO_STORAGE: [Location; 7] =
    in_editors!(folder, "User/globalStorage/rooveterinaryinc.roo-cline");
static ROO_DISCOVERY: Discovery = Discovery {
    storage: &ROO_STORAGE,
    ..Discovery::NONE
};

pub static CLINE: Agent = Agent {
    id: "cline",
    name: "Cline",
    sessions: true,
    // Cline's hooks can only cancel a step: nothing in them for the HUD to
    // show or answer.
    hooks: None,
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: unverified(&CLINE_SOURCES),
    discovery: &CLINE_DISCOVERY,
};

pub static ROO_CODE: Agent = Agent {
    id: "roo-code",
    name: "Roo Code",
    sessions: true,
    hooks: None,
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: unverified(&ROO_SOURCES),
    discovery: &ROO_DISCOVERY,
};
