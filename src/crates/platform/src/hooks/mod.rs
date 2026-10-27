//! Public, per-user client callbacks. Constructing this owner neither installs
//! hooks nor starts a collector. Only an explicit configure(true) opens IPC.
use agent_hud_agents::HookEvent;
use agent_hud_core::requests::Requests;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

mod clients;
mod completion;
mod endpoint;
mod entry;
mod inbox;
mod install;
mod statusline_install;
pub use statusline_install::AntigravityCliChange;
mod observers;
pub(crate) mod origins;
pub use observers::{apply_observer_plan, observer_plan, ObserverPlan};
mod reminders;
mod reviewer;
mod statusline;
mod transcript;
mod turn_log;
// The callback channel: a Unix socket, or a named pipe on Windows.
#[cfg(unix)]
#[path = "unix.rs"]
mod transport;
#[cfg(windows)]
#[path = "windows.rs"]
mod transport;
pub use agent_hud_agents::windows_hook_handler;
pub use entry::{entry, handle_with_io};
pub use inbox::{record_delivery, record_event};
pub use install::{apply_plan, install_plan, InstallContext, InstallPlan};
pub(crate) use install::{target as settings_target, write_contents as write_settings};
pub use statusline::account_reports;

pub const PAYLOAD_LIMIT: usize = 1024 * 1024;
pub type Wake = Arc<dyn Fn() + Send + Sync>;

pub fn socket_path(data: &Path) -> PathBuf {
    data.join("hooks/callback.sock")
}
pub fn supported() -> bool {
    true
}

pub struct Hooks {
    data: PathBuf,
    clients: clients::Clients,
    requests: Requests,
    reminders: reminders::Reminders,
    inbox: Option<inbox::Inbox>,
    wake: Wake,
    waker: Arc<Mutex<Wake>>,
    observers: Mutex<Option<observers::Observers>>,
    antigravity: Option<crate::clients::antigravity::permissions::Observer>,
    deepseek: Option<crate::clients::deepseek::questions::Observer>,
    deepseek_plugin: Option<crate::deepseek_plugin::asks::Observer>,
    server: Option<transport::Server>,
    desktop: Option<crate::clients::codex::desktop::CodexDesktopObserver>,
}
impl Hooks {
    /// Creates a disabled owner. Runtime owns this alongside its existing core.
    pub fn start(data: PathBuf, requests: Requests, wake: Wake) -> Result<Self, String> {
        let waker = Arc::new(Mutex::new(wake));
        let relay = waker.clone();
        let wake: Wake = Arc::new(move || {
            let wake = relay.lock().unwrap_or_else(|p| p.into_inner()).clone();
            wake();
        });
        let inbox = Some(inbox::Inbox::new(&data, wake.clone())?);
        Ok(Self {
            observers: Mutex::new(None),
            antigravity: None,
            deepseek: None,
            deepseek_plugin: None,
            clients: clients::Clients::default(),
            data,
            requests,
            reminders: reminders::Reminders::default(),
            inbox,
            wake,
            waker,
            server: None,
            desktop: None,
        })
    }
    /// Explicit live-host discovery; isolated with_sources runtimes never call this.
    pub fn observe_local_clients(&self) -> Result<(), String> {
        self.observe_clients_at(crate::os::home_dir()?, std::env::vars().collect());
        Ok(())
    }
    pub fn observe_clients_at(
        &self,
        home: PathBuf,
        environment: std::collections::HashMap<String, String>,
    ) {
        *self.observers.lock().unwrap_or_else(|p| p.into_inner()) = Some(
            observers::Observers::new(&home, &environment, self.wake.clone()),
        );
    }
    pub fn delegate_owner(&mut self, context: InstallContext) -> Result<(), String> {
        self.clients.delegate(context)
    }
    pub fn status(&self, desired: bool) -> serde_json::Value {
        self.clients.status(self.is_listening(), desired)
    }
    pub fn antigravity_cli_status(&self) -> serde_json::Value {
        statusline_install::status(self.clients.context())
    }
    /// Only an explicit source-setting Save action changes CLI configuration.
    /// Roll back the returned transaction if saving core preferences fails.
    pub fn set_antigravity_cli_accounts(
        &self,
        previous: &[PathBuf],
        next: &[PathBuf],
    ) -> Result<AntigravityCliChange, String> {
        statusline_install::set_accounts(self.clients.context(), &self.data, previous, next)
    }
    /// Only an explicit preference action installs/uninstalls client adapters.
    /// Process shutdown calls configure(false), leaving the next launch's hooks in place.
    pub fn set_enabled(&mut self, enabled: bool) -> Result<(), String> {
        if enabled {
            let previous = self.is_listening();
            self.configure(true)?;
            if let Err(error) = self.clients.install(true) {
                if !previous {
                    let _ = self.configure(false);
                }
                return Err(error);
            }
            Ok(())
        } else {
            let installed = self.clients.install(false);
            self.configure(false)?;
            // A host without a delegated owner may always stop its listener.
            if self.clients.status(false, false)["installable"] == true {
                installed?;
            }
            Ok(())
        }
    }
    pub fn configure(&mut self, enabled: bool) -> Result<(), String> {
        if !enabled {
            self.antigravity.take();
            self.deepseek.take();
            self.deepseek_plugin.take();
            self.desktop.take();
            self.server.take();
            return Ok(());
        }
        if self.is_listening() {
            return Ok(());
        }
        self.server.take();
        self.desktop.take();
        let server = transport::Server::start(
            &self.data,
            self.requests.clone(),
            self.wake.clone(),
            self.reminders.clone(),
        )?;
        // Each client's native channel starts only where its descriptor
        // says it runs.
        use crate::clients::{self, antigravity, codex, deepseek};
        use agent_hud_agents as agents;
        let desktop = clients::runs(&agents::codex::DESKTOP)
            .then(|| {
                codex::desktop::CodexDesktopObserver::start(
                    codex::data_directory()?,
                    self.requests.clone(),
                    self.wake.clone(),
                )
            })
            .transpose()?;
        if clients::runs(&agents::antigravity::LOCAL_SERVICE) {
            self.antigravity = Some(antigravity::permissions::Observer::start(
                self.requests.clone(),
            )?);
        }
        if clients::runs(&agents::deepseek::QUESTIONS) {
            self.deepseek = Some(deepseek::questions::Observer::start(
                self.requests.clone(),
                self.wake.clone(),
            )?);
        }
        if clients::runs(&agents::deepseek::PLUGIN) {
            use crate::deepseek_plugin::{asks, channel_directory, DeepSeekChannel};
            self.deepseek_plugin = Some(asks::Observer::start(
                DeepSeekChannel::new(channel_directory(&self.data)),
                self.requests.clone(),
                self.wake.clone(),
            )?);
        }
        self.server = Some(server);
        self.desktop = desktop;
        Ok(())
    }
    pub fn is_listening(&self) -> bool {
        self.server
            .as_ref()
            .is_some_and(transport::Server::is_listening)
    }
    pub fn drain_events(&self) -> Vec<HookEvent> {
        let mut events = self
            .inbox
            .as_ref()
            .map(inbox::Inbox::drain)
            .unwrap_or_default();
        if let Some(observers) = self
            .observers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_mut()
        {
            events.extend(observers.drain());
        }
        for event in &events {
            if let Some(path) = event
                .data
                .get("transcriptPath")
                .and_then(serde_json::Value::as_str)
            {
                let flag = match event.kind.as_str() {
                    "origin" => "--session-origin-hook",
                    "attention" => "--attention-hook",
                    "compaction" => "--compaction-hook",
                    "completion" => "--completion-hook",
                    _ => continue,
                };
                self.reminders.record(
                    &event.source,
                    flag,
                    &serde_json::json!({"transcript_path":path}),
                );
            }
        }
        events.sort_by_key(|event| event.at_ms);
        events
    }
    pub fn drain_log_reminders(&self) -> Vec<PathBuf> {
        self.reminders.drain()
    }
    pub fn requests(&self) -> Requests {
        self.requests.clone()
    }
    pub fn set_waker(&self, wake: Wake) {
        *self.waker.lock().unwrap_or_else(|p| p.into_inner()) = wake;
    }
    pub fn data_dir(&self) -> &Path {
        &self.data
    }
    pub fn update_sessions(&self, sessions: &[serde_json::Value]) {
        if let Some(observer) = &self.desktop {
            observer.update_sessions(sessions);
        }
    }
    pub fn stop(&mut self) {
        let _ = self.configure(false);
    }
}
impl Drop for Hooks {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Whether the client's transcript shows a permission request answered in
/// its own window, so a waiting callback can stop waiting for the HUD.
pub(crate) fn settles_in_transcript(source: &str) -> bool {
    agent_hud_agents::hooks::hooks_of(source)
        .ok()
        .and_then(|(_, hooks)| hooks.permission.as_ref())
        .is_some_and(|permission| permission.settles_in_transcript)
}
