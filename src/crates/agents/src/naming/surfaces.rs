//! Where a person typed a session's prompts, written `category:source`: the
//! kind of interface (`cli`, `desktop`, `ide`…) and the app or device behind
//! it (`cli:iterm`, `desktop:claude`, `mobile:iphone`). A session carries two:
//! where its first prompt was typed (`init`) and where its latest one was
//! (`latest`). What cannot be told apart is `others`, as a category or as a
//! source.
//!
//! Three kinds of evidence name a prompt's surface, most exact first:
//! - a delivery: Agent HUD sent the prompt itself, from a phone, a watch, an
//!   MCP caller or the Mac HUD;
//! - a hook: the client's prompt hook ran in a process whose app is known;
//! - the log: the client wrote which of its own clients took the prompt.
pub use super::apps::{bundle_app, terminal_app};
pub use super::origins::{
    from_reports, resolve, Origins, PromptStamp, PromptStamps, Report, Reports,
};

/// A source or category nobody can tell.
pub const OTHERS: &str = "others";
/// The name Agent HUD's Claude Code plugin writes on the prompts it delivers.
pub const AGENT_HUD_PLUGIN: &str = "agent-hud";
/// Agent HUD's own source: the Mac HUD, or a session Agent HUD runs whose
/// sender it did not record.
pub const AGENT_HUD: &str = "agenthud";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// A vendor's desktop app: Claude, Codex, Cowork, Agent HUD on the Mac.
    Desktop,
    /// An interactive command line in a terminal.
    Cli,
    /// An editor's extension: VS Code, Cursor, Windsurf.
    Ide,
    /// Agent HUD on a phone or a watch.
    Mobile,
    /// Another agent, through Agent HUD's MCP tools.
    Mcp,
    /// A script or program: `claude -p`, the Agent SDK, `codex exec`.
    Sdk,
    Others,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Desktop => "desktop",
            Category::Cli => "cli",
            Category::Ide => "ide",
            Category::Mobile => "mobile",
            Category::Mcp => "mcp",
            Category::Sdk => "sdk",
            Category::Others => OTHERS,
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "desktop" => Category::Desktop,
            "cli" => Category::Cli,
            "ide" => Category::Ide,
            "mobile" => Category::Mobile,
            "mcp" => Category::Mcp,
            "sdk" => Category::Sdk,
            OTHERS => Category::Others,
            _ => return None,
        })
    }
}

/// A category and the app or device behind it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Surface {
    pub category: Category,
    pub source: String,
}

impl Surface {
    /// `source` as a tag word: lower case, letters, digits, `-` and `.`; an
    /// empty one is `others`.
    pub fn new(category: Category, source: &str) -> Self {
        let mut word = String::new();
        for c in source.trim().chars().flat_map(char::to_lowercase) {
            if c.is_ascii_alphanumeric() || c == '.' {
                word.push(c);
            } else if !word.is_empty() && !word.ends_with('-') {
                word.push('-');
            }
        }
        let word = word.trim_end_matches('-');
        Self {
            category,
            source: if word.is_empty() {
                OTHERS.into()
            } else {
                word.chars().take(40).collect()
            },
        }
    }
    pub fn tag(&self) -> String {
        format!("{}:{}", self.category.as_str(), self.source)
    }
    /// A tag as `tag` writes it; a word that names no category is not one.
    pub fn parse(tag: &str) -> Option<Self> {
        let (category, source) = tag.split_once(':')?;
        Some(Self::new(Category::parse(category)?, source))
    }
}

/// The category a client its log or hook names stands for, and the source
/// when the client alone tells it. `client` is Claude Code's `entrypoint`, or
/// the Codex client `naming::clients::codex_client` names.
pub fn client_category(agent: &str, client: &str) -> (Category, Option<String>) {
    let fixed = |category, source: &str| (category, Some(source.to_owned()));
    match (agent, client) {
        (_, "agent-hud" | "agent_hud") => fixed(Category::Desktop, AGENT_HUD),
        ("claude", "cli") => (Category::Cli, None),
        ("claude", "claude-desktop" | "claude-desktop-3p") => fixed(Category::Desktop, "claude"),
        ("claude", "local-agent") => fixed(Category::Desktop, "cowork"),
        ("claude", "claude-vscode") => fixed(Category::Ide, "vscode"),
        ("claude", sdk) if sdk.starts_with("sdk-") => (Category::Sdk, None),
        ("codex", "Desktop") => fixed(Category::Desktop, "codex"),
        ("codex", "CLI") => (Category::Cli, None),
        ("codex", "CLI · exec") => (Category::Sdk, None),
        ("codex", "IDE") => fixed(Category::Ide, "vscode"),
        ("antigravity", "Desktop") => fixed(Category::Desktop, "antigravity"),
        ("antigravity", "CLI") => (Category::Cli, None),
        ("antigravity", "CLI · exec") => (Category::Sdk, None),
        ("antigravity", "IDE") => fixed(Category::Ide, "antigravity"),
        (_, other) => fixed(Category::Others, other),
    }
}

/// The surface a client that writes no client of its own runs in.
fn agent_category(agent: &str) -> (Category, Option<&'static str>) {
    match agent {
        "antigravity" => (Category::Ide, Some("antigravity")),
        "cursor" => (Category::Ide, Some("cursor")),
        "qoder" | "qoder-cn" => (Category::Ide, Some("qoder")),
        "qoder-work" => (Category::Desktop, Some("qoderwork")),
        "trae" | "trae-cn" => (Category::Ide, Some("trae")),
        "workbuddy" => (Category::Desktop, Some("workbuddy")),
        "teleagent" => (Category::Desktop, Some("teleagent")),
        "grok-cli" | "hermes" | "kimi" | "opencode" | "pi" | "qwen" | "kilo" | "mimocode"
        | "omp" => (Category::Cli, None),
        _ => (Category::Others, None),
    }
}

/// What a session's log says about the surface of one prompt: the client it
/// names and, for a prompt a Claude Code plugin submitted, the plugin.
pub fn from_log(agent: &str, client: Option<&str>, plugin: Option<&str>) -> Surface {
    if let Some(plugin) = plugin.filter(|p| !p.is_empty()) {
        return if plugin == AGENT_HUD_PLUGIN {
            Surface::new(Category::Desktop, AGENT_HUD)
        } else {
            Surface::new(Category::Others, plugin)
        };
    }
    if let Some(client) = client.filter(|c| !c.is_empty()) {
        let (category, source) = client_category(agent, client);
        return Surface::new(category, source.as_deref().unwrap_or(OTHERS));
    }
    match agent_category(agent) {
        (Category::Others, _) => Surface::new(Category::Others, agent),
        (category, source) => Surface::new(category, source.unwrap_or(OTHERS)),
    }
}

#[cfg(test)]
#[path = "surfaces_tests.rs"]
mod tests;
