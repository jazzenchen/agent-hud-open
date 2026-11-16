//! Agent HUD's server as Claude Code and Codex list it in their own settings,
//! which only their command lines write: whether it is there, and whether it
//! still starts this copy of Agent HUD. A copy that moved leaves the entry
//! naming a file that is gone; the client then cannot start the server, and
//! its sessions take messages through the hooks but cannot send any (Open #100).
use agent_hud_runtime::hud_tools::{Client, Places};
use serde_json::Value;
use std::{fs, path::Path, path::PathBuf};

const SERVER: &str = "agent-hud";

#[derive(Debug, PartialEq)]
pub(super) struct Entry {
    command: Option<String>,
    args: Vec<String>,
}

impl Entry {
    /// Whether the entry starts something other than `executable --mcp`.
    pub(super) fn points_elsewhere(&self, executable: &Path) -> bool {
        let same_file = self.command.as_deref().is_some_and(|command| {
            Path::new(command) == executable
                || fs::canonicalize(command)
                    .ok()
                    .is_some_and(|found| fs::canonicalize(executable).ok() == Some(found))
        });
        !same_file || self.args != ["--mcp"]
    }
}

/// The settings file the client's command line writes the server into.
fn file(client: Client, places: &Places) -> Option<PathBuf> {
    let (variable, folder, name) = match client {
        Client::ClaudeCode => ("CLAUDE_CONFIG_DIR", places.home.clone(), ".claude.json"),
        Client::Codex => ("CODEX_HOME", places.home.join(".codex"), "config.toml"),
        _ => return None,
    };
    let folder = std::env::var_os(variable).map_or(folder, PathBuf::from);
    Some(folder.join(name))
}

/// The entry in a client whose command line adds the server; None for the
/// others, and when the file is missing, too large or unreadable.
pub(super) fn listed(client: Client, places: &Places) -> Option<Entry> {
    let path = file(client, places)?;
    if fs::metadata(&path).ok()?.len() > 4 * 1024 * 1024 {
        return None;
    }
    parse(client, &fs::read_to_string(path).ok()?)
}

fn parse(client: Client, text: &str) -> Option<Entry> {
    let strings = |values: Option<Vec<Option<&str>>>| -> Vec<String> {
        values
            .unwrap_or_default()
            .into_iter()
            .map(|value| value.unwrap_or_default().to_owned())
            .collect()
    };
    if client == Client::ClaudeCode {
        let value = serde_json::from_str::<Value>(text).ok()?;
        let server = value.get("mcpServers")?.get(SERVER)?;
        return Some(Entry {
            command: server["command"].as_str().map(str::to_owned),
            args: strings(
                server["args"]
                    .as_array()
                    .map(|items| items.iter().map(Value::as_str).collect()),
            ),
        });
    }
    let value = toml::from_str::<toml::Value>(text).ok()?;
    let server = value.get("mcp_servers")?.get(SERVER)?;
    Some(Entry {
        command: server
            .get("command")
            .and_then(toml::Value::as_str)
            .map(str::to_owned),
        args: strings(
            server
                .get("args")
                .and_then(toml::Value::as_array)
                .map(|items| items.iter().map(toml::Value::as_str).collect()),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = "/Applications/Agent HUD.app/Contents/MacOS/Agent HUD";

    fn entry(command: &str) -> Entry {
        Entry {
            command: Some(command.into()),
            args: vec!["--mcp".into()],
        }
    }

    #[test]
    fn reads_claude_codes_user_server() {
        let text = format!(
            r#"{{"projects":{{}},"mcpServers":{{"other":{{"command":"npx"}},"agent-hud":{{"type":"stdio","command":"{APP}","args":["--mcp"],"env":{{}}}}}}}}"#
        );
        assert_eq!(parse(Client::ClaudeCode, &text), Some(entry(APP)));
        assert_eq!(
            parse(Client::ClaudeCode, r#"{"mcpServers":{"other":{}}}"#),
            None
        );
        assert_eq!(parse(Client::ClaudeCode, "not json"), None);
    }

    #[test]
    fn reads_codexs_server() {
        let text = format!(
            "model = \"x\"\n\n[mcp_servers.agent-hud]\ncommand = \"{APP}\"\nargs = [\"--mcp\"]\n"
        );
        assert_eq!(parse(Client::Codex, &text), Some(entry(APP)));
        assert_eq!(
            parse(Client::Codex, "[mcp_servers.other]\ncommand = \"y\"\n"),
            None
        );
    }

    #[test]
    fn an_entry_naming_another_copy_or_other_arguments_points_elsewhere() {
        let app = Path::new(APP);
        assert!(!entry(APP).points_elsewhere(app));
        assert!(
            entry("/Users/me/Downloads/Agent HUD.app/Contents/MacOS/Agent HUD")
                .points_elsewhere(app)
        );
        let mut extra = entry(APP);
        extra.args.push("--verbose".into());
        assert!(extra.points_elsewhere(app));
        let none = Entry {
            command: None,
            args: vec!["--mcp".into()],
        };
        assert!(none.points_elsewhere(app));
    }

    #[test]
    fn the_same_file_by_another_name_is_not_elsewhere() {
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("Agent HUD");
        fs::write(&real, "").unwrap();
        let other_name = temp.path().join(".").join("Agent HUD");
        assert!(!entry(&other_name.to_string_lossy()).points_elsewhere(&real));
    }
}
