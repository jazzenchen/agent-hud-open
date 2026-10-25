//! The application service shared by desktop, CLI and MCP transports.
use agent_hud_core::{AccountProjection, AgentHudCore, SourceRoot};
use agent_hud_platform::{AccountService, FileChanges, NativePlatform};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

mod demo;
mod log_reminders;
#[cfg(test)]
mod tests;
pub use agent_hud_core::conversation_feed;
pub use agent_hud_core::demo::DemoScript;
pub use agent_hud_core::l10n;
pub use agent_hud_core::projects;
pub use agent_hud_core::replies;
pub use agent_hud_platform::claude_plugin;
pub use agent_hud_platform::hooks;
pub use agent_hud_platform::instance;
pub use agent_hud_platform::local_tools;
pub use agent_hud_platform::CollectionGate;

/// Every collecting entry point claims the data directory before constructing
/// a Runtime. Owning client hooks additionally claims their shared resource.
/// A preview may run beside a release only with a separate data directory.
pub fn claim_collector(
    owns_hooks: bool,
    executable: Option<&std::path::Path>,
) -> Result<instance::CollectorLocks, instance::CollectorError> {
    let platform = NativePlatform::new().map_err(instance::CollectorError::Unavailable)?;
    let hooks = owns_hooks.then(|| platform.shared_instance_lock());
    instance::claim_collector(&platform.instance_lock(), hooks.as_deref(), executable)
}

/// Path discovery only: a tools client must never construct a data collector.
pub fn local_tools_directory() -> Result<PathBuf, String> {
    Ok(NativePlatform::new()?.data_dir())
}

/// The user's home directory, as the platform adapter resolves it.
pub fn home_directory() -> Result<PathBuf, String> {
    Ok(NativePlatform::new()?.home_dir().to_owned())
}

/// Read-only startup seed discovery, with no Runtime, collector or account jobs.
pub fn legacy_preferences() -> Result<Option<Value>, String> {
    NativePlatform::new()?.legacy_preferences()
}

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
        Self::new_with_accounts(data_dir, true)
    }
    /// The desktop's single startup-mode owner disables account I/O in --demo.
    pub fn new_with_accounts(
        data_dir: Option<PathBuf>,
        read_accounts: bool,
    ) -> Result<Self, String> {
        let seed = legacy_preferences()?.and_then(|value| value.get("settings").cloned());
        Self::new_with_preferences(data_dir, read_accounts, seed)
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
            .unwrap_or_else(|p| p.into_inner())
            .observe_local_clients()?;
        core.set_platform(platform.name());
        core.set_provider_discovery(platform.discover_providers());
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
            .unwrap_or_else(|p| p.into_inner())
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
            executable: std::env::current_exe()
                .map_err(|_| "The application executable is unavailable")?,
            canonical_executable: Some(canonical_executable),
        };
        let mut hooks = self.hooks.lock().unwrap_or_else(|p| p.into_inner());
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
            .unwrap_or_else(|p| p.into_inner())
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
    /// Read the facts the application already owns, without advancing ingestion
    /// or applying a newly completed account job as a side effect of an MCP call.
    pub fn local_query(&mut self, method: &str, params: Value) -> Result<Value, String> {
        match method {
            "statistics" | "session_detail" | "sessions" | "usage_buckets" => {
                self.core.request(method, params)
            }
            _ => Err("This operation is not a local cached query".into()),
        }
    }
    pub fn set_presentation(&mut self, key: &str, value: &Value) -> Result<Value, String> {
        self.core.set_presentation(key, value)
    }
    pub fn set_presentation_values(&mut self, values: &Value) -> Result<Value, String> {
        self.core.set_presentation_values(values)
    }
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        if matches!(
            method,
            "credentials_status" | "set_credentials" | "remove_credentials"
        ) {
            let accounts = self
                .accounts
                .as_ref()
                .ok_or("Account credentials are unavailable in this runtime")?;
            return match method {
                "credentials_status" => Ok(accounts.credentials_status()),
                "set_credentials" => accounts.set_credentials(&params),
                _ => accounts.remove_credentials(&params),
            };
        }
        if method == "open_session" {
            let id = params["id"]
                .as_str()
                .ok_or_else(|| l10n::text("缺少会话 ID", "Session ID is required"))?;
            let detail = self.core.request("session_detail", params.clone())?;
            if self.is_demo() {
                // A demo session has nowhere to go; the HUD hands over as it would.
                return Ok(json!({"opened":true,"action":"demo"}));
            }
            if self
                .platform
                .navigate_session(&detail["session"]["navigationTarget"])?
            {
                return Ok(json!({"opened":true,"action":"native_session"}));
            }
            let path = self.core.transcript_path(id)?;
            self.platform.reveal_transcript(&path)?;
            return Ok(json!({"opened":true,"action":"reveal_transcript"}));
        }
        if method == "answer_request" {
            return self.core.request(method, params);
        }
        if method == "antigravity_cli_status" {
            return Ok(self
                .hooks
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .antigravity_cli_status());
        }
        if method == "set_setting" {
            if params["key"] == "antigravityCliSettings" {
                if self.is_demo() {
                    return Err(l10n::text(
                        "演示模式不会更改 CLI 配置。",
                        "The demo leaves CLI settings unchanged.",
                    ));
                }
                let next = params["value"]
                    .as_array()
                    .ok_or("CLI settings must be a list of paths")?
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .filter(|path| {
                                !path.is_empty() && path.trim() == *path && !path.contains('\0')
                            })
                            .map(PathBuf::from)
                            .ok_or("Each CLI setting must name a path")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let hooks = self.hooks.lock().unwrap_or_else(|p| p.into_inner());
                let change = hooks
                    .set_antigravity_cli_accounts(&self.core.antigravity_cli_settings(), &next)?;
                let result = self.core.request(
                    method,
                    json!({"key":"antigravityCliSettings","value":change.settings()}),
                );
                if result.is_err() {
                    change.rollback().map_err(|error| format!("CLI settings could not be saved, and restoring the previous configuration failed: {error}"))?;
                }
                drop(hooks);
                return result.map(|result| self.decorate_snapshot(result, true));
            }
            if params["key"] == "codexHomes" {
                if let Some(accounts) = &self.accounts {
                    accounts.validate_account_homes(&params["value"])?;
                }
            }
            let result = self.core.request(method, params)?;
            return Ok(self.decorate_snapshot(result, true));
        }
        if method == "request_snapshot" {
            self.ingest_pending_hook_events();
            let method = if self.apply_log_reminders() || self.core.has_reminded_reads() {
                "reminder_snapshot"
            } else {
                method
            };
            let result = self.core.request(method, params)?;
            return Ok(self.decorate_snapshot(result, false));
        }
        // All transports read these from committed facts. An on-demand query
        // must not consume the collector's file notifications or apply a newly
        // completed account job as an incidental side effect.
        if matches!(
            method,
            "statistics" | "session_detail" | "sessions" | "usage_buckets"
        ) {
            return self.local_query(method, params);
        }
        // This boundary runs on the collector/blocking worker, not desktop setup.
        if let Some(path) = self.legacy_database.take() {
            self.core.set_legacy_quota_database(Some(path));
        }
        self.core.file_changes(self.changes.consume());
        self.ingest_pending_hook_events();
        self.apply_log_reminders();
        if let Some(accounts) = &self.accounts {
            if method == "refresh" {
                accounts.refresh();
            }
            self.core.set_account_projection(accounts.projection());
        }
        // Discovery checks installation/configuration only at startup or an explicit
        // refresh. Routine snapshot polls do not repeatedly inspect PATH or credentials.
        if method == "refresh" && self.discover_host {
            self.core
                .set_provider_discovery(self.platform.discover_providers());
        }
        let result = self.core.request(method, params)?;
        Ok(self.decorate_snapshot(result, true))
    }
    fn ingest_pending_hook_events(&mut self) {
        let events = self
            .hooks
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .drain_events();
        let events = events
            .into_iter()
            .filter(|event| {
                if event.kind == "account" {
                    if let Some(accounts) = &self.accounts {
                        accounts.account_changed(&event.source);
                    }
                    false
                } else {
                    true
                }
            })
            .collect();
        self.core.ingest_hook_events(events);
    }

    fn decorate_snapshot(&self, mut result: Value, update_accounts: bool) -> Value {
        if result.get("schemaVersion").is_some() {
            let hooks = self.hooks.lock().unwrap_or_else(|p| p.into_inner());
            let desired = result["settings"]["clientHooks"].as_bool().unwrap_or(false);
            if let Some(connection) = result["connection"].as_object_mut() {
                connection.insert("platformCapabilities".into(), self.platform.capabilities());
                connection.insert("clientHooks".into(), hooks.status(desired));
            }
            if let Some(sessions) = result.get("sessions").and_then(Value::as_array) {
                hooks.update_sessions(sessions);
            }
            if update_accounts {
                if let Some(accounts) = &self.accounts {
                    accounts.update_sources(&result);
                }
            }
        }
        result
    }
}
