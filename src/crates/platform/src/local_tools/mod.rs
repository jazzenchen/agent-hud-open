//! Local application tools channel, following the original public MCPAppChannel.
//! This is a per-user IPC adapter; handlers read the application's existing facts.
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};

/// As large as a response: a tool's arguments can carry a whole text or file.
pub const REQUEST_LIMIT: u64 = 16 * 1024 * 1024;
pub const RESPONSE_LIMIT: u64 = 16 * 1024 * 1024;
/// A request's error when no answer came back: the app took too long or let
/// the connection go. The request may still have been carried out.
pub const NO_ANSWER: &str = "The local tools response did not complete";
pub type Handler = Arc<dyn Fn(Value, &AtomicBool) -> Value + Send + Sync>;

pub fn socket_path(data: &Path) -> PathBuf {
    data.join("tools/mcp.sock")
}
pub fn supported() -> bool {
    true
}

/// The agent-messages tools the app last listed, kept beside the channel. A
/// relay a client starts while the app is away (an update stops every relay,
/// and Codex starts a new one at once) lists these, not none: Codex reads a
/// server's tools only when it starts one, and never again.
fn saved_tools_path(data: &Path) -> PathBuf {
    data.join("tools/host-tools.json")
}
/// Keeps `tools`, the app's answer to `host_tools`, for relays that cannot ask it.
pub fn save_host_tools(data: &Path, tools: &Value) -> Result<(), String> {
    crate::delivery::atomic_json(&saved_tools_path(data), tools).map_err(|error| error.to_string())
}
/// The app's last answer to `host_tools`, when it kept one.
pub fn saved_host_tools(data: &Path) -> Option<Value> {
    let saved: Value = serde_json::from_slice(&std::fs::read(saved_tools_path(data)).ok()?).ok()?;
    saved["tools"].is_array().then_some(saved)
}

// A Unix socket, or a named pipe on Windows.
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{request, Server};
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{request, Server};
