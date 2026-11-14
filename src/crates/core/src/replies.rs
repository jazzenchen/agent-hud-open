//! A host's way to send words into a client's session as its user's next
//! message, as a click on the client's own question sends them. The client's
//! descriptor says it takes them, through what, and which setting turns
//! answering on (`Agent::replies`): for Claude Code, Agent HUD's plugin
//! (`agent_hud_platform::claude_plugin::ClaudeReplies`; agent-hud-native-open
//! `ClaudeDesktopAsks.Reply`) and `settings.claudeAnswers`. Core decides which
//! questions it can answer and with which words; the host only carries them.
//! Without a host, those questions are answered in the client's own window.

pub use agent_hud_agents::replies::{AnswerFix, Unreachable};

pub trait SessionReplies: Send + Sync {
    /// Whether words left now would reach the session `session` of `source`;
    /// Err gives the reason they would not, in the app's language
    /// (`tr!`), and the step that would change it. Asked at every
    /// collection pass for each question that waits, so it must be cheap and
    /// must never wait on the host's own data.
    fn reaches(&self, source: &str, session: &str) -> Result<(), Unreachable>;
    /// Leaves `words` for that session as its user's next message. Err with
    /// the reason, in the app's language, when they could not be left.
    fn reply(&self, source: &str, session: &str, words: &str) -> Result<(), String>;
}
