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
    /// Whether the entry starts `executable`, by this name or another.
    pub(super) fn starts(&self, executable: &Path) -> bool {
        self.command.as_deref().is_some_and(|command| {
            Path::new(command) == executable
                || fs::canonicalize(command)
                    .ok()
                    .is_some_and(|found| fs::canonicalize(executable).ok() == Some(found))
        })
    }
    /// Whether the entry starts `executable` as `client`'s entry should:
    /// with the client's own arguments (`Client::server_args`), or for Claude
    /// Code also plain `--mcp`, as before entries named their client. The
    /// server reads Claude Code's session from the environment for both, so
    /// Claude Code's older entry is never rewritten just to add the name, and
    /// a 0.6 copy that writes plain `--mcp` back is not fought.
    pub(super) fn current(&self, client: Client, executable: &Path) -> bool {
        self.starts(executable)
            && (self.args == client.server_args()
                || (client == Client::ClaudeCode && self.args == ["--mcp"]))
    }
}

/// The settings file the client's command line writes the server into.
pub(super) fn file(client: Client, places: &Places) -> Option<PathBuf> {
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

    /// The entry as Agent HUD writes it for `client` now.
    fn named(command: &str, client: Client) -> Entry {
        Entry {
            command: Some(command.into()),
            args: client.server_args().into(),
        }
    }

    #[test]
    fn an_entry_naming_another_copy_or_other_arguments_is_not_current() {
        let app = Path::new(APP);
        assert!(named(APP, Client::Codex).current(Client::Codex, app));
        assert!(!named(
            "/Users/me/Downloads/Agent HUD.app/Contents/MacOS/Agent HUD",
            Client::Codex
        )
        .current(Client::Codex, app));
        // Written before entries named their client, or for another client.
        assert!(!entry(APP).current(Client::Codex, app));
        assert!(!named(APP, Client::ClaudeCode).current(Client::Codex, app));
        let mut extra = named(APP, Client::Codex);
        extra.args.push("--verbose".into());
        assert!(!extra.current(Client::Codex, app));
        let none = Entry {
            command: None,
            args: Client::Codex.server_args().into(),
        };
        assert!(!none.current(Client::Codex, app));
    }

    /// Claude Code's server reads its session from the environment whether
    /// or not the entry names Claude Code, so its older entry stays.
    #[test]
    fn claude_codes_plain_entry_is_current() {
        let app = Path::new(APP);
        assert!(entry(APP).current(Client::ClaudeCode, app));
        assert!(named(APP, Client::ClaudeCode).current(Client::ClaudeCode, app));
        assert!(!named(APP, Client::Codex).current(Client::ClaudeCode, app));
        assert!(!entry("/old/Agent HUD").current(Client::ClaudeCode, app));
    }

    #[test]
    fn the_same_file_by_another_name_is_current() {
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("Agent HUD");
        fs::write(&real, "").unwrap();
        let other_name = temp.path().join(".").join("Agent HUD");
        assert!(named(&other_name.to_string_lossy(), Client::ClaudeCode)
            .current(Client::ClaudeCode, &real));
    }
}
