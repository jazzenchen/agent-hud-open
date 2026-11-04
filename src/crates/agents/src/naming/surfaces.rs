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
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A source or category nobody can tell.
pub const OTHERS: &str = "others";
/// The name Agent HUD's Claude Code plugin writes on the prompts it delivers.
pub const AGENT_HUD_PLUGIN: &str = "agent-hud";
/// Agent HUD's own source: the Mac HUD, or a session Agent HUD runs whose
/// sender it did not record.
pub const AGENT_HUD: &str = "agenthud";

/// How long before a prompt a hook or delivery may be, or after it, and still
/// be about that prompt: a hook runs just before the client writes the
/// prompt, a delivery reaches the log a moment after it is sent.
const SLACK_MS: i64 = 30_000;
/// How long a prompt Agent HUD's plugin delivered may wait for the running
/// turn to end, so the delivery that sent it is still its sender.
const QUEUED_MS: i64 = 3_600_000;
/// How long after a delivery an unmarked prompt is still taken as that
/// delivery (Codex writes no mark on a prompt Agent HUD sends).
const UNMARKED_MS: i64 = 120_000;
/// How long after Agent HUD started a session its first prompt may come.
const START_MS: i64 = 600_000;

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

/// The app a bundle identifier names, its helpers' identifiers too
/// (`com.microsoft.VSCode.helper`).
pub fn bundle_app(bundle: &str) -> Option<&'static str> {
    const APPS: &[(&str, &str)] = &[
        ("com.apple.Terminal", "terminal"),
        ("com.googlecode.iterm2", "iterm"),
        ("com.mitchellh.ghostty", "ghostty"),
        ("dev.warp.Warp-Stable", "warp"),
        ("dev.warp.Warp-Preview", "warp"),
        ("dev.warp.Warp", "warp"),
        ("com.github.wez.wezterm", "wezterm"),
        ("net.kovidgoyal.kitty", "kitty"),
        ("org.alacritty", "alacritty"),
        ("io.alacritty", "alacritty"),
        ("co.zeit.hyper", "hyper"),
        ("org.tabby", "tabby"),
        ("com.raphaelamorim.rio", "rio"),
        ("com.microsoft.VSCodeInsiders", "vscode"),
        ("com.microsoft.VSCode", "vscode"),
        ("com.vscodium", "vscodium"),
        ("com.todesktop.230313mzl4w4u92", "cursor"),
        ("com.exafunction.windsurf", "windsurf"),
        ("dev.zed.Zed-Preview", "zed"),
        ("dev.zed.Zed", "zed"),
        ("com.apple.dt.Xcode", "xcode"),
        ("com.anthropic.claudefordesktop", "claude"),
        ("com.openai.codex", "codex"),
        ("com.google.antigravity", "antigravity"),
        (crate::trae::process::BUNDLES[0], "trae"),
        (crate::trae::process::BUNDLES[1], "trae"),
        ("app.agenthud", AGENT_HUD),
        ("com.agenthud", AGENT_HUD),
    ];
    let within = |id: &str| {
        bundle == id
            || bundle
                .strip_prefix(id)
                .is_some_and(|rest| rest.starts_with('.'))
    };
    if within("com.jetbrains") || within("com.google.android.studio") {
        return Some("jetbrains");
    }
    APPS.iter().find(|(id, _)| within(id)).map(|(_, app)| *app)
}

/// The terminal a `TERM_PROGRAM` value names.
pub fn terminal_app(program: &str) -> Option<&'static str> {
    Some(match program {
        "Apple_Terminal" => "terminal",
        "iTerm.app" => "iterm",
        "ghostty" => "ghostty",
        "WarpTerminal" => "warp",
        "WezTerm" => "wezterm",
        "vscode" => "vscode",
        "Hyper" => "hyper",
        "Tabby" => "tabby",
        "zed" => "zed",
        "kitty" => "kitty",
        "rio" => "rio",
        "tmux" => "tmux",
        _ => return None,
    })
}

/// What a hook or a delivery reported for a session: its surface tag and when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub surface: Surface,
    pub at: i64,
}

impl Report {
    /// The report a hook record's `data` holds: its `surface` at `at`, or
    /// the earliest one it carried in `first`.
    pub fn from_record(data: &Value, at: i64, first: bool) -> Option<Self> {
        let (surface, at) = if first {
            let first = &data["first"];
            if first.is_object() {
                (&first["surface"], first["at"].as_i64()?)
            } else {
                (&data["surface"], at)
            }
        } else {
            (&data["surface"], at)
        };
        Some(Self {
            surface: Surface::parse(surface.as_str()?)?,
            at,
        })
    }
}

/// The hook and delivery evidence a session has: the latest of each and the
/// first each recorded.
#[derive(Clone, Debug, Default)]
pub struct Reports {
    pub hook: Option<Report>,
    pub first_hook: Option<Report>,
    pub delivery: Option<Report>,
    pub first_delivery: Option<Report>,
}

/// One prompt as the log has it: when, the client that wrote it, and the
/// Claude Code plugin that submitted it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptStamp {
    #[serde(default)]
    pub at: Option<i64>,
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub plugin: Option<String>,
}

/// A session's first and latest prompts as its log has them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptStamps {
    /// Whether this log's prompts were stamped from its start. A log read
    /// before stamps existed takes its client then as its first prompt's.
    #[serde(default)]
    pub tracked: bool,
    #[serde(default)]
    pub first: Option<PromptStamp>,
    #[serde(default)]
    pub last: Option<PromptStamp>,
}

impl PromptStamps {
    /// Called before each line is read: a log first stamped part-way through
    /// keeps the client it had then (and when it started) as its first.
    pub fn begin(&mut self, read_before: bool, client: Option<&str>, started_at: Option<i64>) {
        if self.tracked {
            return;
        }
        self.tracked = true;
        if read_before && self.first.is_none() {
            self.first = Some(PromptStamp {
                at: started_at,
                client: client.map(str::to_owned),
                plugin: None,
            });
        }
    }
    pub fn prompt(&mut self, at: Option<i64>, client: Option<&str>, plugin: Option<&str>) {
        let stamp = PromptStamp {
            at,
            client: client.map(str::to_owned),
            plugin: plugin.map(str::to_owned),
        };
        if self.first.is_none() {
            self.first = Some(stamp.clone());
        }
        self.last = Some(stamp);
    }
}

/// A session's two tags.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Origins {
    pub init: Option<Surface>,
    pub latest: Option<Surface>,
}

impl Origins {
    pub fn json(&self) -> Value {
        json!({
            "init": self.init.as_ref().map(Surface::tag),
            "latest": self.latest.as_ref().map(Surface::tag),
        })
    }
}

/// Whether a prompt with this log surface came through Agent HUD.
fn through_agent_hud(surface: &Surface) -> bool {
    surface.category == Category::Desktop && surface.source == AGENT_HUD
}

/// The surface of one prompt: a delivery that sent it, else a hook that ran
/// for it, else what the log says. `first` reads a session's first prompt,
/// where a hook may come any time before it (the session started earlier).
fn prompt_surface(
    agent: &str,
    stamp: &PromptStamp,
    hook: Option<&Report>,
    delivery: Option<&Report>,
    first: bool,
) -> Surface {
    let logged = from_log(agent, stamp.client.as_deref(), stamp.plugin.as_deref());
    let Some(at) = stamp.at else {
        return logged;
    };
    let window = if first {
        Some(START_MS)
    } else if through_agent_hud(&logged) {
        Some(QUEUED_MS)
    } else if agent == "claude" {
        // Agent HUD reaches Claude Code only through its plugin, whose
        // prompts the log marks: an unmarked prompt was typed.
        None
    } else {
        Some(UNMARKED_MS)
    };
    if let Some(delivery) = delivery
        .filter(|d| window.is_some_and(|window| d.at <= at + SLACK_MS && at - d.at <= window))
    {
        return delivery.surface.clone();
    }
    // A plugin's prompt was not typed where the session's hook runs.
    if stamp.plugin.is_none() {
        if let Some(hook) =
            hook.filter(|h| h.at <= at + SLACK_MS && (first || at - h.at <= SLACK_MS))
        {
            return hook.surface.clone();
        }
    }
    logged
}

/// A session's two tags from its log's prompts and its reports. `client` is
/// the client the log names now, for a log stamped before prompts were or
/// one with no prompt marks; a client that names no client of its own runs
/// in its one interface.
pub fn resolve(
    agent: &str,
    prompts: &PromptStamps,
    client: Option<&str>,
    reports: &Reports,
) -> Origins {
    let fallback = || Some(from_log(agent, client, None));
    let init = match &prompts.first {
        Some(stamp) => Some(prompt_surface(
            agent,
            stamp,
            reports.first_hook.as_ref(),
            reports.first_delivery.as_ref(),
            true,
        )),
        None => fallback(),
    };
    let latest = match &prompts.last {
        Some(stamp) => Some(prompt_surface(
            agent,
            stamp,
            reports.hook.as_ref(),
            reports.delivery.as_ref(),
            false,
        )),
        None => init.clone(),
    };
    Origins { init, latest }
}

/// The tags of a session only hooks report, with no log read yet.
pub fn from_reports(reports: &Reports) -> Origins {
    let latest = reports
        .delivery
        .iter()
        .chain(&reports.hook)
        .max_by_key(|r| r.at)
        .map(|r| r.surface.clone());
    let init = reports
        .first_delivery
        .iter()
        .chain(&reports.first_hook)
        .min_by_key(|r| r.at)
        .map(|r| r.surface.clone());
    Origins { init, latest }
}

#[cfg(test)]
#[path = "surfaces_tests.rs"]
mod tests;
