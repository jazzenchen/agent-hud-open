//! The running Antigravity language servers and CLI, found by their process.
//! A server's token is on its command line, or, since Antigravity 2.34 keeps
//! it off there, in the file the server writes about itself when it starts.
use agent_hud_agents::{antigravity, split};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

pub(super) struct Server {
    pub pid: u32,
    pub token: String,
    pub extension: Option<(u16, String)>,
    /// The client it serves: the desktop app, the IDE or the CLI, by client id.
    pub client: &'static str,
    /// Configured `antigravityHomes` entry this server runs against, if any.
    pub home: Option<PathBuf>,
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

pub(super) fn servers_for(extra_homes: &[PathBuf]) -> Result<(Vec<Server>, Vec<PathBuf>), String> {
    let processes = crate::os::processes()?;
    let env = std::env::vars().collect::<crate::os::Environment>();
    let (saved, pid_homes) = crate::os::home_dir()
        .ok()
        .map(|home| saved_tokens_for(&home, &env, extra_homes))
        .unwrap_or_default();
    Ok(parse_servers_for(
        processes,
        &saved,
        &pid_homes,
        extra_homes,
    ))
}

const MAX_SAVED_BYTES: u64 = 4096;

#[cfg(test)]
fn saved_tokens(home: &Path, env: &crate::os::Environment) -> HashMap<u32, String> {
    saved_tokens_for(home, env, &[]).0
}

fn saved_tokens_for(
    home: &Path,
    env: &crate::os::Environment,
    extra_homes: &[PathBuf],
) -> (HashMap<u32, String>, HashMap<u32, PathBuf>) {
    let mut tokens = HashMap::new();
    let mut pid_homes = HashMap::new();
    let mut dirs: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    for location in antigravity::AGENT.discovery.storage {
        for dir in crate::os::resolve(home, env, location) {
            dirs.push((dir, None));
        }
    }
    for extra in extra_homes {
        for sub in [
            extra.join(".gemini/antigravity-cli"),
            extra.join(".gemini/antigravity"),
            extra.join(".gemini/antigravity-ide"),
            extra.join("antigravity-cli"),
            extra.join("antigravity-acp"),
            extra.clone(),
        ] {
            dirs.push((sub, Some(extra.clone())));
        }
    }
    for (dir, owner_home) in dirs {
        let Ok(entries) = std::fs::read_dir(dir.join("daemon")) else {
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
            if let Some(pid) = pid {
                if let Some(ref extra) = owner_home {
                    pid_homes.entry(pid).or_insert_with(|| extra.clone());
                }
                if let Some(token) = token {
                    tokens.insert(pid, token.to_owned());
                }
            }
        }
    }
    (tokens, pid_homes)
}

fn match_extra_home(candidate: &str, extra_homes: &[PathBuf]) -> Option<PathBuf> {
    let path = Path::new(candidate);
    if !path.is_absolute() {
        return None;
    }
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    for extra in extra_homes {
        let extra_canon = extra.canonicalize().unwrap_or_else(|_| extra.clone());
        if canon.starts_with(&extra_canon) || path.starts_with(extra) {
            return Some(extra.clone());
        }
    }
    None
}

#[cfg(test)]
fn parse_servers(
    processes: Vec<crate::os::ProcessInfo>,
    saved: &HashMap<u32, String>,
) -> Vec<Server> {
    parse_servers_for(processes, saved, &HashMap::new(), &[]).0
}

fn parse_servers_for(
    processes: Vec<crate::os::ProcessInfo>,
    saved: &HashMap<u32, String>,
    pid_homes: &HashMap<u32, PathBuf>,
    extra_homes: &[PathBuf],
) -> (Vec<Server>, Vec<PathBuf>) {
    let mut result = Vec::new();
    let mut active_homes = HashSet::new();
    for process in processes {
        let pid = process.pid;
        let command = process.command.as_str();
        let executable = process.executable.to_lowercase().replace('\\', "/");
        let cli = matches!(executable.rsplit('/').next(), Some("agy" | "agy.exe"));
        let data_dir = flag(command, "app_data_dir").or_else(|| flag(command, "app-data-dir"));
        let gemini_dir = flag(command, "gemini_dir").or_else(|| flag(command, "gemini-dir"));
        let matched_home = pid_homes
            .get(&pid)
            .cloned()
            .or_else(|| {
                data_dir
                    .as_deref()
                    .and_then(|dir| match_extra_home(dir, extra_homes))
            })
            .or_else(|| {
                gemini_dir
                    .as_deref()
                    .and_then(|dir| match_extra_home(dir, extra_homes))
            });
        let data_leaf = data_dir.as_deref().and_then(|dir| {
            Path::new(dir)
                .file_name()
                .and_then(|name| name.to_str())
                .or(Some(dir))
        });
        let server = (executable.contains("language_server")
            || executable.contains("language-server"))
            && (executable.contains("antigravity.app/")
                || executable.contains("antigravity ide.app/")
                || executable.contains("/antigravity/")
                || executable.contains("/antigravity ide/")
                || executable.contains("/antigravity-cli/")
                || executable.contains("/antigravity_cli/")
                || executable.contains("/antigravity-acp/")
                || saved.contains_key(&pid)
                || matched_home.is_some()
                || data_leaf.is_some_and(|leaf| leaf.starts_with("antigravity")));
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
        if let Some(ref home) = matched_home {
            active_homes.insert(home.clone());
        }
        let client = match data_leaf {
            Some("antigravity-cli" | "antigravity-acp") => split::ANTIGRAVITY_CLI,
            Some("antigravity-ide") => split::ANTIGRAVITY_IDE,
            Some("antigravity") => antigravity::AGENT.id,
            _ if cli
                || executable.contains("/antigravity-cli/")
                || executable.contains("/antigravity_cli/")
                || executable.contains("/antigravity-acp/") =>
            {
                split::ANTIGRAVITY_CLI
            }
            _ if executable.contains("antigravity ide.app/")
                || executable.contains("/antigravity ide/") =>
            {
                split::ANTIGRAVITY_IDE
            }
            _ => antigravity::AGENT.id,
        };
        result.push(Server {
            pid,
            token,
            extension,
            client,
            home: matched_home,
            priority: if cli { 1 } else { 0 },
        });
    }
    result.sort_by_key(|server| (server.priority, server.pid));
    (result, active_homes.into_iter().collect())
}

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
