//! What was found of each client on this machine: installed, configured, used.
use serde::Serialize;

/// Only non-sensitive evidence categories cross the native boundary. No credential,
/// config content, account identity, session text or personal path is retained here.
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDiscovery {
    #[serde(skip)]
    pub id: String,
    pub installed: bool,
    pub configured: bool,
    pub usage: bool,
    pub evidence: Vec<String>,
    /// An app-level fallback the host can open, independent of CLI or storage evidence.
    #[serde(skip)]
    pub navigation_target: Option<serde_json::Value>,
}
