//! Starting new sessions from the phone, the HUD or an MCP caller: the
//! folders each client offers, one start per request identity, and moving a
//! Claude Code background job into the desktop app once its turn stopped.
use super::process::{NativeProcessRunner, ProcessRunner};
use super::*;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

mod clients;
mod commands;
mod handoff;
mod offers;
mod start;
mod trust;
use commands::environment;
pub use handoff::ClaudeMove;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartConfig {
    pub home: PathBuf,
    pub claude_config: PathBuf,
    pub codex_data: PathBuf,
    pub claude_executable: Option<PathBuf>,
    pub codex_executable: Option<PathBuf>,
    #[serde(default)]
    pub antigravity_executable: Option<PathBuf>,
    /// Not read by starts, which look for the desktop apps on disk at every
    /// start; kept so a saved configuration keeps its shape.
    #[serde(default)]
    pub claude_desktop: bool,
    #[serde(default)]
    pub codex_desktop: bool,
    pub task_root: PathBuf,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}
impl StartConfig {
    /// How a message reaches Codex's queue when no shared daemon runs: the
    /// Codex this computer starts sessions with, in the environment it starts
    /// them in.
    pub fn codex_queue(&self) -> Option<QueueRoute> {
        Some(QueueRoute {
            executable: self.codex_executable.clone()?,
            environment: environment(self),
            applications: self.applications(),
        })
    }
    /// Where desktop apps are installed: `/Applications` and `~/Applications`
    /// on macOS; empty where they are not looked for yet (Linux, Windows).
    pub fn applications(&self) -> Vec<PathBuf> {
        host::applications(&self.home)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRequest {
    pub provider: Provider,
    pub request_id: String,
    pub text: String,
    /// None is not an offered independent task; empty string explicitly selects it.
    pub folder: String,
    /// The new Codex thread's name; its first line of text when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartOffer {
    pub provider: Provider,
    pub app: String,
    pub folders: Vec<String>,
    pub without_folder: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartReceipt {
    pub provider: Provider,
    pub request_id: String,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub state: String,
    pub reason: Option<String>,
    pub job_id: Option<String>,
    pub handoff_pending: bool,
}

/// Whether a started Claude job can move into the desktop app now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandoffCheck {
    /// Its turn still runs, or nothing says it stopped yet; look again later.
    Wait,
    /// Its turn stopped and nothing else holds the session.
    Ready,
    /// It will never move: the reason is kept on its receipt.
    Drop(String),
}

pub struct SessionStarts {
    config: StartConfig,
    receipts: PathBuf,
    offers: Vec<StartOffer>,
    runner: Arc<dyn ProcessRunner>,
    /// Where desktop apps are installed: `/Applications` and `~/Applications`.
    applications: Vec<PathBuf>,
}
impl SessionStarts {
    pub fn new(config: StartConfig, receipts: PathBuf) -> Result<Self> {
        Self::with_runner(config, receipts, Arc::new(NativeProcessRunner))
    }
    pub fn with_runner(
        config: StartConfig,
        receipts: PathBuf,
        runner: Arc<dyn ProcessRunner>,
    ) -> Result<Self> {
        if !config.home.is_absolute()
            || !config.claude_config.is_absolute()
            || !config.codex_data.is_absolute()
            || !config.task_root.is_absolute()
            || !receipts.is_absolute()
        {
            return Err(Failure::InvalidInput(
                "Explicit start paths must be absolute".into(),
            ));
        }
        private_dir(&receipts)?;
        let applications = config.applications();
        Ok(Self {
            config,
            receipts,
            offers: Vec::new(),
            runner,
            applications,
        })
    }
    /// Tests look for desktop apps in fixture folders instead of this computer's.
    #[cfg(any(test, feature = "fixtures"))]
    pub fn set_application_folders(&mut self, folders: Vec<PathBuf>) {
        self.applications = folders;
    }
    fn executable(&self, provider: Provider) -> Option<&Path> {
        match provider {
            Provider::Claude => self.config.claude_executable.as_deref(),
            Provider::Codex => self.config.codex_executable.as_deref(),
            Provider::Antigravity => self.config.antigravity_executable.as_deref(),
            Provider::DeepSeek
            | Provider::Cursor
            | Provider::Trae
            | Provider::WorkBuddy
            | Provider::ZCode
            | Provider::Qoder
            | Provider::Kimi
            | Provider::Pi
            | Provider::OpenCode => None,
        }
    }
    /// Whether the client's desktop app is installed, looked up on disk each
    /// time: Claude.app for Claude; Codex.app or ChatGPT.app for Codex;
    /// Antigravity.app or Antigravity IDE.app for Antigravity.
    fn desktop(&self, provider: Provider) -> bool {
        let names: &[&str] = match provider {
            Provider::Claude => &["Claude.app"],
            Provider::Codex => &["Codex.app", "ChatGPT.app"],
            Provider::Antigravity => &["Antigravity.app", "Antigravity IDE.app"],
            Provider::DeepSeek
            | Provider::Cursor
            | Provider::Trae
            | Provider::WorkBuddy
            | Provider::ZCode
            | Provider::Qoder
            | Provider::Kimi
            | Provider::Pi
            | Provider::OpenCode => &[],
        };
        self.applications
            .iter()
            .any(|folder| names.iter().any(|name| folder.join(name).is_dir()))
    }
}

/// A JSON file under 1 MiB, or null when it is missing, larger or not JSON.
fn read_json(path: &Path) -> Value {
    fs::read(path)
        .ok()
        .filter(|bytes| bytes.len() < 1024 * 1024)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests;
