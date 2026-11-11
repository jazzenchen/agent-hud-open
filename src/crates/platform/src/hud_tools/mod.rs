//! HUD tools in each agent client: Agent HUD's MCP server, registered where
//! that client looks for MCP servers, and one skill that teaches the agent
//! how to use it. Settings adds and removes the two together.
use std::{
    fs,
    path::{Path, PathBuf},
};

mod places;
pub use places::Places;
use places::{antigravity, opencode, pi, qoder, trae_mcp};
mod deepseek;
mod mcp_json;
mod skill;
#[cfg(test)]
mod tests;

pub use skill::{SkillState, SKILL_FOLDER};

/// The name each client knows Agent HUD's MCP server by.
pub const SERVER_NAME: &str = "agent-hud";

/// A client Settings can give HUD tools to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Client {
    ClaudeCode,
    Codex,
    Cursor,
    DeepSeek,
    Antigravity,
    ZCode,
    WorkBuddy,
    Kimi,
    Qoder,
    Trae,
    TraeCn,
    Pi,
    OpenCode,
}

/// In the order Settings shows them.
pub const CLIENTS: [Client; 13] = [
    Client::ClaudeCode,
    Client::Codex,
    Client::Cursor,
    Client::DeepSeek,
    Client::Antigravity,
    Client::ZCode,
    Client::WorkBuddy,
    Client::Kimi,
    Client::Qoder,
    Client::Trae,
    Client::TraeCn,
    Client::Pi,
    Client::OpenCode,
];

impl Client {
    pub fn from_id(id: &str) -> Option<Self> {
        CLIENTS.into_iter().find(|client| client.id() == id)
    }

    /// The id Settings and the host use.
    pub fn id(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claudeCode",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::DeepSeek => "deepseek",
            Self::Antigravity => "antigravity",
            Self::ZCode => "zcode",
            Self::WorkBuddy => "workbuddy",
            Self::Kimi => "kimi",
            Self::Qoder => "qoder",
            Self::Trae => "trae",
            Self::TraeCn => "traeCn",
            Self::Pi => "pi",
            Self::OpenCode => "opencode",
        }
    }

    /// The vendor's own name for the client.
    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::Cursor => "Cursor",
            Self::DeepSeek => "DeepSeek Harness",
            Self::Antigravity => "Antigravity",
            Self::ZCode => "ZCode",
            Self::WorkBuddy => "WorkBuddy",
            Self::Kimi => "Kimi Code",
            Self::Qoder => "Qoder",
            Self::Trae => "TRAE",
            Self::TraeCn => "TRAE CN",
            Self::Pi => "Pi",
            Self::OpenCode => "OpenCode",
        }
    }

    pub fn command(self) -> Option<&'static str> {
        match self {
            Self::ClaudeCode => Some("claude"),
            Self::Codex => Some("codex"),
            _ => None,
        }
    }

    /// The folder this client reads the user's skills from.
    pub fn skills(self, places: &Places) -> PathBuf {
        match self {
            Self::ClaudeCode => places.claude.join("skills"),
            Self::Codex => places.home.join(".agents").join("skills"),
            Self::Cursor => places.home.join(".cursor").join("skills"),
            Self::DeepSeek => places.deepseek.join("skills"),
            Self::Antigravity => antigravity(places).join("skills"),
            Self::ZCode => places.home.join(".zcode").join("skills"),
            Self::WorkBuddy => places.home.join(".workbuddy").join("skills"),
            Self::Kimi => places.home.join(".kimi-code").join("skills"),
            Self::Qoder => qoder(places).join("skills"),
            Self::Trae => places.home.join(".trae").join("skills"),
            Self::TraeCn => places.home.join(".trae-cn").join("skills"),
            // Both read the Agent Skills folder Codex uses too, beside their
            // own: one copy there, never a second one they would both list.
            Self::Pi | Self::OpenCode => places.home.join(".agents").join("skills"),
        }
    }

    /// The settings file Agent HUD edits to add its MCP server.
    pub fn settings(self, places: &Places) -> Option<PathBuf> {
        match self {
            Self::Cursor => Some(places.home.join(".cursor").join("mcp.json")),
            Self::DeepSeek => Some(places.deepseek.join("cordis.patch.yml")),
            Self::Antigravity => Some(antigravity(places).join("mcp_config.json")),
            Self::ZCode => Some(places.home.join(".zcode/cli/config.json")),
            Self::WorkBuddy => Some(places.home.join(".workbuddy/mcp.json")),
            Self::Kimi => Some(places.home.join(".kimi-code/mcp.json")),
            Self::Qoder => Some(qoder(places).join("settings.json")),
            Self::Trae => Some(trae_mcp(places, "Trae")),
            Self::TraeCn => Some(trae_mcp(places, "Trae CN")),
            Self::Pi => Some(pi(places).join("mcp.json")),
            Self::OpenCode => Some(opencode(places).join("opencode.json")),
            Self::ClaudeCode | Self::Codex => None,
        }
    }

    /// Whether a client without a command line is on this machine: its
    /// folder, or its app.
    pub fn present(self, places: &Places) -> bool {
        let app =
            |name: &str| crate::os::application_installed(&places.home, &places.environment, name);
        match self {
            Self::Cursor => places.home.join(".cursor").is_dir() || app("Cursor"),
            Self::DeepSeek => places.deepseek.is_dir() || app("DeepSeek Harness"),
            Self::Antigravity => antigravity(places).is_dir() || app("Antigravity"),
            Self::ZCode => places.home.join(".zcode").is_dir() || app("ZCode"),
            Self::WorkBuddy => places.home.join(".workbuddy").is_dir() || app("WorkBuddy"),
            Self::Kimi => {
                places.home.join(".kimi-code").is_dir() || places.home.join(".kimi").is_dir()
            }
            Self::Qoder => qoder(places).is_dir() || app("Qoder"),
            Self::Trae => places.home.join(".trae").is_dir() || app("Trae"),
            Self::TraeCn => places.home.join(".trae-cn").is_dir() || app("Trae CN"),
            Self::Pi => pi(places).is_dir(),
            Self::OpenCode => opencode(places).is_dir(),
            Self::ClaudeCode | Self::Codex => false,
        }
    }

    /// Whether the client's settings file lists Agent HUD's server.
    pub fn listed(self, places: &Places) -> bool {
        let Some(file) = self.settings(places) else {
            return false;
        };
        let Ok(text) = read(&file) else {
            return false;
        };
        match self {
            Self::ZCode => mcp_json::listed_zcode(text.as_deref()),
            Self::OpenCode => mcp_json::listed_opencode(text.as_deref()),
            Self::DeepSeek => deepseek::listed(text.as_deref()),
            Self::ClaudeCode | Self::Codex => false,
            _ => mcp_json::listed(text.as_deref()),
        }
    }

    /// Adds (`Some(executable)`) or removes (`None`) Agent HUD's server in the
    /// client's settings file. Returns whether the file changed.
    pub fn set_server(self, places: &Places, executable: Option<&Path>) -> Result<bool, String> {
        let file = self
            .settings(places)
            .ok_or("This client adds MCP servers with its own command")?;
        let target = crate::hooks::settings_target(&file)?;
        let original = read(&target)?;
        let updated = match self {
            Self::ZCode => mcp_json::edit_zcode(self.name(), original.as_deref(), executable)?,
            Self::OpenCode => {
                mcp_json::edit_opencode(self.name(), original.as_deref(), executable)?
            }
            Self::DeepSeek => deepseek::edit(original.as_deref(), executable)?,
            Self::ClaudeCode | Self::Codex => unreachable!("no settings file"),
            Self::Trae | Self::TraeCn => {
                mcp_json::edit_quoted(self.name(), original.as_deref(), executable)?
            }
            _ => mcp_json::edit(self.name(), original.as_deref(), executable)?,
        };
        if updated == original {
            return Ok(false);
        }
        crate::hooks::write_settings(&target, updated.as_deref().map(str::as_bytes))?;
        Ok(true)
    }

    pub fn skill_state(self, places: &Places) -> SkillState {
        skill::state(&self.skills(places))
    }

    /// Writes the skill, or brings Agent HUD's older copy up to date. A
    /// folder of the same name that Agent HUD did not write stays untouched,
    /// and so is refused. Returns whether a file changed.
    pub fn add_skill(self, places: &Places) -> Result<bool, String> {
        skill::add(&self.skills(places))
    }

    /// Removes Agent HUD's skill; someone else's folder of the same name stays.
    pub fn remove_skill(self, places: &Places) -> Result<(), String> {
        skill::remove(&self.skills(places))
    }
}

/// Settings files are small; anything bigger is not one Agent HUD should edit.
const LIMIT: u64 = 4 * 1024 * 1024;

/// A settings file's text, None when it does not exist.
pub(crate) fn read(path: &Path) -> Result<Option<String>, String> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.len() > LIMIT => {
            return Err(format!("{} is too large to edit", path.display()))
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(format!("{} could not be read", path.display())),
    }
    let bytes = fs::read(path).map_err(|_| format!("{} could not be read", path.display()))?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("{} is not UTF-8 text", path.display()))
}
