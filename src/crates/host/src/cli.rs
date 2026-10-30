//! Installed CLI routes execute before Tauri, accounts, hooks or window creation.
use std::{path::PathBuf, process::Command};
pub(super) fn resource(name: &str) -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let directory = executable.parent()?;
    let extension = std::env::consts::EXE_SUFFIX;
    let file = format!("{name}{extension}");
    [
        directory.join("../Resources").join(&file),
        directory.join("resources").join(&file),
        directory.join(&file),
    ]
    .into_iter()
    .find(|path| path.is_file())
}
pub fn entry(arguments: &[String]) -> Option<i32> {
    if !arguments.iter().skip(1).any(|arg| arg == "--mcp") {
        return None;
    }
    let Some(server) = resource("agent-hud-mcp") else {
        eprintln!("The installed MCP relay is missing. Build the app's packaged resources before using --mcp.");
        return Some(1);
    };
    let mut command = Command::new(server);
    if let Some(engine) = resource("agent-hud-engine") {
        command.env("AGENTHUD_ENGINE", engine);
    }
    Some(serve(command))
}
// Each host runs the MCP server in the relay's place where it can.
#[cfg(unix)]
#[path = "cli/unix.rs"]
mod host;
#[cfg(windows)]
#[path = "cli/windows.rs"]
mod host;
use host::serve;
