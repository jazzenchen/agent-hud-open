//! OpenCode, and its forks Kilo Code and MiMo Code, which keep OpenCode's
//! database under folders of their own and load OpenCode's plugins.
use crate::hooks::Observer;
use crate::Discovery;
use crate::{Base, Location, Profiles, Source};
pub mod legacy;
pub mod transcript;

use crate::Agent;

/// A client that keeps its sessions in OpenCode's database: OpenCode or one
/// of its forks. Its sessions' ids and keys start with its id.
pub struct Store {
    pub agent: &'static Agent,
    /// The database's file name in the client's data folder.
    pub database: &'static str,
    /// Whether its JSON message files from before the database are read too.
    pub legacy: bool,
    /// Sessions whose ids start with this are copies the client made of
    /// another client's tasks, which are read where they were made: Kilo
    /// Code's tasks moved in from its VS Code extension, with no tokens.
    pub copies: Option<&'static str>,
    /// Tables naming the messages the client imported from other clients
    /// (their `message_ids`, else the whole `session_id`), which are read
    /// where they were made: MiMo Code's imports from Claude Code, Codex and
    /// OpenCode.
    pub imports: &'static [&'static str],
}

/// Every client of OpenCode's database, OpenCode first.
pub static STORES: [&Store; 4] = [&STORE, &KILO_STORE, &MIMO_CODE_STORE, &TELEAGENT_STORE];

/// The client of OpenCode's database with this id.
pub fn store(source: &str) -> Option<&'static Store> {
    STORES
        .iter()
        .copied()
        .find(|store| store.agent.id == source)
}

static STORE: Store = Store {
    agent: &AGENT,
    database: "opencode.db",
    legacy: true,
    copies: None,
    imports: &[],
};

static SOURCES: [Source; 2] = [
    Source {
        reads_as: "opencode",
        location: Location::data_home(&["opencode"], ""),
    },
    Source {
        reads_as: "opencode",
        location: Location::data_home(&["opencode"], "storage/message"),
    },
];
static DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::data_home(&["opencode"], ""),
        Location::config_home(&["opencode"], ""),
    ],
    settings: &[
        Location::config_home(&["opencode"], "opencode.json"),
        Location::config_home(&["opencode"], "opencode.jsonc"),
        Location::data_home(&["opencode"], "auth.json"),
    ],
    sessions: &[
        Location::data_home(&["opencode"], "storage/session"),
        Location::data_home(&["opencode"], "opencode.db"),
    ],
    commands: &["opencode"],
    credentials_env: &["OPENCODE_GO_API_KEY"],
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "opencode",
    name: "OpenCode",
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

/// OpenCode's plugin records each turn when it completes (`observer.js`).
/// OpenCode 1 and OpenCode 2 both load every script in `plugin/` and
/// `plugins/`, so one script serves both, exporting the plugin of the
/// version that loads it (`export.js`). Its forks load OpenCode 1's plugin,
/// written for their own folders and ids.
static OBSERVER: Observer = Observer {
    turns: Location::data_home(&["opencode"], "agent-hud/turns"),
    script: Location::config_home(&["opencode"], "plugin/agent-hud.js"),
    marker: "// Agent HUD OpenCode session observer\n",
    body: concat!(
        "// Agent HUD OpenCode session observer\n",
        "const SOURCE = \"opencode\", FOLDER = \"opencode\";\n",
        "const CHANNEL = /*agent-hud:channel*/ null;\n",
        include_str!("../hooks/mailbox.js"),
        include_str!("observer.js"),
        include_str!("export.js")
    ),
    at_key: "completedAtMs",
    states: false,
};

// Kilo Code keeps `kilo.db` in `$XDG_DATA_HOME/kilo` and loads plugins from
// `$XDG_CONFIG_HOME/kilo/{plugin,plugins}` (Kilo-Org/kilocode 080ca6c,
// packages/core/src/global.ts, packages/opencode/src/storage/db.ts and
// config/plugin.ts). Its older JSON files were copied into the database once
// and are not read again; its VS Code extension moves its older tasks in
// (packages/kilo-vscode/src/legacy-migration).

static KILO_STORE: Store = Store {
    agent: &KILO,
    database: "kilo.db",
    legacy: false,
    copies: Some("ses_migrated_"),
    imports: &[],
};

static KILO_SOURCES: [Source; 1] = [Source {
    reads_as: "kilo",
    location: Location::data_home(&["kilo"], ""),
}];
static KILO_DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::data_home(&["kilo"], ""),
        Location::config_home(&["kilo"], ""),
    ],
    settings: &[
        Location::config_home(&["kilo"], "kilo.jsonc"),
        Location::config_home(&["kilo"], "kilo.json"),
        Location::config_home(&["kilo"], "opencode.jsonc"),
        Location::config_home(&["kilo"], "opencode.json"),
        Location::data_home(&["kilo"], "auth.json"),
    ],
    sessions: &[Location::data_home(&["kilo"], "kilo.db")],
    commands: &["kilo", "kilocode"],
    ..Discovery::NONE
};

pub static KILO: Agent = Agent {
    id: "kilo",
    name: "Kilo Code",
    sessions: true,
    hooks: None,
    observer: Some(&KILO_OBSERVER),
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&KILO_SOURCES),
    discovery: &KILO_DISCOVERY,
};

static KILO_OBSERVER: Observer = Observer {
    turns: Location::data_home(&["kilo"], "agent-hud/turns"),
    script: Location::config_home(&["kilo"], "plugins/agent-hud.js"),
    marker: "// Agent HUD Kilo Code session observer\n",
    body: concat!(
        "// Agent HUD Kilo Code session observer\n",
        "const SOURCE = \"kilo\", FOLDER = \"kilo\";\n",
        "const CHANNEL = /*agent-hud:channel*/ null;\n",
        include_str!("../hooks/mailbox.js"),
        include_str!("observer.js"),
        "export const AgentHUDSessionObserver = server;\n"
    ),
    at_key: "completedAtMs",
    states: false,
};

// MiMo Code: its CLI keeps `mimocode.db` in `$XDG_DATA_HOME/mimocode` and
// loads plugins from `$XDG_CONFIG_HOME/mimocode/{plugin,plugins}`
// (XiaomiMiMo/MiMo-Code 6b66e8c, packages/shared/src/global.ts,
// packages/cli/src/storage/db.ts and config/paths.ts). `MIMOCODE_HOME` moves
// both under `<home>/data` and `<home>/config`, which is not read. Its
// desktop app, which tokscale reads from the same database, loads no plugins
// of the user's (packages/cli/src/plugin/host.ts).

static MIMO_CODE_STORE: Store = Store {
    agent: &MIMO_CODE,
    database: "mimocode.db",
    legacy: false,
    copies: None,
    imports: &["external_import", "claude_import"],
};

static MIMO_CODE_SOURCES: [Source; 1] = [Source {
    reads_as: "mimocode",
    location: Location::data_home(&["mimocode"], ""),
}];
static MIMO_CODE_DISCOVERY: Discovery = Discovery {
    storage: &[
        Location::data_home(&["mimocode"], ""),
        Location::config_home(&["mimocode"], ""),
    ],
    settings: &[
        Location::config_home(&["mimocode"], "mimocode.jsonc"),
        Location::config_home(&["mimocode"], "mimocode.json"),
    ],
    sessions: &[Location::data_home(&["mimocode"], "mimocode.db")],
    commands: &["mimo"],
    ..Discovery::NONE
};

pub static MIMO_CODE: Agent = Agent {
    id: "mimocode",
    name: "MiMo Code",
    sessions: true,
    hooks: None,
    observer: Some(&MIMO_CODE_OBSERVER),
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&MIMO_CODE_SOURCES),
    discovery: &MIMO_CODE_DISCOVERY,
};

static MIMO_CODE_OBSERVER: Observer = Observer {
    turns: Location::data_home(&["mimocode"], "agent-hud/turns"),
    script: Location::config_home(&["mimocode"], "plugins/agent-hud.js"),
    marker: "// Agent HUD MiMo Code session observer\n",
    body: concat!(
        "// Agent HUD MiMo Code session observer\n",
        "const SOURCE = \"mimocode\", FOLDER = \"mimocode\";\n",
        "const CHANNEL = /*agent-hud:channel*/ null;\n",
        include_str!("../hooks/mailbox.js"),
        include_str!("observer.js"),
        "export const AgentHUDSessionObserver = server;\n"
    ),
    at_key: "completedAtMs",
    states: false,
};

// TeleAgent 3.0 (desktop app com.teleai.superagent) stores sessions in an
// OpenCode database: `$XDG_DATA_HOME/TeleAgent/users/<owner>/teleagent.db`.
// Each account has its own database, with the same tables and message JSON
// as OpenCode. It does not load the OpenCode plugin, so there is no observer.
const fn teleagent_place(path: &'static str) -> Location {
    Location {
        base: Base::DataHome,
        env: &[],
        folders: &["TeleAgent"],
        profiles: Some(Profiles::Inside("users")),
        path,
        not_if: None,
    }
}

static TELEAGENT_STORE: Store = Store {
    agent: &TELEAGENT,
    database: "teleagent.db",
    legacy: false,
    copies: None,
    imports: &[],
};

static TELEAGENT_SOURCES: [Source; 1] = [Source {
    reads_as: "teleagent",
    location: teleagent_place(""),
}];
static TELEAGENT_DISCOVERY: Discovery = Discovery {
    storage: &[teleagent_place("")],
    settings: &[Location::config_home(&["teleagent"], "")],
    sessions: &[teleagent_place("")],
    usage_files: &[teleagent_place("teleagent.db")],
    apps: &["TeleAgent"],
    bundles: &["com.teleai.superagent"],
    ..Discovery::NONE
};

pub static TELEAGENT: Agent = Agent {
    id: "teleagent",
    name: "TeleAgent",
    sessions: true,
    hooks: None,
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&TELEAGENT_SOURCES),
    discovery: &TELEAGENT_DISCOVERY,
};

#[cfg(test)]
mod tests;
