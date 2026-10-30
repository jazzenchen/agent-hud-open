//! Cursor: sessions from its own database, tokens from its `stop` hook.
pub mod account;
mod hooks;
pub mod sessions;

use crate::{Agent, Base, Discovery, Location, PerOs, Source, Support};

/// Cursor's session headers, in its state database.
const HEADERS: Source = Source {
    reads_as: "cursor",
    location: account::STATE,
};
/// The turn log the `stop` hook appends in Agent HUD's own data folder
/// (`hooks::turn_log`): `folder`, named after the app's identifier on that
/// system, or the one `AGENTHUD_DATA_DIR` names. Cursor keeps a turn's tokens
/// nowhere else on disk. Its hooks run on macOS and Linux only.
const fn turn_log(base: Base, folder: &'static [&'static str]) -> Source {
    Source {
        reads_as: "cursor",
        location: Location {
            base,
            env: &["AGENTHUD_DATA_DIR"],
            folders: folder,
            profiles: None,
            path: "clients/cursor",
            not_if: None,
        },
    }
}
static MAC_SOURCES: [Source; 2] = [HEADERS, turn_log(Base::AppData, &["app.agenthud.mac"])];
static WINDOWS_SOURCES: [Source; 1] = [HEADERS];
static LINUX_SOURCES: [Source; 2] = [HEADERS, turn_log(Base::DataHome, &["app.agenthud.linux"])];

static DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::app_data(&["Cursor"], ""),
        Location::home(&[], &[".cursor"], ""),
    ],
    settings: &[
        Location::app_data(&["Cursor"], "User/settings.json"),
        Location::app_data(&["Cursor"], "User/globalStorage/state.vscdb"),
    ],
    sessions: &[
        Location::home(&[], &[".cursor"], "chats"),
        Location::app_data(&["Cursor"], "User/workspaceStorage"),
    ],
    commands: &["cursor", "cursor-agent"],
    apps: &["Cursor"],
    bundles: &["com.todesktop.230313mzl4w4u92"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "cursor",
    name: "Cursor",
    sessions: true,
    hooks: Some(&hooks::HOOKS),
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: PerOs {
        macos: Support::Verified(&MAC_SOURCES),
        windows: Support::Unverified(&WINDOWS_SOURCES),
        linux: Support::Unverified(&LINUX_SOURCES),
    },
    discovery: &DISCOVERY,
};
