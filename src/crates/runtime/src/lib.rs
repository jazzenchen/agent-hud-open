//! The application service shared by desktop, CLI and MCP transports.
use agent_hud_core::{AccountProjection, AgentHudCore, SourceRoot};
pub use agent_hud_platform::WebSignInDesk;
use agent_hud_platform::{AccountService, FileChanges, NativePlatform};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
};

mod demo;
mod log_reminders;
mod request;
mod startup;
#[cfg(test)]
mod tests;
pub use agent_hud_core::conversation_feed;
pub use agent_hud_core::demo::DemoScript;
pub use agent_hud_core::l10n;
pub use agent_hud_core::projects;
pub use agent_hud_core::replies;
pub use agent_hud_platform::app_ancestors;
pub use agent_hud_platform::claude_plugin;
pub use agent_hud_platform::client_executable;
pub use agent_hud_platform::deepseek_plugin;
pub use agent_hud_platform::delivery;
pub use agent_hud_platform::hooks;
pub use agent_hud_platform::hud_tools;
pub use agent_hud_platform::instance;
pub use agent_hud_platform::local_tools;
#[cfg(target_os = "macos")]
pub use agent_hud_platform::registered_applications;
#[cfg(target_os = "windows")]
pub use agent_hud_platform::windows_session;
pub use agent_hud_platform::CollectionGate;
pub use startup::{claim_collector, home_directory, legacy_preferences, local_tools_directory};

pub struct Runtime {
    core: AgentHudCore,
    platform: NativePlatform,
    discover_host: bool,
    changes: FileChanges,
    accounts: Option<AccountService>,
    legacy_database: Option<PathBuf>,
    hooks: Arc<Mutex<hooks::Hooks>>,
    log_reminders: log_reminders::LogReminders,
    gate: CollectionGate,
    demo: Option<demo::Demo>,
}

impl Runtime {
    /// Startup must use the loaded preference rather than wait for a snapshot.
    pub fn launch_at_login_enabled(&self) -> bool {
        self.core.launch_at_login_enabled()
    }

    pub fn new(data_dir: Option<PathBuf>) -> Result<Self, String> {
        let seed = legacy_preferences().and_then(|value| value.get("settings").cloned());
        Self::new_with_preferences(data_dir, true, seed)
    }
    /// The product host reads the legacy seed once and shares it with its
    /// presentation owner. An explicit None never triggers a second read.
    pub fn new_with_preferences(
        data_dir: Option<PathBuf>,
        read_accounts: bool,
        initial_settings: Option<Value>,
    ) -> Result<Self, String> {
        let platform = NativePlatform::new()?;
        let roots = platform.source_roots();
        let changes = FileChanges::new(&roots);
        let data_dir = data_dir.unwrap_or_else(|| platform.data_dir());
        let mut core = AgentHudCore::new(roots, data_dir.clone());
        if let Some(seed) = initial_settings.as_ref() {
            core.seed_initial_preferences(seed)?;
        }
        let hooks = Arc::new(Mutex::new(hooks::Hooks::start(
            data_dir.clone(),
            core.requests(),
            Arc::new(|| {}),
        )?));
        hooks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .observe_local_clients()?;
        core.set_platform(platform.name());
        core.set_provider_discovery(platform.discover_providers());
        core.set_claude_desktop_bases(platform.claude_desktop_bases());
        core.set_claude_pids(platform.claude_pids());
        core.set_claude_projects_supported(platform.claude_projects_supported());
        let legacy_database = platform.legacy_quota_database();
        let gate = CollectionGate::default();
        let accounts = if read_accounts {
            match platform.account_service(data_dir, Arc::clone(&gate)) {
                Ok(accounts) => Some(accounts),
                Err(error) => {
                    let mut projection = AccountProjection::default();
                    for id in ["claude", "codex", "cursor", "deepseek", "antigravity"] {
                        projection.providers.insert(
                            id.into(),
                            json!({"status":"error","checkedAt":null,"notice":error}),
                        );
                    }
                    core.set_account_projection(projection);
                    None
                }
            }
        } else {
            None
        };
        Ok(Self {
            core,
            platform,
            discover_host: true,
            changes,
            accounts,
            legacy_database,
            hooks,
            log_reminders: log_reminders::LogReminders::default(),
            gate,
            demo: None,
        })
    }
    pub fn with_sources(roots: Vec<SourceRoot>, data_dir: PathBuf) -> Result<Self, String> {
        let platform = NativePlatform::new()?;
        let changes = FileChanges::new(&roots);
        let mut core = AgentHudCore::new(roots, data_dir.clone());
        let hooks = Arc::new(Mutex::new(hooks::Hooks::start(
            data_dir,
            core.requests(),
            Arc::new(|| {}),
        )?));
        core.set_platform(platform.name());
        core.set_claude_pids(platform.claude_pids());
        Ok(Self {
            core,
            platform,
            discover_host: false,
            changes,
            accounts: None,
            legacy_database: None,
            hooks,
            log_reminders: log_reminders::LogReminders::default(),
            gate: CollectionGate::default(),
            demo: None,
        })
    }
    /// Account readings hold this gate. A host holds it for every collection
    /// pass ("snapshot", "refresh", "request_snapshot"), taken before any lock
    /// its UI requests also wait on, so local and account reads stay serial.
    pub fn collection_gate(&self) -> CollectionGate {
        Arc::clone(&self.gate)
    }
    pub fn set_change_waker(&mut self, wake: Arc<dyn Fn() + Send + Sync>) {
        self.changes.set_waker(Arc::clone(&wake));
        if let Some(accounts) = &self.accounts {
            accounts.set_waker(wake);
        }
    }
    /// Requests wake the same serial worker immediately, independently of the
    /// transcript collector's 2s coalescing of file events.
    pub fn set_request_waker(&mut self, wake: Arc<dyn Fn() + Send + Sync>) {
        self.core.requests().set_waker(Arc::clone(&wake));
        self.hooks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .set_waker(wake);
    }
    /// The product host's way to send words into a session as its user's
    /// next message (`replies`); None leaves the Claude desktop app's
    /// questions to be answered in its window.
    pub fn set_session_replies(&mut self, replies: Option<Arc<dyn replies::SessionReplies>>) {
        self.core.set_session_replies(replies);
    }
    /// The store that keeps conversations (`conversation_feed`).
    pub fn set_conversation_sink(&mut self, sink: Option<conversation_feed::Sink>) {
        self.core.set_conversation_sink(sink);
    }
    /// Configure/stop this independent owner outside the desktop Backend lock.
    /// It uses the core's existing Requests; it never creates another Runtime.
    pub fn hooks_handle(&self) -> Arc<Mutex<hooks::Hooks>> {
        Arc::clone(&self.hooks)
    }
    /// The native product host, never an RPC payload, supplies its approved fixed executable.
    pub fn delegate_hook_owner(&mut self, canonical_executable: PathBuf) -> Result<(), String> {
        let context = hooks::InstallContext {
            home: self.platform.home_dir().to_owned(),
            environment: std::env::vars().collect(),
            executable: agent_hud_platform::launched_executable()
                .map_err(|_| "The application executable is unavailable")?,
            canonical_executable: Some(canonical_executable),
        };
        let mut hooks = self.hooks.lock().unwrap_or_else(PoisonError::into_inner);
        // Nothing is in yet, so this only says which opt-in hooks go in.
        hooks.set_opted_in(self.core.opt_in_approvals())?;
        hooks.delegate_owner(context)?;
        if self.core.client_hooks_enabled() {
            hooks.set_enabled(true)?;
        }
        Ok(())
    }
    /// The desktop's Condvar worker waits for this deadline too.
    /// Whether local logs are still being indexed (`AgentHudCore::indexing`).
    pub fn indexing(&self) -> bool {
        self.core.indexing()
    }
    pub fn next_log_reminder(&self) -> Option<std::time::Instant> {
        self.log_reminders.next_look()
    }
    /// A hook's follow-up look: stats the logs it pointed at, and true when
    /// one changed and waits to be read. An unchanged look costs no pass.
    pub fn reminder_look(&mut self) -> bool {
        self.apply_log_reminders() || self.core.has_reminded_reads()
    }
    /// The look between passes (`AgentHudCore::look`): true when a pass is due.
    pub fn look(&mut self) -> bool {
        self.core.look()
    }
    /// When a pass is due with no event (`AgentHudCore::next_pass_due`).
    pub fn next_pass_due(&self) -> Option<std::time::Instant> {
        self.core.next_pass_due()
    }
    /// When the snapshot is due again with nothing changing (`AgentHudCore::next_due`).
    pub fn next_snapshot_due(&self) -> Option<std::time::Instant> {
        let due = u64::try_from(self.core.next_due()?).unwrap_or(0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis() as u64;
        Some(std::time::Instant::now() + std::time::Duration::from_millis(due.saturating_sub(now)))
    }
    fn apply_log_reminders(&mut self) -> bool {
        let paths = self
            .hooks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain_log_reminders();
        self.log_reminders.remind(paths);
        let due = self.log_reminders.due(std::time::Instant::now());
        !due.is_empty() && self.core.remind_paths(due) > 0
    }
    pub fn stop_accounts(&self) {
        if let Some(accounts) = &self.accounts {
            accounts.stop();
        }
    }
    pub fn set_presentation(&mut self, key: &str, value: &Value) -> Result<Value, String> {
        self.core.set_presentation(key, value)
    }
    pub fn set_presentation_values(&mut self, values: &Value) -> Result<Value, String> {
        self.core.set_presentation_values(values)
    }
    /// What the host's sign-in window works with (`WebSignInDesk`); none in
    /// a runtime that reads no accounts.
    pub fn web_sign_in_desk(&self) -> Option<WebSignInDesk> {
        self.accounts.as_ref().map(AccountService::web_sign_in_desk)
    }
}
