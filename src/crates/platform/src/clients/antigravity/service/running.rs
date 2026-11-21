//! The running Antigravity language servers and CLI, found by their process.
//! A server's token is on its command line, or, since Antigravity 2.34 keeps
//! it off there, in the file the server writes about itself when it starts.
use agent_hud_agents::{antigravity, split};
use std::{collections::HashMap, path::Path};
pub(super) struct Server {
    pub pid: u32,
    pub token: String,
    pub extension: Option<(u16, String)>,
    /// The client it serves: the desktop app, the IDE or the CLI, by client id.
    pub client: &'static str,
    priority: u8,
}
fn flag(command: &str, name: &str) -> Option<String> {
    let target = format!("--{name}");
    let mut offset = 0;
    while let Some(found) = command[offset..].find(&target) {
        let at = offset + found;
        let after = at + target.len();
        if (at == 0 || command.as_bytes()[at - 1].is_ascii_whitespace())
            && command
                .as_bytes()
                .get(after)
                .is_some_and(|byte| *byte == b'=' || byte.is_ascii_whitespace())
        {
            let value = command[after..].trim_start_matches('=').trim_start();
            let value = if let Some(quote @ ('\'' | '"')) = value.chars().next() {
                value[1..].split(quote).next()?
            } else {
                value.split_whitespace().next()?
            };
            if !value.is_empty() && !value.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n')) {
                return Some(value.into());
            }
        }
        offset = after;
    }
    None
}

pub(super) fn servers() -> Result<Vec<Server>, String> {
    let processes = crate::os::processes()?;
    let saved = crate::os::home_dir()
        .map(|home| saved_tokens(&home.join(".gemini")))
        .unwrap_or_default();
    Ok(parse_servers(processes, &saved))
}

/// The data folders a server may write its own file under: the desktop
/// app's, the IDE's and the CLI's.
const DATA_DIRS: [&str; 3] = ["antigravity", "antigravity-ide", "antigravity-cli"];
/// Such a file holds a few short fields; anything larger is not one.
const MAX_SAVED_BYTES: u64 = 4096;

/// The token each server saved with its process id, from the files under
/// `<gemini>/<data folder>/daemon` (`{"pid", "httpsPort", "csrfToken", …}`).
/// A file left by a server that is gone names a process that is no longer a
/// server here, or one whose service refuses the token; both are passed over.
fn saved_tokens(gemini: &Path) -> HashMap<u32, String> {
    let mut tokens = HashMap::new();
    for dir in DATA_DIRS {
        let Ok(entries) = std::fs::read_dir(gemini.join(dir).join("daemon")) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json")
                || !entry
                    .metadata()
                    .is_ok_and(|meta| meta.is_file() && meta.len() <= MAX_SAVED_BYTES)
            {
                continue;
            }
            let Some(saved) = std::fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            else {
                continue;
            };
            let pid = saved["pid"]
                .as_u64()
                .and_then(|pid| u32::try_from(pid).ok());
            let token = saved["csrfToken"].as_str().filter(|token| {
                !token.is_empty() && !token.bytes().any(|byte| byte.is_ascii_control())
            });
            if let (Some(pid), Some(token)) = (pid, token) {
                tokens.insert(pid, token.to_owned());
            }
        }
    }
    tokens
}

fn parse_servers(
    processes: Vec<crate::os::ProcessInfo>,
    saved: &HashMap<u32, String>,
) -> Vec<Server> {
    let mut result = Vec::new();
    for process in processes {
        let pid = process.pid;
        let command = process.command.as_str();
        let executable = process.executable.to_lowercase().replace('\\', "/");
        let cli = matches!(executable.rsplit('/').next(), Some("agy" | "agy.exe"));
        let data_dir = flag(command, "app_data_dir");
        let server = (executable.contains("language_server")
            || executable.contains("language-server"))
            && (executable.contains("antigravity.app/")
                || executable.contains("antigravity ide.app/")
                || executable.contains("/antigravity/")
                || executable.contains("/antigravity-cli/")
                || executable.contains("/antigravity_cli/")
                || data_dir
                    .as_deref()
                    .is_some_and(|path| path.starts_with("antigravity")));
        if !server && !cli {
            continue;
        }
        let token = flag(command, "csrf_token")
            .or_else(|| saved.get(&pid).cloned())
            .unwrap_or_default();
        let extension = flag(command, "extension_server_port")
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|port| *port != 0)
            .and_then(|port| {
                let token =
                    flag(command, "extension_server_csrf_token").unwrap_or_else(|| token.clone());
                (!token.is_empty()).then_some((port, token))
            });
        if token.is_empty() && extension.is_none() {
            continue;
        }
        // The client: its data folder names it, else where it is installed
        // (the CLI's folder, the IDE's app); the desktop app otherwise.
        let client = match data_dir.as_deref() {
            Some("antigravity-cli") => split::ANTIGRAVITY_CLI,
            Some("antigravity-ide") => split::ANTIGRAVITY_IDE,
            Some("antigravity") => antigravity::AGENT.id,
            _ if cli
                || executable.contains("/antigravity-cli/")
                || executable.contains("/antigravity_cli/") =>
            {
                split::ANTIGRAVITY_CLI
            }
            _ if executable.contains("antigravity ide.app/") => split::ANTIGRAVITY_IDE,
            _ => antigravity::AGENT.id,
        };
        result.push(Server {
            pid,
            token,
            extension,
            client,
            priority: if cli { 1 } else { 0 },
        });
    }
    result.sort_by_key(|server| (server.priority, server.pid));
    result
}

/// The CLI's own executable, rather than another `agy` found on PATH.
/// A token-less CLI still supports its read-only `/usage` command; when no
/// CLI process is running, fall back to the installed or app-bundled CLI.
pub(super) fn cli_executable() -> Result<Option<String>, String> {
    if let Some(running) = cli_from(crate::os::processes()?) {
        return Ok(Some(running));
    }
    let Ok(home) = crate::os::home_dir() else {
        return Ok(None);
    };
    let env = std::env::vars().collect::<crate::os::Environment>();
    Ok(
        crate::os::find_engine(&home, &env, "agy")
            .and_then(|path| path.to_str().map(str::to_owned)),
    )
}

fn cli_from(processes: Vec<crate::os::ProcessInfo>) -> Option<String> {
    processes.into_iter().find_map(|process| {
        let path = std::path::Path::new(&process.executable);
        (path.is_absolute()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.eq_ignore_ascii_case("agy") || name.eq_ignore_ascii_case("agy.exe")
                }))
        .then_some(process.executable)
    })
}

#[cfg(test)]
mod tests;
