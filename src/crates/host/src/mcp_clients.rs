//! HUD tools in each agent client, only after an explicit Settings action:
//! Agent HUD's MCP server, added with the client's own command where it has
//! one and in its settings file where it does not, and the skill that
//! teaches the agent to use it. At launch, the skill of every client that
//! has the server is brought up to date.
use agent_hud_runtime::hud_tools::{Client, Places, SkillState, CLIENTS};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tauri::Manager;
#[derive(Default)]
pub struct Clients(Mutex<()>);
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub(crate) fn home() -> Result<PathBuf, String> {
    agent_hud_runtime::home_directory()
}
fn client(id: &str) -> Result<Client, String> {
    Client::from_id(id).ok_or_else(|| "Unknown MCP client".into())
}
fn paths(home: &std::path::Path) -> Vec<PathBuf> {
    let mut dirs = vec![
        home.join(".bun/bin"),
        home.join(".local/bin"),
        home.join(".npm-global/bin"),
        "/opt/homebrew/bin".into(),
        "/usr/local/bin".into(),
        "/usr/bin".into(),
        "/bin".into(),
    ];
    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("npm"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs
}
pub(crate) fn executable(client: &str, home: &std::path::Path) -> Option<PathBuf> {
    let command = Client::from_id(client)?.command()?;
    super::system::cli_candidates(home, command, paths(home))
        .into_iter()
        .find(|path| path.is_file())
        // A desktop app's bundled copy, as the account readers find it.
        .or_else(|| agent_hud_runtime::client_executable(command))
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
        Client::Cursor | Client::DeepSeek | Client::Antigravity => return client.listed(places),
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
    let home = home()?;
    let places = Places::current(&home);
    let options = app.state::<super::HostOptions>();
    let path = options.canonical_executable.as_ref();
    let ready = path.is_some_and(|path| std::env::current_exe().ok().as_ref() == Some(path))
        && super::cli::resource("agent-hud-mcp").is_some()
        && super::cli::resource("agent-hud-engine").is_some();
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
        "notice": if ready { None } else { Some("MCP setup requires the fixed installed app and its packaged relay") },
        "json": path.map(|path| json!({"mcpServers":{"agent-hud":{"command":path,"args":["--mcp"]}}})),
        "clients": clients,
    }))
}
fn run(app: &tauri::AppHandle, id: &str, install: bool) -> Result<Value, String> {
    let home = home()?;
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
/// HUD's server, adding it where an older version added only the server.
/// Never adds a server, and only logs a failure.
pub(crate) fn repair_on_launch(app: &tauri::AppHandle) {
    // The demo leaves the user's clients alone.
    if app.state::<super::AppState>().demo {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let owner = app.state::<Clients>();
        let Ok(_guard) = owner.0.lock() else {
            return;
        };
        let Ok(home) = home() else {
            return;
        };
        let places = Places::current(&home);
        for client in CLIENTS {
            if !installed(client, &places) {
                continue;
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
/// Runs a client's command line in `home` and waits up to `timeout`; Err with
/// the first line it printed when it fails.
pub(crate) fn run_client(
    executable: &std::path::Path,
    arguments: &[String],
    home: &std::path::Path,
    timeout: Duration,
) -> Result<(), String> {
    // A temporary file avoids waiting on descendant processes that keep an output pipe open.
    let output_path = std::env::temp_dir().join(format!(
        "agenthud-client-{}-{}.log",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    super::system::private_output(&mut options);
    let output = options.open(&output_path).map_err(|e| e.to_string())?;
    let result = (|| {
        let mut command = Command::new(executable);
        super::system::configure_client_command(&mut command);
        command
            .args(arguments)
            .current_dir(home)
            .stdin(Stdio::null())
            .stdout(output.try_clone().map_err(|e| e.to_string())?)
            .stderr(output);
        if let Ok(path) = std::env::join_paths(paths(home)) {
            command.env("PATH", path);
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let deadline = Instant::now() + timeout;
        let exit_status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            if Instant::now() >= deadline {
                super::system::stop_client(&mut child);
                return Err(format!(
                    "The client did not finish within {} seconds",
                    timeout.as_secs()
                ));
            }
            std::thread::sleep(Duration::from_millis(30));
        };
        if !exit_status.success() {
            use std::io::Read;
            let mut bytes = vec![];
            fs::File::open(&output_path)
                .map_err(|e| e.to_string())?
                .take(65536)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            let line = String::from_utf8_lossy(&bytes)
                .lines()
                .find(|s| !s.trim().is_empty())
                .unwrap_or("The client's command failed")
                .chars()
                .take(160)
                .collect::<String>();
            return Err(line);
        }
        Ok(())
    })();
    let _ = fs::remove_file(output_path);
    result
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
        let _guard = owner.0.lock().map_err(|_| "MCP client owner unavailable")?;
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
