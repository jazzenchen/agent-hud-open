//! The clients Agent HUD can hand a message to, and how each takes one.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Hash, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Codex,
    Antigravity,
    DeepSeek,
    Cursor,
    Trae,
    WorkBuddy,
    ZCode,
    Qoder,
    Kimi,
    /// Pi and OpenCode take a message through Agent HUD's observer in them
    /// (its mailbox), at once even while the session is idle.
    Pi,
    OpenCode,
}
impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Antigravity => "antigravity",
            Self::DeepSeek => "deepseek",
            Self::Cursor => "cursor",
            Self::Trae => "trae",
            Self::WorkBuddy => "workbuddy",
            Self::ZCode => "zcode",
            Self::Qoder => "qoder",
            Self::Kimi => "kimi",
            Self::Pi => "pi",
            Self::OpenCode => "opencode",
        }
    }
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "antigravity" => Some(Self::Antigravity),
            "deepseek" => Some(Self::DeepSeek),
            "cursor" => Some(Self::Cursor),
            "trae" | "trae-cn" => Some(Self::Trae),
            "workbuddy" | "codebuddy" => Some(Self::WorkBuddy),
            "zcode" => Some(Self::ZCode),
            "qoder" | "qoder-cn" | "qoder-work" => Some(Self::Qoder),
            "kimi" => Some(Self::Kimi),
            "pi" => Some(Self::Pi),
            "opencode" => Some(Self::OpenCode),
            _ => None,
        }
    }
    pub fn uses_hook_channel(self) -> bool {
        matches!(
            self,
            Self::Cursor
                | Self::Trae
                | Self::WorkBuddy
                | Self::ZCode
                | Self::Qoder
                | Self::Kimi
                | Self::Pi
                | Self::OpenCode
        )
    }
    /// Whether Agent HUD's observer in the client takes the session's
    /// messages from the hook channel (`agents::hooks` mailbox.js), rather
    /// than its stop hook.
    pub fn uses_observer(self) -> bool {
        matches!(self, Self::Pi | Self::OpenCode)
    }
    pub fn queues_in_channel(self) -> bool {
        matches!(self, Self::Claude | Self::DeepSeek) || self.uses_hook_channel()
    }
}
