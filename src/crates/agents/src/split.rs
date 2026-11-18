//! An agent's clients, and the vendor each agent belongs to.
//!
//! A command line and a desktop app are separate clients, and an editor
//! extension belongs to its vendor's command line (owner, 2026-11-18). One
//! agent's data (its sessions' `source`, its account, its hooks) can therefore
//! hold several clients, each with its own switches. The agent's own id names
//! the client left over once the others are told apart, so a switch saved
//! under the agent's id keeps its meaning, and a client split off later starts
//! from that switch until it is set itself.
use crate::l10n::tr;
use crate::naming::surfaces::Category;
use crate::{Discovery, Location};

/// One client of an agent: its id, its name in Settings, the agent whose
/// data it shares, and how Agent HUD tells it is on this computer.
pub struct Client {
    pub id: &'static str,
    pub name: &'static str,
    pub agent: &'static str,
    pub discovery: &'static Discovery,
}

pub const CLAUDE_DESKTOP: &str = "claude-desktop";
pub const CODEX_DESKTOP: &str = "codex-desktop";
pub const ANTIGRAVITY_IDE: &str = "antigravity-ide";
pub const ANTIGRAVITY_CLI: &str = "antigravity-cli";

const fn gemini(path: &'static str) -> Location {
    Location::home(&["GEMINI_CLI_HOME"], &[".gemini"], path)
}

static CLAUDE_CLI_FOUND: Discovery = Discovery {
    storage: &[Location::home(&[], &[".local/share/claude"], "versions")],
    commands: &["claude"],
    ..Discovery::NONE
};
static CLAUDE_DESKTOP_FOUND: Discovery = Discovery {
    storage: &[Location::app_data(&["Claude"], "claude-code")],
    apps: &["Claude"],
    ..Discovery::NONE
};
static CODEX_CLI_FOUND: Discovery = Discovery {
    commands: &["codex"],
    ..Discovery::NONE
};
static CODEX_DESKTOP_FOUND: Discovery = Discovery {
    apps: &["Codex", "ChatGPT"],
    bundles: &["com.openai.codex"],
    ..Discovery::NONE
};
static ANTIGRAVITY_DESKTOP_FOUND: Discovery = Discovery {
    storage: &[gemini("antigravity")],
    apps: &["Antigravity"],
    bundles: &["com.google.antigravity"],
    ..Discovery::NONE
};
static ANTIGRAVITY_IDE_FOUND: Discovery = Discovery {
    storage: &[gemini("antigravity-ide")],
    apps: &["Antigravity IDE"],
    ..Discovery::NONE
};
static ANTIGRAVITY_CLI_FOUND: Discovery = Discovery {
    storage: &[gemini("antigravity-cli")],
    commands: &["agy"],
    ..Discovery::NONE
};

/// The agents split into clients, each agent's own id first.
pub static SPLIT: &[&[Client]] = &[
    &[
        Client {
            id: "claude",
            name: "Claude Code CLI",
            agent: "claude",
            discovery: &CLAUDE_CLI_FOUND,
        },
        Client {
            id: CLAUDE_DESKTOP,
            name: "Claude Desktop",
            agent: "claude",
            discovery: &CLAUDE_DESKTOP_FOUND,
        },
    ],
    &[
        Client {
            id: "codex",
            name: "Codex CLI",
            agent: "codex",
            discovery: &CODEX_CLI_FOUND,
        },
        Client {
            id: CODEX_DESKTOP,
            name: "Codex Desktop",
            agent: "codex",
            discovery: &CODEX_DESKTOP_FOUND,
        },
    ],
    &[
        Client {
            id: "antigravity",
            name: "Antigravity Desktop",
            agent: "antigravity",
            discovery: &ANTIGRAVITY_DESKTOP_FOUND,
        },
        Client {
            id: ANTIGRAVITY_IDE,
            name: "Antigravity IDE",
            agent: "antigravity",
            discovery: &ANTIGRAVITY_IDE_FOUND,
        },
        Client {
            id: ANTIGRAVITY_CLI,
            name: "Antigravity CLI",
            agent: "antigravity",
            discovery: &ANTIGRAVITY_CLI_FOUND,
        },
    ],
];

/// The clients an agent is split into; none for an agent that is one client.
pub fn family(agent: &str) -> &'static [Client] {
    SPLIT
        .iter()
        .find(|clients| clients[0].agent == agent)
        .copied()
        .unwrap_or(&[])
}

/// Every client id of an agent: its own id first.
pub fn ids(agent: &str) -> Vec<&str> {
    match family(agent) {
        [] => vec![agent],
        clients => clients.iter().map(|client| client.id).collect(),
    }
}

/// A client split off an agent, by its id (not the agent's own).
pub fn split_off(id: &str) -> Option<&'static Client> {
    SPLIT
        .iter()
        .flat_map(|clients| clients.iter().skip(1))
        .find(|client| client.id == id)
}

/// The agent whose data a client shares: itself for an agent's own id.
pub fn agent_of(id: &str) -> Option<&'static str> {
    split_off(id)
        .map(|client| client.agent)
        .or_else(|| crate::by_id(id).map(|agent| agent.id))
}

/// The client a session of `agent` belongs to, by the client its log names
/// (Claude Code's `entrypoint`, the client `naming::clients::codex_client`
/// names, Antigravity's storage folder).
pub fn of_log<'a>(agent: &'a str, client: Option<&str>) -> &'a str {
    match (agent, client) {
        ("claude", Some("claude-desktop" | "claude-desktop-3p" | "local-agent")) => CLAUDE_DESKTOP,
        ("codex", Some("Desktop")) => CODEX_DESKTOP,
        ("antigravity", Some("IDE")) => ANTIGRAVITY_IDE,
        ("antigravity", Some("CLI" | "CLI · exec")) => ANTIGRAVITY_CLI,
        _ => agent,
    }
}

/// The client a hook's caller is, from the surface its agent's rule found
/// (`process::Origin`); None when the rule could not tell, where only the
/// agent is known.
pub fn of_caller<'a>(
    agent: &'a str,
    category: Option<Category>,
    app: Option<&str>,
) -> Option<&'a str> {
    let category = category?;
    Some(match (agent, category, app) {
        ("claude", Category::Desktop, Some("claude" | "cowork")) => CLAUDE_DESKTOP,
        ("codex", Category::Desktop, Some("codex")) => CODEX_DESKTOP,
        ("antigravity", Category::Ide, _) => ANTIGRAVITY_IDE,
        ("antigravity", Category::Cli | Category::Sdk, _) => ANTIGRAVITY_CLI,
        _ => agent,
    })
}

/// Whether a client is on, from the saved switches (`switch(id)`): its own,
/// else, for a client split off, its agent's, so a client split off after the
/// person turned its agent off stays off. Unset is on.
pub fn is_on(id: &str, switch: &dyn Fn(&str) -> Option<bool>) -> bool {
    switch(id)
        .or_else(|| split_off(id).and_then(|client| switch(client.agent)))
        .unwrap_or(true)
}

/// Whether any client of an agent is on: its data is then read.
pub fn agent_on(agent: &str, switch: &dyn Fn(&str) -> Option<bool>) -> bool {
    ids(agent).into_iter().any(|id| is_on(id, switch))
}

/// The switches to save before `id` changes: each client of its agent that
/// has none yet keeps the value it shows now, so changing one client never
/// moves another.
pub fn settle(id: &str, switch: &dyn Fn(&str) -> Option<bool>) -> Vec<(&'static str, bool)> {
    let Some(agent) = agent_of(id) else {
        return vec![];
    };
    family(agent)
        .iter()
        .filter(|client| client.id != id && switch(client.id).is_none())
        .map(|client| (client.id, is_on(client.id, switch)))
        .collect()
}

/// The vendor an agent belongs to, as the agent list groups its clients: the
/// group's id and name, in the interface language. An agent that is its
/// vendor's only one is a group of its own, named by its own id.
pub fn vendor(agent: &str) -> (String, String) {
    let group = |id: &str, name: String| (id.to_owned(), name);
    match agent {
        "claude" => group("anthropic", "Anthropic".into()),
        "codex" => group("openai", "OpenAI".into()),
        "antigravity" => group("google", "Google".into()),
        // xAI owns Cursor; its accounts can be merged into SuperGrok.
        "cursor" | "grok-cli" | "grok-bot" => group("xai", "xAI".into()),
        "zcode" | "glm" => group("zhipu", tr!("Zhipu AI")),
        "codebuddy" | "codebuddy-cn" | "workbuddy" => group("tencent", tr!("Tencent")),
        "qwen" | "qoder" | "qoder-cn" | "qoder-work" => group("alibaba", tr!("Alibaba")),
        "trae" | "trae-cn" | "ark" => group("bytedance", tr!("ByteDance")),
        "minimax" | "minimax-code" => group("minimax", "MiniMax".into()),
        own => group(own, String::new()),
    }
}

#[cfg(test)]
#[path = "split_tests.rs"]
mod tests;
