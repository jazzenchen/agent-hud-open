//! Linux: the computer name from the environment. Grants have no store here
//! yet (a Secret Service adapter would keep them), so sharing stays off.
use agent_hud_lan::credentials::SecretStore;
use std::sync::Arc;

pub fn computer_name() -> String {
    super::super::environment_name("HOSTNAME").unwrap_or_else(|| "Computer".into())
}
pub(crate) fn secrets() -> Option<Arc<dyn SecretStore>> {
    None
}
