//! Settings → Agents → Agent plugins: Agent HUD's DeepSeek Harness plugin,
//! which shows the questions and approvals Harness sessions wait on in the
//! HUD, lets the app leave words for a running session, and tells the app
//! which session called one of its MCP tools. It is installed and removed
//! only when the user asks there: one row in Harness's home patch and the
//! module it loads. At launch, an installed plugin whose module changed with
//! this version of the app is written again under its new version. The
//! module, its row and the folder it shares with the app live in
//! `agent_hud_runtime::deepseek_plugin`.
use agent_hud_runtime::deepseek_plugin::{self as plugin, DeepSeekChannel};
use agent_hud_runtime::hud_tools::{Client, Places};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};
use tauri::Manager;

pub struct DeepSeekPlugin {
    channel: DeepSeekChannel,
    folder: PathBuf,
    owner: Mutex<()>,
}

impl DeepSeekPlugin {
    /// The plugin's channel and module folder, in the app's data folder.
    pub(crate) fn new() -> Result<Self, String> {
        let data = agent_hud_runtime::local_tools_directory()?;
        Ok(Self {
            channel: DeepSeekChannel::new(plugin::channel_directory(&data)),
            folder: plugin::plugin_directory(&data),
            owner: Mutex::new(()),
        })
    }

    pub(crate) fn channel(&self) -> &DeepSeekChannel {
        &self.channel
    }
}

/// Whether Harness is on this machine, whether its home patch loads the
/// plugin, and how many sessions run it now.
fn status(channel: &DeepSeekChannel, places: &Places) -> Value {
    json!({
        "found": Client::DeepSeek.present(places),
        "installed": plugin::installed(places),
        "sessions": channel.loaded(SystemTime::now()).len(),
    })
}

/// Writes the module and the row that loads it.
fn install(channel: &DeepSeekChannel, folder: &Path, places: &Places) -> Result<(), String> {
    if !Client::DeepSeek.present(places) {
        return Err(format!("{} is not installed", Client::DeepSeek.name()));
    }
    let file = channel.prepare_plugin(folder)?;
    plugin::set_row(places, Some((&file, channel.directory())))?;
    plugin::remove_other_versions(folder, &file);
    Ok(())
}

/// Takes the row out, then forgets every check-in, word and ask, and
/// removes the module.
fn remove(channel: &DeepSeekChannel, folder: &Path, places: &Places) -> Result<(), String> {
    plugin::set_row(places, None)?;
    channel.clear()?;
    plugin::remove_plugin(folder);
    Ok(())
}

/// At launch: when the user installed the plugin and this version of the
/// app carries another module, or keeps it elsewhere, writes it again. Never
/// installs a plugin the user has not, and only logs a failure.
pub(crate) fn repair_on_launch(app: &tauri::AppHandle) {
    // The demo leaves the user's Harness alone.
    if app.state::<super::AppState>().demo {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DeepSeekPlugin>();
        let Ok(_owner) = state.owner.lock() else {
            return;
        };
        let Ok(home) = super::mcp_clients::home() else {
            return;
        };
        let places = Places::current(&home);
        if let Err(error) = repair(&state.channel, &state.folder, &places) {
            eprintln!("The DeepSeek Harness plugin could not be brought up to date: {error}");
        }
    });
}

/// Writes the module and its row again when the row loads another version
/// or another folder. Returns whether it did.
fn repair(channel: &DeepSeekChannel, folder: &Path, places: &Places) -> Result<bool, String> {
    if !plugin::installed(places) {
        return Ok(false);
    }
    let file = folder.join(plugin::file_name());
    if plugin::installed_as(places, &file, channel.directory()) && file.is_file() {
        return Ok(false);
    }
    let file = channel.prepare_plugin(folder)?;
    let changed = plugin::set_row(places, Some((&file, channel.directory())))?;
    plugin::remove_other_versions(folder, &file);
    Ok(changed)
}

#[tauri::command]
pub async fn native_deepseek_plugin(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    action: String,
) -> Result<Value, String> {
    if window.label() != "main" {
        return Err("The DeepSeek Harness plugin is set up in Settings".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DeepSeekPlugin>();
        let _owner = state
            .owner
            .lock()
            .map_err(|_| "The plugin's owner is unavailable")?;
        let places = Places::current(&super::mcp_clients::home()?);
        match action.as_str() {
            "status" => {}
            "install" => install(&state.channel, &state.folder, &places)?,
            "remove" => remove(&state.channel, &state.folder, &places)?,
            _ => return Err("Unknown plugin action".into()),
        }
        Ok(status(&state.channel, &places))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, fs};

    fn setup() -> (tempfile::TempDir, DeepSeekChannel, PathBuf, Places) {
        let temp = tempfile::tempdir().unwrap();
        let dsh = temp.path().join("dsh");
        let places = Places::new(
            &temp.path().join("home"),
            &HashMap::from([("DSH_HOME".to_owned(), dsh.to_string_lossy().into_owned())]),
        );
        let channel = DeepSeekChannel::new(temp.path().join("data/deepseek-channel"));
        let folder = temp.path().join("data/deepseek-plugin");
        (temp, channel, folder, places)
    }

    #[test]
    fn install_needs_harness_and_status_follows_install_and_remove() {
        let (_temp, channel, folder, places) = setup();
        // Harness's app in /Applications counts too, so only check where it is not.
        if !Client::DeepSeek.present(&places) {
            assert!(install(&channel, &folder, &places)
                .unwrap_err()
                .contains("not installed"));
        }
        fs::create_dir_all(&places.deepseek).unwrap();
        assert_eq!(
            status(&channel, &places),
            json!({"found": true, "installed": false, "sessions": 0})
        );
        install(&channel, &folder, &places).unwrap();
        assert!(folder.join(plugin::file_name()).is_file());
        assert_eq!(status(&channel, &places)["installed"], true);
        assert!(
            !repair(&channel, &folder, &places).unwrap(),
            "already current"
        );
        remove(&channel, &folder, &places).unwrap();
        assert_eq!(status(&channel, &places)["installed"], false);
        assert!(!folder.join(plugin::file_name()).exists());
        assert!(!places.deepseek.join("cordis.patch.yml").exists());
    }

    #[test]
    fn launch_rewrites_only_a_plugin_the_user_installed() {
        let (temp, channel, folder, places) = setup();
        fs::create_dir_all(&places.deepseek).unwrap();
        assert!(!repair(&channel, &folder, &places).unwrap());
        assert!(!folder.exists(), "never installs");
        // Installed by an earlier version of the app, from another folder.
        let old = temp.path().join("old-data/deepseek-plugin");
        let old_channel = DeepSeekChannel::new(temp.path().join("old-data/deepseek-channel"));
        install(&old_channel, &old, &places).unwrap();
        assert!(repair(&channel, &folder, &places).unwrap());
        assert!(plugin::installed_as(
            &places,
            &folder.join(plugin::file_name()),
            channel.directory()
        ));
        // A module deleted by hand is written again.
        fs::remove_file(folder.join(plugin::file_name())).unwrap();
        assert!(!repair(&channel, &folder, &places).unwrap());
        assert!(folder.join(plugin::file_name()).is_file());
    }
}
