//! Settings → Agents → Agent plugins: Agent HUD's Claude Code plugin, which
//! lets the HUD answer Claude's questions in the session that asked. It is
//! installed and removed only when the user asks there, with Claude Code's own
//! `plugin install`; at launch, a plugin the user installed from another
//! folder, such as the Swift app's, is installed again from this app's. The
//! channel it shares with the app and the HUD's answers live in
//! `agent_hud_runtime::claude_plugin`.
use agent_hud_runtime::claude_plugin::{self as plugin, ClaudeChannel, ClaudeReplies};
use agent_hud_runtime::replies::SessionReplies;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tauri::Manager;

/// Claude Code fetches nothing to install a local plugin, but may take a while.
const COMMAND_TIME: Duration = Duration::from_secs(60);

pub struct ClaudePlugin {
    channel: ClaudeChannel,
    folder: PathBuf,
    owner: Mutex<()>,
}

impl ClaudePlugin {
    /// The plugin's channel and files: where a product build keeps them, else
    /// in the app's data folder.
    pub(crate) fn new(paths: Option<(PathBuf, PathBuf)>) -> Result<Self, String> {
        let (channel, folder) = match paths {
            Some(paths) => paths,
            None => {
                let data = agent_hud_runtime::local_tools_directory()?;
                (data.join("claude-channel"), data.join("claude-plugin"))
            }
        };
        Ok(Self {
            channel: ClaudeChannel::new(channel),
            folder,
            owner: Mutex::new(()),
        })
    }

    /// The HUD's way to answer Claude's questions in a session running the plugin.
    pub(crate) fn replies(&self) -> Arc<dyn SessionReplies> {
        let claude_config = super::mcp_clients::home()
            .map(|home| plugin::claude_config_directory(&home))
            .unwrap_or_default();
        Arc::new(ClaudeReplies::new(self.channel.clone(), claude_config))
    }
}

/// Whether Claude Code is on this Mac, whether it lists the plugin (and
/// installed it from another folder than `folder`), and how many sessions run
/// it now.
fn status(
    channel: &ClaudeChannel,
    folder: &Path,
    executable: Option<&Path>,
    claude_config: &Path,
) -> Value {
    let installed = plugin::installed(claude_config);
    json!({
        "found": executable.is_some(),
        "installed": installed,
        "elsewhere": installed && !plugin::installed_from(claude_config, folder),
        "sessions": channel.loaded(SystemTime::now()).len(),
    })
}

/// Writes the plugin's files and has Claude Code install them from `folder`,
/// unless it did already. A copy from another folder talks to another
/// channel, so it is taken out first.
fn install(
    channel: &ClaudeChannel,
    folder: &Path,
    executable: Option<&Path>,
    claude_config: &Path,
    home: &Path,
) -> Result<(), String> {
    let executable = executable.ok_or("Claude Code's command line is not installed")?;
    channel.prepare_plugin(folder)?;
    if plugin::installed_from(claude_config, folder) {
        return Ok(());
    }
    if plugin::installed(claude_config) || plugin::marketplace_folder(claude_config).is_some() {
        take_out(executable, claude_config, home)?;
    }
    super::mcp_clients::run_client(
        executable,
        &plugin::install_arguments(folder),
        home,
        COMMAND_TIME,
    )
    .map_err(install_error)?;
    if !plugin::installed_from(claude_config, folder) {
        return Err("Claude Code still runs the plugin from another folder".into());
    }
    Ok(())
}

/// A Claude Code too old for `plugin install --marketplace` only says the
/// option is unknown; say what fixes it, keeping its words.
fn install_error(error: String) -> String {
    if error.contains("unknown option") && error.contains("--marketplace") {
        let message = agent_hud_runtime::l10n::text(
            "Claude Code 版本太旧，无法识别 --marketplace。请先在终端运行 claude update 升级，再点安装。",
            concat!(
                "This Claude Code is too old to recognize --marketplace. ",
                "Run claude update in a terminal to upgrade, then click Install again."
            ),
        );
        format!("{message} ({error})")
    } else {
        error
    }
}

/// At launch: when the user installed the plugin from another folder, such
/// as the Swift app's, has Claude Code install it from this app's instead.
/// Runs in the background under the Settings actions' owner; never installs
/// a plugin the user has not, and only logs a failure.
pub(crate) fn repair_on_launch(app: &tauri::AppHandle) {
    // The demo leaves the user's Claude Code alone.
    if app.state::<super::AppState>().demo {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<ClaudePlugin>();
        let Ok(_owner) = state.owner.lock() else {
            return;
        };
        let result = super::mcp_clients::home().and_then(|home| {
            let executable = super::mcp_clients::executable("claudeCode", &home);
            let config = plugin::claude_config_directory(&home);
            repair(
                &state.channel,
                &state.folder,
                executable.as_deref(),
                &config,
                &home,
            )
        });
        if let Err(error) = result {
            eprintln!("The Claude Code plugin could not be installed from this app: {error}");
        }
    });
}

/// Installs the plugin again from `folder` when Claude Code lists it from
/// another folder and its command line is here. Returns whether it did.
/// When Claude Code runs it from `folder` already, rewrites what this
/// version of the app changed in its files, which sessions read when they
/// start.
fn repair(
    channel: &ClaudeChannel,
    folder: &Path,
    executable: Option<&Path>,
    claude_config: &Path,
    home: &Path,
) -> Result<bool, String> {
    if plugin::installed_from(claude_config, folder) {
        channel.prepare_plugin(folder)?;
        return Ok(false);
    }
    if executable.is_none() || !plugin::installed(claude_config) {
        return Ok(false);
    }
    install(channel, folder, executable, claude_config, home).map(|()| true)
}

/// Forgets every check-in and word left, then has Claude Code remove the
/// plugin and its marketplace.
fn remove(
    channel: &ClaudeChannel,
    executable: Option<&Path>,
    claude_config: &Path,
    home: &Path,
) -> Result<(), String> {
    channel.clear()?;
    if !plugin::installed(claude_config) {
        return Ok(());
    }
    let executable = executable.ok_or("Claude Code's command line is not installed")?;
    take_out(executable, claude_config, home)
}

/// Has Claude Code remove the plugin, when it lists it, then its marketplace.
fn take_out(executable: &Path, claude_config: &Path, home: &Path) -> Result<(), String> {
    let [uninstall, marketplace] = plugin::remove_arguments();
    if plugin::installed(claude_config) {
        super::mcp_clients::run_client(executable, &uninstall, home, COMMAND_TIME)?;
    }
    let _ = super::mcp_clients::run_client(executable, &marketplace, home, COMMAND_TIME);
    Ok(())
}

#[tauri::command]
pub async fn native_claude_plugin(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    action: String,
) -> Result<Value, String> {
    if window.label() != "main" {
        return Err("The Claude Code plugin is set up in Settings".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<ClaudePlugin>();
        let _owner = state
            .owner
            .lock()
            .map_err(|_| "The plugin's owner is unavailable")?;
        let home = super::mcp_clients::home()?;
        let executable = super::mcp_clients::executable("claudeCode", &home);
        let config = plugin::claude_config_directory(&home);
        match action.as_str() {
            "status" => {}
            "install" => install(
                &state.channel,
                &state.folder,
                executable.as_deref(),
                &config,
                &home,
            )?,
            "remove" => remove(&state.channel, executable.as_deref(), &config, &home)?,
            _ => return Err("Unknown plugin action".into()),
        }
        Ok(status(
            &state.channel,
            &state.folder,
            executable.as_deref(),
            &config,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod error_tests {
    use super::install_error;

    #[test]
    fn unknown_marketplace_option_explains_how_to_upgrade() {
        let error = "error: unknown option '--marketplace'";
        let message = install_error(error.into());
        assert!(message.contains("claude update"));
        assert!(message.ends_with(&format!("({error})")));
    }

    #[test]
    fn other_install_errors_are_unchanged() {
        for error in [
            "error: unknown option '--scope'",
            "error: --marketplace folder does not exist",
            "error: permission denied",
        ] {
            assert_eq!(install_error(error.into()), error);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;

    /// Stands in for Claude Code: `plugin install`, `plugin uninstall` and
    /// `plugin marketplace remove` change its records of installed plugins
    /// and of marketplaces as the real one does, and every call is logged.
    fn fake_claude(root: &Path, config: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let record = config.join("plugins/installed_plugins.json");
        fs::create_dir_all(record.parent().unwrap()).unwrap();
        let claude = root.join("claude");
        fs::write(
            &claude,
            format!(
                "#!/bin/sh\necho \"$@\" >> '{log}'\ncase \"$1 $2 $3\" in\n\"plugin install agent-hud\") printf '%s' '{on}' > '{record}'; printf '{market}' \"$5\" \"$5\" > '{markets}' ;;\n\"plugin uninstall agent-hud@agent-hud\") printf '%s' '{off}' > '{record}' ;;\n\"plugin marketplace remove\") printf '%s' '{none}' > '{markets}' ;;\nesac\n",
                log = root.join("calls").display(),
                record = record.display(),
                markets = config.join("plugins/known_marketplaces.json").display(),
                on = r#"{"plugins":{"agent-hud@agent-hud":[]}}"#,
                off = r#"{"plugins":{}}"#,
                market = r#"{"agent-hud":{"source":{"source":"directory","path":"%s"},"installLocation":"%s"}}"#,
                none = "{}",
            ),
        )
        .unwrap();
        fs::set_permissions(&claude, fs::Permissions::from_mode(0o755)).unwrap();
        claude
    }

    /// Claude Code's records once it installed the plugin from `folder`.
    fn listed_from(config: &Path, folder: &Path) {
        fs::create_dir_all(config.join("plugins")).unwrap();
        fs::write(
            config.join("plugins/installed_plugins.json"),
            br#"{"plugins":{"agent-hud@agent-hud":[]}}"#,
        )
        .unwrap();
        fs::write(
            config.join("plugins/known_marketplaces.json"),
            json!({"agent-hud":{"source":{"source":"directory","path":folder},"installLocation":folder}})
                .to_string(),
        )
        .unwrap();
    }

    /// What Claude Code is asked to replace a copy from another folder with ours.
    fn replaced(folder: &Path) -> [String; 3] {
        [
            "plugin uninstall agent-hud@agent-hud".into(),
            "plugin marketplace remove agent-hud".into(),
            format!(
                "plugin install agent-hud --marketplace {} --scope user",
                folder.display()
            ),
        ]
    }

    #[test]
    fn install_and_remove_go_through_claude_code_and_status_follows() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let config = root.join("claude-config");
        let channel = ClaudeChannel::new(root.join("channel"));
        let folder = root.join("plugin");
        let claude = fake_claude(root, &config);

        assert_eq!(
            status(&channel, &folder, None, &config),
            json!({"found":false,"installed":false,"elsewhere":false,"sessions":0})
        );
        assert!(
            install(&channel, &folder, None, &config, root).is_err(),
            "no Claude Code, nothing to install into"
        );

        install(&channel, &folder, Some(&claude), &config, root).unwrap();
        assert!(folder.join("hooks/register.ts").is_file());
        // Installed already: the files are refreshed, Claude Code is not asked again.
        install(&channel, &folder, Some(&claude), &config, root).unwrap();
        let calls = fs::read_to_string(root.join("calls")).unwrap();
        assert_eq!(
            calls
                .lines()
                .filter(|line| line.starts_with("plugin install agent-hud --marketplace"))
                .count(),
            1
        );

        let session = "44444444-4444-4444-8444-444444444444";
        fs::write(
            channel.directory().join(format!("sessions/{session}.json")),
            br#"{"at":1}"#,
        )
        .unwrap();
        assert_eq!(
            status(&channel, &folder, Some(&claude), &config),
            json!({"found":true,"installed":true,"elsewhere":false,"sessions":1})
        );

        remove(&channel, Some(&claude), &config, root).unwrap();
        assert_eq!(
            status(&channel, &folder, Some(&claude), &config),
            json!({"found":true,"installed":false,"elsewhere":false,"sessions":0})
        );
        assert!(!channel.directory().join("sessions").exists());
        let calls = fs::read_to_string(root.join("calls")).unwrap();
        assert!(calls.contains("plugin uninstall agent-hud@agent-hud"));
        assert!(calls.contains("plugin marketplace remove agent-hud"));
        // Removing what is not there asks nothing of Claude Code.
        remove(&channel, None, &config, root).unwrap();
    }

    #[test]
    fn installing_replaces_a_copy_from_another_folder() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let config = root.join("claude-config");
        let channel = ClaudeChannel::new(root.join("channel"));
        let folder = root.join("plugin");
        let claude = fake_claude(root, &config);
        // The Swift app's copy, which talks to its own channel.
        listed_from(&config, &root.join("swift-plugin"));
        assert_eq!(
            status(&channel, &folder, Some(&claude), &config),
            json!({"found":true,"installed":true,"elsewhere":true,"sessions":0})
        );

        install(&channel, &folder, Some(&claude), &config, root).unwrap();
        let calls = fs::read_to_string(root.join("calls")).unwrap();
        assert_eq!(calls.lines().collect::<Vec<_>>(), replaced(&folder));
        assert_eq!(plugin::marketplace_folder(&config), Some(folder.clone()));
        assert!(folder.join("hooks/register.ts").is_file());
        assert_eq!(
            status(&channel, &folder, Some(&claude), &config)["elsewhere"],
            false
        );
    }

    #[test]
    fn installing_over_our_own_copy_asks_nothing_of_claude_code() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let config = root.join("claude-config");
        let channel = ClaudeChannel::new(root.join("channel"));
        let folder = root.join("plugin");
        let claude = fake_claude(root, &config);
        listed_from(&config, &folder);

        install(&channel, &folder, Some(&claude), &config, root).unwrap();
        assert!(
            folder.join("hooks/register.ts").is_file(),
            "files refreshed"
        );
        assert!(!repair(&channel, &folder, Some(&claude), &config, root).unwrap());
        assert!(!root.join("calls").exists());
    }

    #[test]
    fn the_launch_repair_brings_an_older_copy_of_our_plugin_up_to_date() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let config = root.join("claude-config");
        let channel = ClaudeChannel::new(root.join("channel"));
        let folder = root.join("plugin");
        let claude = fake_claude(root, &config);
        listed_from(&config, &folder);
        install(&channel, &folder, Some(&claude), &config, root).unwrap();
        let script = folder.join("hooks/register.ts");
        let current = fs::read(&script).unwrap();
        fs::write(&script, "// another version").unwrap();

        assert!(!repair(&channel, &folder, None, &config, root).unwrap());
        assert_eq!(
            fs::read(&script).unwrap(),
            current,
            "the files are this app's again"
        );
        assert!(
            !root.join("calls").exists(),
            "Claude Code was asked nothing"
        );
    }

    #[test]
    fn the_launch_repair_installs_nothing_the_user_did_not() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let config = root.join("claude-config");
        let channel = ClaudeChannel::new(root.join("channel"));
        let folder = root.join("plugin");
        let claude = fake_claude(root, &config);

        assert!(!repair(&channel, &folder, Some(&claude), &config, root).unwrap());
        // Nor when only a marketplace from another folder is left.
        let swift = root.join("swift-plugin");
        fs::write(
            config.join("plugins/known_marketplaces.json"),
            json!({"agent-hud":{"source":{"source":"directory","path":swift}}}).to_string(),
        )
        .unwrap();
        assert!(!repair(&channel, &folder, Some(&claude), &config, root).unwrap());
        assert!(!root.join("calls").exists());
        assert!(!folder.exists());
        assert!(!plugin::installed(&config));
    }

    #[test]
    fn the_launch_repair_installs_a_copy_from_another_folder_again_from_ours() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let config = root.join("claude-config");
        let channel = ClaudeChannel::new(root.join("channel"));
        let folder = root.join("plugin");
        let claude = fake_claude(root, &config);
        listed_from(&config, &root.join("swift-plugin"));

        assert!(
            !repair(&channel, &folder, None, &config, root).unwrap(),
            "no Claude Code, nothing to run"
        );
        assert!(repair(&channel, &folder, Some(&claude), &config, root).unwrap());
        let calls = fs::read_to_string(root.join("calls")).unwrap();
        assert_eq!(calls.lines().collect::<Vec<_>>(), replaced(&folder));
        assert!(plugin::installed_from(&config, &folder));
        assert!(folder.join("hooks/register.ts").is_file());

        assert!(
            !repair(&channel, &folder, Some(&claude), &config, root).unwrap(),
            "once is enough"
        );
        let calls = fs::read_to_string(root.join("calls")).unwrap();
        assert_eq!(calls.lines().count(), 3);
    }
}
