//! Native boundary: environment, per-user paths and operating-system actions.
//! It does not parse transcripts, price tokens or decide session state.
use agent_hud_core::{ProviderDiscovery, SourceRoot};
mod discovery;
pub use discovery::discover_at;
mod file_changes;
pub mod instance;
pub use file_changes::FileChanges;
mod accounts;
pub mod claude_plugin;
pub mod clients;
mod data_folder;
pub mod deepseek_plugin;
pub mod delivery;
pub mod hooks;
pub mod hud_tools;
pub mod json_text;
#[cfg(windows)]
pub mod local_ipc;
pub mod local_tools;
mod navigation;
mod os;
#[cfg(target_os = "windows")]
#[path = "windows/session.rs"]
pub mod windows_session;
pub use accounts::{AccountService, CollectionGate, WebSignInDesk};
pub use navigation::session_url;
#[cfg(target_os = "macos")]
pub use os::registered_applications;

/// The processes above `pid` in the same application bundle, nearest first
/// (none outside a bundle, or on a host without bundles).
pub fn app_ancestors(pid: u32) -> Vec<u32> {
    os::app_ancestors(pid)
}

/// The program client hooks run: this executable, or on Linux the AppImage it
/// was started from, whose temporary mount changes with every launch (Open #11).
pub fn launched_executable() -> std::io::Result<std::path::PathBuf> {
    os::launched_executable()
}
/// A client's own command line (`claude`, `codex`), where the account readers
/// find it: an app's bundled copy as well as one on PATH.
pub fn client_executable(name: &str) -> Option<std::path::PathBuf> {
    let home = os::home_dir().ok()?;
    let env = std::env::vars().collect::<os::Environment>();
    os::find_engine(&home, &env, name)
}
use std::path::{Path, PathBuf};

pub struct NativePlatform {
    home: PathBuf,
}

impl NativePlatform {
    pub fn new() -> Result<Self, String> {
        let home = os::home_dir()?;
        Ok(Self { home })
    }
    pub fn home_dir(&self) -> &Path {
        &self.home
    }
    pub fn name(&self) -> &'static str {
        std::env::consts::OS
    }
    pub fn discover_providers(&self) -> Vec<ProviderDiscovery> {
        discovery::discover(&self.home)
    }
    pub fn account_service(
        &self,
        data_dir: PathBuf,
        gate: CollectionGate,
    ) -> Result<AccountService, String> {
        AccountService::new(
            self.home.clone(),
            data_dir,
            std::collections::HashSet::new(),
            gate,
        )
    }
    pub fn legacy_preferences() -> Option<serde_json::Value> {
        os::legacy_preferences()
    }
    pub fn legacy_quota_database(&self) -> Option<PathBuf> {
        os::legacy_quota_database(&self.home)
    }
    /// The data folder: the one `AGENTHUD_DATA_DIR` names, else the app's
    /// own (`data_folder`).
    pub fn data_dir(&self) -> PathBuf {
        if let Some(path) = directory(DATA_DIR_VARIABLE) {
            return path;
        }
        data_folder::resolve(&os::data_parent(&self.home), os::DATA_FOLDER)
    }
    /// An earlier build's data folder that has not been moved yet. None when
    /// `AGENTHUD_DATA_DIR` names the folder: then nothing is moved.
    pub fn earlier_data_dir(&self) -> Option<PathBuf> {
        if directory(DATA_DIR_VARIABLE).is_some() {
            return None;
        }
        data_folder::earlier(&os::data_parent(&self.home))
    }
    /// Moves an earlier build's data folder to the app's own. Only a copy
    /// holding `shared_instance_lock` may call it, before it claims its data.
    pub fn adopt_earlier_data(&self) -> Result<(), String> {
        if directory(DATA_DIR_VARIABLE).is_some() {
            return Ok(());
        }
        data_folder::adopt(&os::data_parent(&self.home), os::DATA_FOLDER)
            .map_err(|error| error.to_string())
    }
    /// The lock every Agent HUD that owns client hooks claims, whatever its
    /// data directory: agent-hud-native-open's `~/Library/Caches/app.agenthud/instance.lock`
    /// on macOS, so this app and the Swift app never run side by side.
    pub fn shared_instance_lock(&self) -> PathBuf {
        os::caches_dir(&self.home)
            .join("app.agenthud")
            .join("instance.lock")
    }
    /// The lock that keeps two copies from collecting into one data directory.
    pub fn instance_lock(&self) -> PathBuf {
        self.data_dir().join(data_folder::LOCK)
    }
    /// Implementation and verification are separate: portable readers remain
    /// usable on Windows/Linux, whose runtime validation is still pending.
    pub fn capabilities(&self) -> serde_json::Value {
        os::capabilities()
    }
    pub fn source_roots(&self) -> Vec<SourceRoot> {
        os::source_roots(&self.home)
    }
    /// The Claude desktop app's data folders on this host, for its session index.
    pub fn claude_desktop_bases(&self) -> Vec<PathBuf> {
        os::claude_desktop_bases(&self.home, &std::env::vars().collect::<os::Environment>())
    }
    /// This host's word on whether the processes Claude Code registers still run.
    pub fn claude_pids(&self) -> std::sync::Arc<dyn agent_hud_agents::claude::processes::Pids> {
        std::sync::Arc::new(os::HostPids)
    }
    pub fn claude_projects_supported(&self) -> bool {
        os::OPENS_CLAUDE_PROJECTS
    }
    /// The core resolves a known session and validates source membership before this call.
    pub fn navigate_session(&self, target: &serde_json::Value) -> Result<bool, String> {
        if target["kind"] == "claude-project-thread" {
            return Ok(navigation::session_url(target)
                .is_some_and(|url| os::open_url_in_app("com.anthropic.claudefordesktop", &url)));
        }
        if matches!(
            target["kind"].as_str(),
            Some("claude-background-job" | "claudeBackgroundJob")
        ) {
            return navigation::open_claude_job(&self.home, target);
        }
        if matches!(
            target["kind"].as_str(),
            Some("antigravity-conversation" | "antigravityConversation")
        ) {
            return navigation::open_antigravity(
                &self.home,
                target["id"].as_str().unwrap_or(""),
                target["cwd"].as_str(),
            );
        }
        if let Some(folder) = navigation::cursor_folder(target) {
            if os::open_folder_in_app(agent_hud_agents::cursor::BUNDLE, folder) {
                return Ok(true);
            }
        }
        let Some(url) = navigation::session_url(target) else {
            if matches!(
                target["kind"].as_str(),
                Some(
                    "claude-desktop"
                        | "claude-desktop-session"
                        | "claudeDesktopSession"
                        | "claude-cowork"
                        | "claude-cowork-session"
                        | "claudeCoworkSession"
                )
            ) {
                return Ok(navigation::activate_app("com.anthropic.claudefordesktop"));
            }
            return Ok(false);
        };
        os::open_session_url(&url)?;
        Ok(true)
    }
    /// Brings a client's own app forward, starting it when it is closed, for
    /// a step only the user can take there (allowing Agent HUD's hooks in
    /// Codex). False where this host has no such app or cannot open it.
    pub fn open_client(&self, source: &str) -> bool {
        let Some(agent) = agent_hud_agents::by_id(source) else {
            return false;
        };
        agent.discovery.bundles.iter().any(|bundle| {
            (os::app_running(bundle) && os::activate_app(bundle))
                || (os::registered_bundle(bundle) && os::launch_app(bundle))
        })
    }
    pub fn reveal_transcript(&self, path: &Path) -> Result<(), String> {
        os::reveal_transcript(path)
    }
}

/// Names the data folder in development and tests.
const DATA_DIR_VARIABLE: &str = "AGENTHUD_DATA_DIR";

fn directory(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
