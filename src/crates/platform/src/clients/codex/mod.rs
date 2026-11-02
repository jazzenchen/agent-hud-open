//! Codex's native channel: the desktop app's IPC (`agents::codex::DESKTOP`).
// The same observer runs over a Unix socket or, on Windows, a named pipe.
pub mod desktop;
pub mod desktop_io;

use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Codex's data folder: `CODEX_HOME`, or `~/.codex`.
pub fn data_directory() -> Result<PathBuf, String> {
    let home = crate::os::home_dir()?;
    Ok(
        crate::os::variable(&std::env::vars().collect(), "CODEX_HOME", &home)
            .unwrap_or_else(|| home.join(".codex")),
    )
}

/// A Codex TUI served by the Codex daemon: the terminal it runs in, when the
/// host can read that process's environment.
pub(crate) fn daemon_terminal(
    session: &str,
    home: &Path,
    env: &HashMap<String, String>,
) -> Option<Value> {
    if !crate::os::READS_PROCESS_ENVIRONMENT || !crate::hooks::origins::is_uuid(session) {
        return None;
    }
    let origin = desktop::daemon_tui_origin(session, home, env)?;
    let port = crate::hooks::origins::loopback_port(&origin)?;
    let (args, env) =
        crate::os::codex_cli_on_port(port, &["TERM_PROGRAM", "ITERM_SESSION_ID", "TMUX"])?;
    if agent_hud_agents::codex::process::invocation(&args) != "cli" {
        return None;
    }
    crate::hooks::origins::iterm_session(&env)
}
