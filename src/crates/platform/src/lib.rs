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
pub mod hooks;
pub mod json_text;
#[cfg(windows)]
pub mod local_ipc;
pub mod local_tools;
mod navigation;
mod os;
pub use accounts::{AccountService, CollectionGate};
pub use navigation::session_url;
#[cfg(target_os = "macos")]
pub use os::registered_applications;
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
    pub fn data_dir(&self) -> PathBuf {
        if let Some(path) = directory("AGENTHUD_NEXT_DATA_DIR") {
            return path;
        }
        os::data_dir(&self.home)
    }
    /// The lock every Agent HUD that owns client hooks claims, whatever its
    /// data directory: agent-hud-native-open's `~/Library/Caches/app.agenthud/instance.lock`
    /// on macOS, so a Next build and an Open build never run side by side.
    pub fn shared_instance_lock(&self) -> PathBuf {
        os::caches_dir(&self.home)
            .join("app.agenthud")
            .join("instance.lock")
    }
    /// The lock that keeps two copies from collecting into one data directory.
    pub fn instance_lock(&self) -> PathBuf {
        self.data_dir().join("instance.lock")
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

fn directory(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
