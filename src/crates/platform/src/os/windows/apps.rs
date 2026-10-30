//! Running applications: not implemented on this host.
//!
//! Navigation that needs a running client, such as opening a Codex thread or
//! an Antigravity conversation, is attempted without this check, and bringing
//! the client forward reports false.

/// Unknown here, so callers attempt navigation.
pub(crate) fn app_running(_bundle_id: &str) -> bool {
    true
}

pub(crate) fn activate_app(_bundle_id: &str) -> bool {
    false
}
