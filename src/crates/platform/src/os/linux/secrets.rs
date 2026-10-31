//! The system credential store: not implemented on this host.
//!
//! Clients such as gh keep tokens in the Secret Service (libsecret). Until an adapter reads it,
//! those readers fall back to their plain configuration files.

use agent_hud_agents::Answer;

pub(crate) fn stored_secret(_service: &str) -> Answer<String> {
    Answer::Unsupported
}

/// A desktop app's Chromium cookie: not decrypted on this host yet, so the
/// Claude desktop app's sign-in is not read here.
pub(crate) fn app_cookie(
    _app_data: &std::path::Path,
    _app: &str,
    _site: &str,
    _name: &str,
) -> Answer<String> {
    Answer::Unsupported
}
