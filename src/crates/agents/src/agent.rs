//! The descriptor every client has, and the registry of them all.
use crate::hooks::{Hooks, Observer};
use crate::{
    antigravity, buddy, claude, codex, copilot, cursor, deepseek, devin, glm, grok, hermes, kimi,
    openclaw, opencode, pi, qoder, qwen, zcode,
};
use crate::{Channel, Discovery, PerOs, Replies, Source};

/// What Agent HUD knows about one client. Each behaviour the contract names
/// (docs/agent-contract.md) joins this descriptor as its step lands.
pub struct Agent {
    /// The id sessions, settings and the ledger use.
    pub id: &'static str,
    /// The client's own name for itself.
    pub name: &'static str,
    /// Whether its sessions can be read, beyond its installation or account.
    pub sessions: bool,
    /// Its hooks, when it reports through them or is known to them.
    pub hooks: Option<&'static Hooks>,
    /// Its session observer, when its own extension or plugin records turns.
    pub observer: Option<&'static Observer>,
    /// Its native channel, when the client is reached through the OS rather
    /// than its files or hooks.
    pub channel: Option<&'static Channel>,
    /// Its command-line program, as installed on PATH or in its own folders,
    /// where the engine looks for it: to start it, or to tell a hook's
    /// caller is it.
    pub program: Option<&'static str>,
    /// How words reach its sessions, when they can.
    pub replies: Option<&'static Replies>,
    /// Which lines of its transcript start its turns, when it states its
    /// own rule; others are read with `transcript::turns::SHARED`.
    pub turns: Option<&'static crate::transcript::turns::Turns>,
    /// What each line of its transcript shows in a page of messages, when it
    /// states its own rule; others are read with `transcript::items::SHARED`.
    pub items: Option<&'static crate::transcript::items::Items>,
    /// Where the engine reads its sessions, on each OS.
    pub sources: PerOs<&'static [Source]>,
    /// How Agent HUD tells it is installed, configured or used here.
    pub discovery: &'static Discovery,
}

/// Every client, in the order Settings and the statistics list them.
pub static ALL: &[&Agent] = &[
    &claude::AGENT,
    &codex::AGENT,
    &deepseek::AGENT,
    &antigravity::AGENT,
    &cursor::AGENT,
    &grok::CLI,
    &grok::BOT,
    &copilot::AGENT,
    &openclaw::AGENT,
    &hermes::AGENT,
    &zcode::AGENT,
    &buddy::CODEBUDDY,
    &buddy::WORKBUDDY,
    &qwen::AGENT,
    &opencode::AGENT,
    &kimi::AGENT,
    &pi::AGENT,
    &qoder::QODER,
    &qoder::QODER_CN,
    &qoder::QODER_WORK,
    &devin::AGENT,
    &glm::AGENT,
];

/// The client with this id.
pub fn by_id(id: &str) -> Option<&'static Agent> {
    ALL.iter().copied().find(|agent| agent.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_client_is_listed_once_in_settings_order() {
        let ids = ALL.iter().map(|agent| agent.id).collect::<Vec<_>>();
        assert_eq!(
            ids,
            [
                "claude",
                "codex",
                "deepseek",
                "antigravity",
                "cursor",
                "grok-cli",
                "grok-bot",
                "copilot",
                "openclaw",
                "hermes",
                "zcode",
                "codebuddy",
                "workbuddy",
                "qwen",
                "opencode",
                "kimi",
                "pi",
                "qoder",
                "qoder-cn",
                "qoder-work",
                "devin",
                "glm",
            ]
        );
        for agent in ALL {
            assert!(std::ptr::eq(by_id(agent.id).unwrap(), *agent));
        }
        assert!(by_id("unknown").is_none());
    }
}
