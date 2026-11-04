//! Native channels: client mechanisms that need the operating system. Each
//! client's descriptor names its channel (`Agent::channel`), what it carries
//! and on which OS it runs; the channel itself lives here, one folder per
//! client. None of them starts a conversation, an engine or a sign-in.
pub mod antigravity;
pub mod codex;
pub(crate) mod deepseek;

/// Whether the client's native channel runs on this OS.
pub(crate) fn runs(channel: &agent_hud_agents::Channel) -> bool {
    channel.runs_on(crate::os::OS)
}

/// The terminal a session runs in, as the client's native channel knows it
/// (Codex's daemon serves its terminal sessions).
pub(crate) fn session_terminal(
    agent: &str,
    session: &str,
    home: &std::path::Path,
    env: &std::collections::HashMap<String, String>,
) -> Option<serde_json::Value> {
    if agent == agent_hud_agents::codex::AGENT.id {
        codex::daemon_terminal(session, home, env)
    } else {
        None
    }
}
