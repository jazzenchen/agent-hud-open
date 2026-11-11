//! Sends words into running agent sessions (Claude Code, Codex, Antigravity,
//! DeepSeek Harness) and starts new ones. The host supplies the sessions it
//! knows; nothing here discovers transcripts, starts a collector, resumes a
//! second engine or installs clients.
mod antigravity;
mod claude;
mod codex;
mod deepseek;
mod files;
#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;
mod handoffs;
mod journal;
pub mod process;
mod provider;
mod sending;
mod service;
mod setup;
mod starts;
mod targets;
// What sending and starting need from the operating system: private files
// and folders, where apps and command lines are installed, and how a client
// link opens. macOS and Linux share `unix.rs`.
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod host;
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod host;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod host;
#[cfg(unix)]
mod unix;

pub use antigravity::{
    desktop_installed as antigravity_desktop_installed, AntigravityTransport,
    NativeAntigravityTransport,
};
pub use claude::ClaudeChannel;
pub use codex::{
    desktop_installed as codex_desktop_installed, thread_open as codex_thread_open, CodexTransport,
    NativeCodexTransport, QueueRoute,
};
pub use deepseek::DeepSeekChannel;
pub use files::atomic_json;
pub use handoffs::Handoffs;
use host::private_dir;
use serde::{Deserialize, Serialize};
pub use service::hook_session_key;
use service::valid_session_id;
pub use starts::{
    ClaudeMove, HandoffCheck, SessionStarts, StartConfig, StartOffer, StartReceipt, StartRequest,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub use targets::{Capability, ReachableTarget};

pub type Result<T> = std::result::Result<T, Failure>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "reason", rename_all = "camelCase")]
pub enum Failure {
    InvalidInput(String),
    Unavailable(String),
    Unsupported(String),
    Rejected(String),
    TimedOut,
    Protocol(String),
    AcceptanceUncertain,
    IdentityConflict,
    Io(String),
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Failure {}
impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        if matches!(
            e.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        ) {
            Self::TimedOut
        } else {
            Self::Io(e.to_string())
        }
    }
}

/// Why a message does not go into a Claude Code session: the "Send
/// messages" switch on its Agent HUD plugin's row is off.
pub const CLAUDE_MESSAGES_OFF: &str =
    "Sending messages to Claude Code is off (Settings → Agent plugins)";
/// The same, for a DeepSeek Harness session.
pub const DEEPSEEK_MESSAGES_OFF: &str =
    "Sending messages to DeepSeek Harness is off (Settings → Agent plugins)";

pub use provider::Provider;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PromptForm {
    #[default]
    Labelled,
    Own,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    #[default]
    Automatic,
    Start,
    Steer,
    Enqueue,
    Interrupt,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeliveryState {
    Waiting,
    Taken,
    Started,
    Steered,
    Queued,
    Refused,
    CancelRequested,
    Cancelled,
    Expired,
    Uncertain,
    Failed,
    Interrupted,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionTarget {
    pub provider: Provider,
    pub session_id: String,
    /// Codex thread identity can differ from the transcript/session identity.
    pub thread_id: Option<String>,
    pub parent_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRequest {
    pub provider: Provider,
    pub session_id: String,
    /// Caller-owned identity, stable across reconnects/retries.
    pub request_id: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub form: PromptForm,
    #[serde(default)]
    pub operation: Operation,
    pub expected_turn_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub provider: Provider,
    pub session_id: String,
    pub thread_id: Option<String>,
    pub request_id: String,
    pub client_request_id: String,
    pub turn_id: Option<String>,
    pub owner_client_id: Option<String>,
    pub state: DeliveryState,
    pub at_ms: u64,
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalClientPaths {
    pub claude_channel: PathBuf,
    pub codex_data: PathBuf,
    pub receipts: PathBuf,
}

/// One host owns this service. Clone the Arc, not a second service instance: the
/// per-session gate preserves send order without serializing unrelated clients.
pub struct LocalClients {
    paths: LocalClientPaths,
    targets: Mutex<Vec<SessionTarget>>,
    gates: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    codex: Arc<dyn CodexTransport>,
    antigravity: Arc<dyn AntigravityTransport>,
    /// The folder Agent HUD's DeepSeek Harness plugin shares with the app;
    /// none, and no Harness session takes a message.
    deepseek: Option<DeepSeekChannel>,
    timeout: Duration,
    /// The folder whose hook inbox records which of Agent HUD's surfaces
    /// sent each prompt (`delivered`); none records nothing.
    deliveries: Option<PathBuf>,
}

/// Whether the client with this id hands a session a message even while it
/// is idle, through a rewake hook waiting in the session (Qoder CLI's).
pub fn wakes_idle(client: &str) -> bool {
    Provider::parse(client).is_some_and(Provider::uses_observer)
        || agent_hud_agents::hooks::hooks_by_id(client).is_some_and(|hooks| hooks.rewake.is_some())
}

pub(crate) fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub(crate) fn valid_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok()
}
pub(crate) fn antigravity_cascade_id(value: &str) -> &str {
    value.strip_prefix("antigravity:").unwrap_or(value)
}
pub(crate) fn millis(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests;
