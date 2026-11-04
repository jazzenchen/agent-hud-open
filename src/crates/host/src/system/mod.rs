//! Explicit operating-system actions. No login registration happens merely by
//! constructing this owner. There are no system notifications: the HUD island
//! is the alert surface on every platform.
use serde_json::{json, Value};
use std::sync::Mutex;
use tauri::Manager;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(unix)]
mod unix;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
use linux as target;
#[cfg(target_os = "macos")]
use macos as target;
#[cfg(target_os = "windows")]
use windows as target;

// Each host file provides the same items:
//   cli_candidates(home, command, dirs)  where a client's command line may be
//                                        installed: `dirs` in order, with the host's
//                                        executable names, after any bundled copy
//   bundled_cli(command)                 a desktop app's own copy of a client's command
//                                        line (none outside Windows)
//   private_output(options)              a new file only this user can read
//   client_command_line(arguments)       a command for the host's interactive shell
//   configure_client_command(command)    client setup runs in the background, even
//                                        for a console executable
//   stop_client(child)
pub(crate) use target::{
    bundled_cli, cli_candidates, client_command_line, configure_client_command, private_output,
    stop_client,
};

pub struct System {
    identifier: String,
    cached: Mutex<Option<Value>>,
}
impl System {
    pub fn new(identifier: String) -> Self {
        Self {
            identifier,
            cached: Mutex::new(None),
        }
    }
    pub fn register_if_never_registered(&self) {
        if target::register_if_never_registered() {
            if let Ok(mut cached) = self.cached.lock() {
                *cached = None;
            }
        }
    }
    fn refresh(&self) -> Result<Value, String> {
        let status = target::status(&self.identifier)?;
        *self.cached.lock().map_err(|_| "System state unavailable")? = Some(status.clone());
        Ok(status)
    }
    pub fn project(&self, snapshot: &mut Value) {
        let mut status = self.cached.lock().ok().and_then(|cached| cached.clone());
        if status.is_none() {
            status = self.refresh().ok();
        }
        if let Some(status) = status {
            if let Some(enabled) = status["launchAtLogin"]["enabled"].as_bool() {
                if status["launchAtLogin"]["status"] != "disabled" {
                    // This is the OS's current registration, not a remembered desired toggle.
                    snapshot["settings"]["launchAtLogin"] = json!(enabled);
                }
            }
            snapshot["connection"]["system"] = status;
        }
    }
    /// A desktop host invokes this before saving its corresponding Core setting.
    /// None means this is not an OS action. Errors never claim the setting changed.
    pub fn set_setting(&self, key: &str, value: &Value) -> Result<Option<Value>, String> {
        if key != "launchAtLogin" {
            return Ok(None);
        }
        let enabled = value
            .as_bool()
            .ok_or("The system setting must be a boolean")?;
        target::set_login(&self.identifier, enabled)?;
        let status = self.refresh()?;
        Ok(Some(status["launchAtLogin"]["enabled"].clone()))
    }
}

#[tauri::command]
pub async fn native_system_status(app: tauri::AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<System>().refresh())
        .await
        .map_err(|_| "System status worker failed".to_owned())?
}
