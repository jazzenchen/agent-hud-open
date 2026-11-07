//! The user's choices for agent messages, as the app's settings hold them.
use serde_json::Value;

/// Passes in a row before the user is asked to look, unless they chose another.
pub const DEFAULT_LIMIT: u32 = 10;

/// Settings → Agent plugins. Among the agent tools: "Let agents message each
/// other" (`agentMessages`), "Send as me" (`agentMessagesAsUser`) and how many
/// passes in a row before the user looks (`agentMessagesLimit`, null for no
/// limit). On the Claude Code and DeepSeek Harness plugins' rows: "Send
/// messages" (`claudeMessages`, `deepseekMessages`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub enabled: bool,
    /// The words go in as the user's own, with no label and no reply line.
    pub as_user: bool,
    /// Passes in a row with no word from the user; None is no limit.
    pub limit: Option<u32>,
    /// Messages may go into Claude Code sessions.
    pub claude: bool,
    /// Messages may go into DeepSeek Harness sessions.
    pub deepseek: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            enabled: false,
            as_user: false,
            limit: Some(DEFAULT_LIMIT),
            claude: true,
            deepseek: true,
        }
    }
}
impl Preferences {
    /// The choices in the app's `settings`; a setting not there yet is its default.
    pub fn read(settings: &Value) -> Self {
        let limit = match settings.get("agentMessagesLimit") {
            None => Some(DEFAULT_LIMIT),
            Some(Value::Null) => None,
            Some(limit) => limit
                .as_u64()
                .and_then(|limit| u32::try_from(limit).ok())
                .filter(|limit| *limit > 0)
                .or(Some(DEFAULT_LIMIT)),
        };
        Self {
            enabled: settings["agentMessages"] == true,
            as_user: settings["agentMessagesAsUser"] == true,
            limit,
            claude: settings["claudeMessages"] != false,
            deepseek: settings["deepseekMessages"] != false,
        }
    }
}
