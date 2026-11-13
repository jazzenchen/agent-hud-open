//! The descriptor every client has, and the registry of them all.
use crate::hooks::{Hooks, Observer};
use crate::{
    antigravity, ark, buddy, claude, cline, codex, copilot, cursor, deepseek, devin, droid, glm,
    goose, grok, hermes, kimi, minimax, openclaw, opencode, pi, qoder, qwen, trae, zcode,
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
    &trae::TRAE,
    &trae::TRAE_CN,
    &devin::AGENT,
    &glm::AGENT,
    &minimax::AGENT,
    &ark::AGENT,
    &opencode::KILO,
    &opencode::MIMO_CODE,
    &opencode::TELEAGENT,
    &pi::OH_MY_PI,
    &droid::AGENT,
    &cline::CLINE,
    &cline::ROO_CODE,
    &goose::AGENT,
];

/// Every account reader, in the order the engine reads them, one at a
/// time. A provider is not always one client: Grok Bot's quota covers its
/// CLI and its Bot, beside the CLI's own xAI credits, the open agents' plans
/// are read per plan, and relays are read for whichever clients send their
/// calls through them.
pub static ACCOUNTS: &[&crate::account::Provider] = &[
    &claude::account::PROVIDER,
    &codex::account::PROVIDER,
    &deepseek::account::PROVIDER,
    &antigravity::account::PROVIDER,
    &cursor::account::PROVIDER,
    &grok::account::PROVIDER,
    &grok::credits::PROVIDER,
    &copilot::account::PROVIDER,
    &devin::account::PROVIDER,
    &hermes::account::PROVIDER,
    &crate::coding_plans::KIMI,
    &crate::coding_plans::GLM,
    &crate::coding_plans::OPENCODE_GO,
    &minimax::account::PROVIDER,
    &ark::account::PROVIDER,
    &cline::account::PROVIDER,
    &crate::relays::PROVIDER,
    &droid::account::PROVIDER,
    &crate::qoder::account::PROVIDER,
    &crate::buddy::account::PROVIDER,
    &trae::account::PROVIDER,
];

/// Where a session of `source` opens, as its agent says (`Discovery::session_target`):
/// the target the session list and `open_session` both use. None for an
/// agent with no rule or a session with no place of its own.
pub fn session_target(
    source: &str,
    id: &str,
    cwd: Option<&str>,
    client: Option<&str>,
) -> Option<serde_json::Value> {
    (by_id(source)?.discovery.session_target?)(id, cwd, client)
}

/// The account reader with this provider id.
pub fn account_provider(id: &str) -> Option<&'static crate::account::Provider> {
    ACCOUNTS.iter().copied().find(|provider| provider.id == id)
}

/// The client with this id.
pub fn by_id(id: &str) -> Option<&'static Agent> {
    ALL.iter().copied().find(|agent| agent.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No two clients share an id: `by_id` finds each one as itself.
    #[test]
    fn every_client_has_its_own_id() {
        for agent in ALL {
            assert!(std::ptr::eq(by_id(agent.id).unwrap(), *agent));
        }
        assert!(by_id("unknown").is_none());
    }

    #[test]
    fn a_session_opens_where_its_agent_says() {
        use serde_json::json;
        assert_eq!(
            session_target("grok-bot", "grok-bot:acct:126dbae3-ad9b", None, None),
            Some(json!({"kind":"grok-bot-agent","id":"126dbae3-ad9b"}))
        );
        // Not a Grok Bot session id: no bot to open.
        assert_eq!(session_target("grok-bot", "onqw4zbomnwg", None, None), None);
        assert_eq!(
            session_target("grok-bot", "grok-bot:acct:", None, None),
            None
        );
        assert_eq!(
            session_target("cursor", "cursor:bc-12", None, None),
            Some(json!({"kind":"cursor-agent","id":"bc-12","cwd":null}))
        );
        assert_eq!(
            session_target("cursor", "cursor:3e8f", Some("/w/p"), None),
            Some(json!({"kind":"cursor-agent","id":"3e8f","cwd":"/w/p"}))
        );
        assert_eq!(session_target("cursor", "cursor:3e8f", None, None), None);
        assert_eq!(
            session_target("antigravity", "antigravity:c1", Some("/w"), Some("Desktop")),
            Some(json!({"kind":"antigravity-conversation","id":"c1","cwd":"/w"}))
        );
        assert_eq!(
            session_target("antigravity", "antigravity:c1", None, Some("CLI")),
            None
        );
        // Agents with no rule of their own, and unknown sources.
        assert_eq!(session_target("claude", "abc", Some("/w"), None), None);
        assert_eq!(session_target("grok-cli", "grok-cli:s", None, None), None);
        assert_eq!(session_target("unknown", "x", None, None), None);
    }
}
