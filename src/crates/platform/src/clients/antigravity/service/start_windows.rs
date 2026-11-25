//! Find the desktop service for a new conversation, which has no owner yet.
use super::{service, Endpoint};
use agent_hud_agents::antigravity;

pub(crate) fn desktop_start_endpoint() -> Result<Option<Endpoint>, String> {
    // Other Antigravity clients must not take a desktop project start or
    // prevent a usable desktop endpoint from being verified.
    let candidates = super::running::servers()?
        .into_iter()
        .filter(|server| server.client == antigravity::AGENT.id)
        .collect::<Vec<_>>();
    match candidates.len() {
        0 => Ok(None),
        1 => service()?
            .verify_servers(candidates)
            .pop()
            .map(Some)
            .ok_or_else(|| "The running Antigravity Desktop service could not be verified".into()),
        _ => Err("More than one Antigravity Desktop service is running".into()),
    }
}
