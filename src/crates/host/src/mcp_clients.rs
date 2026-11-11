//! HUD tools in each agent client, only after an explicit Settings action:
//! Agent HUD's MCP server, added with the client's own command where it has
//! one and in its settings file where it does not, and the skill that
//! teaches the agent to use it. At launch, the skill of every client that
//! has the server is brought up to date. A marked DeepSeek Harness server and
//! an unambiguously broken file-managed server follow the fixed installed
//! executable, so moving or reinstalling the app cannot leave a stale path.
use crate::client_cli::{executable, run_client};
use agent_hud_runtime::hud_tools::{Client, Places, SkillState, CLIENTS};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::Duration,
};
use tauri::Manager;
#[derive(Default)]
pub struct Clients(Mutex<()>);
fn client(id: &str) -> Result<Client, String> {
    Client::from_id(id).ok_or_else(|| "Unknown MCP client".into())
}
/// Whether the client lists Agent HUD's server.
fn installed(client: Client, places: &Places) -> bool {
    let path = match client {
        Client::ClaudeCode => std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| places.home.clone())
            .join(".claude.json"),
        Client::Codex => std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| places.home.join(".codex"))
            .join("config.toml"),
        _ => return client.listed(places),
    };
    let Ok(meta) = fs::metadata(&path) else {
        return false;
    };
    if meta.len() > 4 * 1024 * 1024 {
        return false;
    }
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    if client == Client::ClaudeCode {
        serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["mcpServers"].get("agent-hud").cloned())
            .is_some()
    } else {
        toml::from_str::<toml::Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("mcp_servers")
                    .and_then(|v| v.get("agent-hud"))
                    .cloned()
            })
            .is_some()
    }
}

/// Refresh a server only when Agent HUD can prove that its ownership or
/// failure is safe to establish. This never adds a server.
fn repair_server_path(client: Client, places: &Places, executable: &Path) -> Result<bool, String> {
    match client {
        Client::DeepSeek if installed(client, places) => {
            client.set_server(places, Some(executable))
        }
        Client::Cursor | Client::Antigravity
            if client.missing_known_executable(places, executable) =>
        {
            client.set_server(places, Some(executable))
        }
        _ => Ok(false),
    }
}
/// Whether the client is on this machine: its command line, or for the
/// clients without one, its folder or app.
fn available(client: Client, places: &Places) -> bool {
    match client.command() {
        Some(_) => executable(client.id(), &places.home).is_some(),
        None => client.present(places),
    }
}
fn args(client: Client, install: bool, path: &std::path::Path) -> Vec<String> {
    let mut args = vec![
        "mcp".into(),
        if install {
            "add".into()
        } else {
            "remove".into()
        },
    ];
    if client == Client::ClaudeCode {
        args.extend(["--scope".into(), "user".into()]);
    }
    args.push("agent-hud".into());
    if install {
        args.extend([
            "--".into(),
            path.to_string_lossy().into_owned(),
            "--mcp".into(),
        ]);
    }
    args
}
fn status(app: &tauri::AppHandle) -> Result<Value, String> {
    let home = agent_hud_runtime::home_directory()?;
    let places = Places::current(&home);
    let options = app.state::<super::HostOptions>();
    let path = options.canonical_executable.as_ref();
    let running = std::env::current_exe().ok();
    let installed_copy = path.is_some_and(|path| running.as_ref() == Some(path));
    let complete = super::cli::resource("agent-hud-mcp").is_some()
        && super::cli::resource("agent-hud-engine").is_some();
    let ready = installed_copy && complete;
    let notice = if !installed_copy {
        let copy = running
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        Some(agent_hud_runtime::l10n::text(
            &format!("只有安装好的 Agent HUD 才能添加 HUD 工具，这一份在 {copy}。"),
            &format!("Only the installed Agent HUD can add HUD tools; this copy is at {copy}."),
        ))
    } else if !complete {
        Some(agent_hud_runtime::l10n::text(
            "这一份 Agent HUD 缺少 MCP 组件。",
            "This copy of Agent HUD is missing its MCP components.",
        ))
    } else {
        None
    };
    let clients = CLIENTS
        .iter()
        .map(|&client| {
            let command = client.command().zip(path).map(|(name, path)| {
                super::system::client_command_line(
                    &std::iter::once(name.to_string())
                        .chain(args(client, true, path))
                        .collect::<Vec<_>>(),
                )
            });
            json!({
                "id": client.id(),
                "name": client.name(),
                "available": available(client, &places),
                "installed": installed(client, &places),
                "command": command,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "ready": ready,
        "notice": notice,
        "json": path.map(|path| json!({"mcpServers":{"agent-hud":{"command":path,"args":["--mcp"]}}})),
        "clients": clients,
    }))
}
fn run(app: &tauri::AppHandle, id: &str, install: bool) -> Result<Value, String> {
    let home = agent_hud_runtime::home_directory()?;
    let places = Places::current(&home);
    let client = client(id)?;
    let options = app.state::<super::HostOptions>();
    let path = options
        .canonical_executable
        .as_ref()
        .ok_or("No fixed application executable is delegated")?;
    if std::env::current_exe().map_err(|e| e.to_string())? != *path {
        return Err("MCP setup is only available in the fixed installed app".into());
    }
    if install && super::cli::resource("agent-hud-mcp").is_none() {
        return Err("The packaged MCP relay is missing".into());
    }
    let server = match client.command() {
        Some(_) => executable(client.id(), &home)
            .ok_or_else(|| "The client's command line is not installed".to_string())
            .and_then(|executable| {
                run_client(
                    &executable,
                    &args(client, install, path),
                    &home,
                    Duration::from_secs(15),
                )
            }),
        None if install && !client.present(&places) => {
            Err(format!("{} is not installed", client.name()))
        }
        None => client
            .set_server(&places, install.then_some(path.as_path()))
            .map(drop),
    };
    // The skill goes with the server: added once the server is in, and
    // removed even when the server could not be, so no skill is left
    // pointing at tools that are gone.
    if install {
        server?;
        client.add_skill(&places).map_err(|error| {
            format!("HUD tools were added, but their skill could not be written: {error}")
        })?;
    } else {
        let skill = client.remove_skill(&places);
        server?;
        skill?;
    }
    status(app)
}
/// At launch: brings the skill up to date in every client that lists Agent
/// HUD's server, adding it where an older version added only the server. A
/// safely identifiable existing server also follows the fixed installed
/// executable after a move or reinstall. Neither action adds a server.
pub(crate) fn repair_on_launch(app: &tauri::AppHandle) {
    // The demo leaves the user's clients alone.
    if app.state::<super::AppState>().demo {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let owner = app.state::<Clients>();
        let _guard = owner.0.lock().unwrap_or_else(PoisonError::into_inner);
        let Ok(home) = agent_hud_runtime::home_directory() else {
            return;
        };
        let canonical = app
            .state::<super::HostOptions>()
            .canonical_executable
            .clone();
        let places = Places::current(&home);
        for client in CLIENTS {
            if !installed(client, &places) {
                continue;
            }
            if let Some(path) = canonical.as_deref() {
                if let Err(error) = repair_server_path(client, &places, path) {
                    eprintln!(
                        "The Agent HUD MCP path for {} could not be reconciled: {error}",
                        client.name()
                    );
                }
            }
            match client.skill_state(&places) {
                SkillState::Current | SkillState::Foreign => {}
                SkillState::Missing | SkillState::Older => {
                    if let Err(error) = client.add_skill(&places) {
                        eprintln!(
                            "The HUD tools skill for {} could not be written: {error}",
                            client.name()
                        );
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, fs};

    fn places(root: &Path) -> Places {
        Places::new(
            &root.join("home"),
            &HashMap::from([(
                "DSH_HOME".to_owned(),
                root.join("dsh").to_string_lossy().into_owned(),
            )]),
        )
    }

    #[test]
    fn repair_replaces_only_an_existing_deepseek_server_path() {
        let temp = tempfile::tempdir().unwrap();
        let places = places(temp.path());
        fs::create_dir_all(&places.deepseek).unwrap();
        let old = Path::new("C:/Users/me/AppData/Local/Agent HUD/Agent HUD.exe");
        let current = Path::new("E:/use/Agent HUD/Agent HUD.exe");
        Client::DeepSeek.set_server(&places, Some(old)).unwrap();

        assert!(repair_server_path(Client::DeepSeek, &places, current).unwrap());
        let text = fs::read_to_string(places.deepseek.join("cordis.patch.yml")).unwrap();
        assert!(text.contains("E:/use/Agent HUD/Agent HUD.exe"));
        assert!(!text.contains("C:/Users/me/AppData/Local/Agent HUD/Agent HUD.exe"));
        assert!(!repair_server_path(Client::DeepSeek, &places, current).unwrap());
    }

    #[test]
    fn repair_never_adds_a_deepseek_server() {
        let temp = tempfile::tempdir().unwrap();
        let places = places(temp.path());
        fs::create_dir_all(&places.deepseek).unwrap();

        assert!(!repair_server_path(
            Client::DeepSeek,
            &places,
            Path::new("E:/use/Agent HUD/Agent HUD.exe"),
        )
        .unwrap());
        assert!(!places.deepseek.join("cordis.patch.yml").exists());
    }

    #[test]
    fn repair_updates_only_a_missing_standard_cursor_path() {
        let temp = tempfile::tempdir().unwrap();
        let places = places(temp.path());
        let old = temp.path().join("old/Agent HUD.exe");
        let current = temp.path().join("current/Agent HUD.exe");
        Client::Cursor.set_server(&places, Some(&old)).unwrap();

        assert!(repair_server_path(Client::Cursor, &places, &current).unwrap());
        let text = fs::read_to_string(places.home.join(".cursor/mcp.json")).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            value["mcpServers"]["agent-hud"]["command"],
            current.to_string_lossy().as_ref()
        );

        fs::create_dir_all(old.parent().unwrap()).unwrap();
        fs::write(&old, "custom copy").unwrap();
        Client::Cursor.set_server(&places, Some(&old)).unwrap();
        assert!(!repair_server_path(Client::Cursor, &places, &current).unwrap());
    }

    #[test]
    fn repair_updates_a_missing_standard_antigravity_path() {
        let temp = tempfile::tempdir().unwrap();
        let places = places(temp.path());
        let old = temp.path().join("old/Agent HUD.exe");
        let current = temp.path().join("current/Agent HUD.exe");
        Client::Antigravity.set_server(&places, Some(&old)).unwrap();

        assert!(repair_server_path(Client::Antigravity, &places, &current).unwrap());
        let text = fs::read_to_string(places.home.join(".gemini/config/mcp_config.json")).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            value["mcpServers"]["agent-hud"]["command"],
            current.to_string_lossy().as_ref()
        );
    }
}

#[tauri::command]
pub async fn native_mcp_clients(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    action: String,
    client: Option<String>,
) -> Result<Value, String> {
    if window.label() != "main" {
        return Err("MCP setup is owned by Settings".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let owner = app.state::<Clients>();
        let _guard = owner.0.lock().unwrap_or_else(PoisonError::into_inner);
        match action.as_str() {
            "status" => status(&app),
            "install" | "remove" => run(
                &app,
                client.as_deref().ok_or("Client is required")?,
                action == "install",
            ),
            _ => Err("Unknown MCP setup action".into()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
