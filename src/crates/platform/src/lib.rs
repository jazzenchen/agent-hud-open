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
pub mod hooks;
pub mod hud_tools;
pub mod json_text;
#[cfg(windows)]
pub mod local_ipc;
pub mod local_tools;
mod navigation;
mod os;
pub use accounts::{AccountService, CollectionGate};
pub use navigation::session_url;

/// A client's own command line (`claude`, `codex`), where the account readers
/// find it: an app's bundled copy as well as one on PATH.
pub fn client_executable(name: &str) -> Option<std::path::PathBuf> {
    let home = os::home_dir().ok()?;
    let env = std::env::vars().collect::<os::Environment>();
    os::engine_candidates(&home, &env, name)
        .into_iter()
        .find(|path| os::engine_executable(path))
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
    pub fn legacy_preferences(&self) -> Result<Option<serde_json::Value>, String> {
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
    /// The core resolves a known session and validates source membership before this call.
    pub fn navigate_session(&self, target: &serde_json::Value) -> Result<bool, String> {
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
