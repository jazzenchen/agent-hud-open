//! HUD tools in each agent client, only after an explicit Settings action:
//! Agent HUD's MCP server, added with the client's own command where it has
//! one and in its settings file where it does not, and the skill that
//! teaches the agent to use it. At launch, every client that has the server
//! is pointed at this copy again and its skill brought up to date.
use crate::client_cli::{executable, run_client};
use agent_hud_runtime::hud_tools::{Client, Places, SkillState, CLIENTS};
use agent_hud_runtime::l10n::tr;
use serde_json::{json, Value};
use std::{
    sync::{Mutex, PoisonError},
    time::Duration,
};
use tauri::Manager;
#[path = "mcp_clients/entry.rs"]
mod entry;
#[derive(Default)]
pub struct Clients(Mutex<()>);
fn client(id: &str) -> Result<Client, String> {
    Client::from_id(id).ok_or_else(|| "Unknown MCP client".into())
}
/// Whether the client lists Agent HUD's server.
fn installed(client: Client, places: &Places) -> bool {
    match client {
        Client::ClaudeCode | Client::Codex => entry::listed(client, places).is_some(),
        _ => client.listed(places),
    }
}
/// Adds the server again with the client's own command line, so it starts
/// `path`: removed first, so the add never meets an entry of that name.
fn repoint(client: Client, places: &Places, path: &std::path::Path) -> Result<(), String> {
    let executable = executable(client.id(), &places.home)
        .ok_or("the client's command line is not installed")?;
    for install in [false, true] {
        run_client(
            &executable,
            &args(client, install, path),
            &places.home,
            Duration::from_secs(15),
        )?;
    }
    Ok(())
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
        Some(tr!(
            "Only the installed Agent HUD can add HUD tools; this copy is at {{copy}}.",
            copy = copy
        ))
    } else if !complete {
        Some(tr!("This copy of Agent HUD is missing its MCP components."))
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
/// At launch, in every client that lists Agent HUD's server: points the
/// server at this copy when it names another (a copy that moved, Open #100),
/// and brings the skill up to date, adding it where an older version added
/// only the server. Never adds a server, and only logs a failure.
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
                if client.command().is_none() {
                    let _ = client.set_server(&places, Some(path));
                } else if entry::listed(client, &places)
                    .is_some_and(|entry| entry.points_elsewhere(path))
                {
                    if let Err(error) = repoint(client, &places, path) {
                        eprintln!(
                            "Agent HUD's MCP server in {} still starts another copy: {error}",
                            client.name()
                        );
                    }
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
