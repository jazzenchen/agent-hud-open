//! The user's choices for agent messages, as the app's settings hold them.
use serde_json::Value;

/// Passes in a row before the user is asked to look, unless they chose another.
pub const DEFAULT_LIMIT: u32 = 10;

/// Settings → Agents. Among the agent tools: "Let agents message each
/// other" (`agentMessages`), "Send as me" (`agentMessagesAsUser`), "Ask for
/// replies" (`agentMessagesReplies`: never, when needed or always), "Let agents
/// decide when to steer" (`agentMessagesSteerOnOwn`) and how many
/// passes in a row before the user looks (`agentMessagesLimit`, null for no
/// limit). On the Claude Code and DeepSeek Harness plugins' rows: "Send
/// messages" (`claudeMessages`, `deepseekMessages`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub enabled: bool,
    /// The words go in as the user's own, with no label and no reply line.
    pub as_user: bool,
    /// Which messages ask for a reply (`Replies`).
    pub replies: Replies,
    /// Agents may steer on their own, to correct the work a turn in progress
    /// is doing, and a steer's last line asks for no reply of its own. Off:
    /// the tools allow a steer only when the user asked for one.
    pub steer_on_own: bool,
    /// Passes in a row with no word from the user; None is no limit.
    pub limit: Option<u32>,
    /// Messages may go into Claude Code sessions.
    pub claude: bool,
    /// Messages may go into DeepSeek Harness sessions.
    pub deepseek: bool,
}
/// "Ask for replies": which messages end with the line asking the recipient
/// to reply, which Agent HUD follows up when no reply comes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Replies {
    /// None: the recipient is never asked, and its answer stays in its own
    /// session.
    Never,
    /// The sender says of each message (`require_reply`): one that asks
    /// carries the label and the reply line, under "Send as me" too.
    #[default]
    WhenNeeded,
    /// Every labelled message except a reply, so two agents do not answer
    /// each other until the pass limit; a reply asks only when its sender
    /// passes `require_reply`.
    Always,
}
impl Replies {
    /// `agentMessagesReplies`; before it, the switch "Ask for replies only
    /// when needed" (`agentMessagesRequireReply`), whose off was Always.
    fn read(settings: &Value) -> Self {
        match settings["agentMessagesReplies"].as_str() {
            Some("never") => Self::Never,
            Some("always") => Self::Always,
            Some(_) => Self::WhenNeeded,
            None if settings["agentMessagesRequireReply"] == false => Self::Always,
            None => Self::WhenNeeded,
        }
    }
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            enabled: false,
            as_user: false,
            replies: Replies::WhenNeeded,
            steer_on_own: false,
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
            replies: Replies::read(settings),
            steer_on_own: settings["agentMessagesSteerOnOwn"] == true,
            limit,
            claude: settings["claudeMessages"] != false,
            deepseek: settings["deepseekMessages"] != false,
        }
    }
}
