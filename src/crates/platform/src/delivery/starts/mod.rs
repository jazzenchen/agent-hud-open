//! Starting new sessions from the phone, the HUD or an MCP caller: the
//! folders each client offers, one start per request identity, and moving a
//! Claude Code background job into the desktop app once its turn stopped.
//! DeepSeek Harness has no command that starts a session: Agent HUD's plugin
//! in a running Harness host makes it (`delivery::deepseek`). Pi and OpenCode
//! start one as a run of their command line (`runs`).
use super::process::{NativeProcessRunner, ProcessRunner};
use super::*;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
};

mod clients;
// Only Windows has a Desktop project catalog on this start route.
#[cfg(windows)]
#[path = "codex_projects_windows.rs"]
mod codex_host;
#[cfg(unix)]
#[path = "codex_projects_unix.rs"]
mod codex_host;
mod commands;
mod handoff;
mod offers;
mod runs;
mod start;
mod trust;
mod workspaces;
use commands::environment;
pub use handoff::ClaudeMove;

/// The clients Agent HUD starts sessions in, in the order they are offered.
pub const STARTED: [Provider; 6] = [
    Provider::Claude,
    Provider::Codex,
    Provider::Antigravity,
    Provider::DeepSeek,
    Provider::Pi,
    Provider::OpenCode,
];

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
            applications: self.codex_applications(),
        })
    }
    /// Where the Codex desktop app is: its app folders on macOS, its own
    /// programs on Windows, none on Linux.
    pub fn codex_applications(&self) -> Vec<PathBuf> {
        host::codex_applications(&self.home)
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
    /// The model the new session runs, as `list_models` names it; none
    /// leaves it to the client's own choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// An effort from the selected model, when the start route supports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
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

/// How long live desktop-app probes are cached per provider so refreshing
/// `startFolders` does not repeatedly enumerate processes, shortcuts and
/// Windows Store packages.
const DESKTOP_CACHE_TTL: Duration = Duration::from_secs(60);

pub struct SessionStarts {
    config: StartConfig,
    receipts: PathBuf,
    offers: Vec<StartOffer>,
    runner: Arc<dyn ProcessRunner>,
    /// Test override for desktop app entries (`set_application_folders`); none
    /// asks the host adapter (`host::desktop_installed`) with a 60s TTL cache.
    applications: Option<Vec<PathBuf>>,
    desktop_cache: Mutex<HashMap<Provider, (std::time::Instant, bool)>>,
    codex_projects: codex_host::ProjectCatalog,
    /// The folder Agent HUD's DeepSeek Harness plugin shares with the app;
    /// none, and no Harness session is offered or started.
    deepseek: Option<DeepSeekChannel>,
    /// Pi's and OpenCode's command lines as a test names them; none, and
    /// they are looked up on disk at every offer and start (`command`).
    commands: Option<HashMap<Provider, PathBuf>>,
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
        Ok(Self {
            config,
            receipts,
            offers: Vec::new(),
            runner,
            applications: None,
            desktop_cache: Mutex::new(HashMap::new()),
            codex_projects: Default::default(),
            deepseek: None,
            commands: None,
        })
    }
    /// Harness sessions start through Agent HUD's plugin and the folder it
    /// shares with the app (`LocalClients::deepseek_channel`).
    pub fn with_deepseek_channel(mut self, channel: Option<DeepSeekChannel>) -> Self {
        self.deepseek = channel;
        self
    }
    /// Whether a Harness session can start now: the plugin is installed, and
    /// a Harness host runs it or the Desktop app is there to open.
    pub(super) fn starts_deepseek(&self) -> bool {
        self.deepseek.as_ref().is_some_and(|channel| {
            channel.installed()
                && (channel.hosted(std::time::SystemTime::now())
                    || self.desktop(Provider::DeepSeek))
        })
    }
    /// Tests look for desktop apps in fixture folders instead of this computer's.
    #[cfg(any(test, feature = "fixtures"))]
    pub fn set_application_folders(&mut self, folders: Vec<PathBuf>) {
        self.applications = Some(folders);
    }
    /// Tests name Pi's and OpenCode's command lines instead of this
    /// computer's: those not named are not installed.
    #[cfg(any(test, feature = "fixtures"))]
    pub fn set_commands(&mut self, commands: HashMap<Provider, PathBuf>) {
        self.commands = Some(commands);
    }
    /// The command line of a client started as a run (Pi, OpenCode), looked
    /// up on disk each time, so one installed or removed while the app runs
    /// is offered or dropped at once. The desktop app of OpenCode carries one.
    fn command(&self, provider: Provider) -> Option<PathBuf> {
        if let Some(commands) = &self.commands {
            return commands.get(&provider).cloned();
        }
        let name = match provider {
            Provider::Pi => "pi",
            Provider::OpenCode => "opencode",
            _ => return None,
        };
        crate::os::find_engine(&self.config.home, &self.client_environment(), name)
    }
    /// The environment a started client sees: this app's own, with the
    /// configured variables and the command folders on its `PATH`.
    fn client_environment(&self) -> HashMap<String, String> {
        std::env::vars()
            .filter(|(key, _)| !host::is_path_key(key))
            .chain(environment(&self.config))
            .collect()
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
    /// Whether the client's desktop app is installed (cached for 60s when
    /// probing the host live): Claude (macOS), Codex/ChatGPT,
    /// Antigravity/Antigravity IDE, DeepSeek Harness.
    fn desktop(&self, provider: Provider) -> bool {
        if let Some(entries) = &self.applications {
            return desktop_in(entries, provider);
        }
        self.desktop_cached(provider, std::time::Instant::now(), |provider| {
            host::desktop_installed(&self.config.home, provider)
        })
    }
    fn desktop_cached(
        &self,
        provider: Provider,
        now: std::time::Instant,
        probe: impl FnOnce(Provider) -> bool,
    ) -> bool {
        if let Ok(cache) = self.desktop_cache.lock() {
            if let Some(&(at, installed)) = cache.get(&provider) {
                if now.saturating_duration_since(at) < DESKTOP_CACHE_TTL {
                    return installed;
                }
            }
        }
        let installed = probe(provider);
        if let Ok(mut cache) = self.desktop_cache.lock() {
            cache.insert(provider, (now, installed));
        }
        installed
    }
}

/// Whether `entries` (macOS application folders, Windows desktop executable
/// paths, or folders containing them) includes `provider`'s desktop app.
/// Windows Claude Desktop is intentionally omitted until `claude://resume`
/// hand-off is verified on Windows.
pub(crate) fn desktop_in(entries: &[PathBuf], provider: Provider) -> bool {
    let (bundles, executables): (&[&str], &[&str]) = match provider {
        Provider::Claude => (&["Claude.app"], &[]),
        Provider::Codex => (&["Codex.app", "ChatGPT.app"], &["Codex.exe", "ChatGPT.exe"]),
        Provider::Antigravity => (
            &["Antigravity.app", "Antigravity IDE.app"],
            &["Antigravity.exe", "Antigravity IDE.exe"],
        ),
        Provider::DeepSeek => (&["DeepSeek Harness.app"], &["DeepSeek Harness.exe"]),
        Provider::Cursor
        | Provider::Trae
        | Provider::WorkBuddy
        | Provider::ZCode
        | Provider::Qoder
        | Provider::Kimi
        | Provider::Pi
        | Provider::OpenCode => return false,
    };
    entries.iter().any(|entry| {
        if entry.is_file() {
            return entry
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| executables.iter().any(|exe| name.eq_ignore_ascii_case(exe)));
        }
        if !entry.is_dir() {
            return false;
        }
        bundles.iter().any(|bundle| entry.join(bundle).is_dir())
            || executables.iter().any(|exe| entry.join(exe).is_file())
    })
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
