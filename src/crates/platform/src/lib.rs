//! Native boundary: environment, per-user paths and operating-system actions.
//! It does not parse transcripts, price tokens or decide session state.
use agent_hud_core::{ProviderDiscovery, SourceRoot};
mod discovery;
pub use discovery::{discover_at, discover_clients_at};
mod data_places;
mod file_changes;
pub mod home_folder;
pub mod instance;
pub use file_changes::FileChanges;
mod accounts;
pub mod claude_plugin;
pub mod client_features;
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
/// A web page in one of `browsers` (bundle identifiers, the preferred first),
/// for a page that only works there, such as a Chrome Web Store listing: the
/// system's browser when it is one of them, else the first one installed,
/// else the system's browser all the same. Only `https` addresses, as
/// `NativePlatform::open_link`. A host that cannot name a browser opens the
/// system's.
pub fn open_link_in(url: &str, browsers: &[&str]) -> Result<bool, String> {
    web_link(url)?;
    let listed = |id: &str| {
        browsers
            .iter()
            .any(|browser| browser.eq_ignore_ascii_case(id))
    };
    if !os::default_browser().is_some_and(|id| listed(&id)) {
        if let Some(browser) = browsers
            .iter()
            .find(|browser| os::registered_bundle(browser))
        {
            if os::open_url_in_app(browser, url) {
                return Ok(true);
            }
        }
    }
    Ok(os::open_session_url(url).is_ok())
}

/// Only an `https` address opens in a browser, so a window's link can never
/// start a program or open a file.
fn web_link(url: &str) -> Result<(), String> {
    let web = url.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty() && !url.chars().any(|c| c.is_whitespace() || c.is_control())
    });
    web.then_some(())
        .ok_or_else(|| "Only https links open in the browser".into())
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
    pub fn discover_clients(&self) -> Vec<ProviderDiscovery> {
        discovery::discover_clients(&self.home)
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
    /// Implementation and verification are separate: portable readers remain
    /// usable on Windows/Linux, whose runtime validation is still pending.
    pub fn capabilities(&self) -> serde_json::Value {
        os::capabilities()
    }
    pub fn source_roots(&self) -> Vec<SourceRoot> {
        os::source_roots(&self.home)
    }
    pub fn source_roots_for(&self, settings: &serde_json::Value) -> Vec<SourceRoot> {
        os::extra_roots::with_extra_homes(self.source_roots(), &self.home, settings)
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
    /// A web page in the system's browser: only `https` addresses, so a
    /// window's link can never start a program or open a file.
    pub fn open_link(&self, url: &str) -> Result<bool, String> {
        web_link(url)?;
        Ok(os::open_session_url(url).is_ok())
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
