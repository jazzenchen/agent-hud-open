//! The JSON MCP settings files Agent HUD edits itself, for the clients that
//! have no command that adds a server: Cursor's `~/.cursor/mcp.json` and
//! Antigravity's `~/.gemini/config/mcp_config.json`. Both keep servers under
//! `mcpServers`. Agent HUD sets its own entry and leaves the rest of the
//! file as it was.
use super::SERVER_NAME;
use serde_json::{json, Map, Value};
use std::path::Path;

fn not_plain(client: &str) -> String {
    format!("{client}'s MCP settings file is not plain JSON (it may hold comments), so Agent HUD left it alone. Add the agent-hud server there by hand.")
}

/// Whether the file lists Agent HUD's server. One the person turned off in
/// the client (Antigravity's `disabled`) still counts, so Settings offers to
/// remove it; Cursor ignores that key.
pub(super) fn listed(text: Option<&str>) -> bool {
    text.and_then(|text| serde_json::from_str::<Value>(text).ok())
        .is_some_and(|value| value["mcpServers"][SERVER_NAME].is_object())
}

/// The file's new text with Agent HUD's server added (`Some(executable)`) or
/// removed; the original when nothing changes.
pub(super) fn edit(
    client: &str,
    original: Option<&str>,
    executable: Option<&Path>,
) -> Result<Option<String>, String> {
    let unchanged = Ok(original.map(str::to_owned));
    let mut value = match original.filter(|text| !text.trim().is_empty()) {
        None if executable.is_none() => return unchanged,
        None => Value::Object(Map::new()),
        Some(text) => serde_json::from_str(text).map_err(|_| not_plain(client))?,
    };
    let root = value.as_object_mut().ok_or_else(|| not_plain(client))?;
    match executable {
        Some(executable) => {
            let servers = root
                .entry("mcpServers")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| not_plain(client))?;
            let command = json!(executable.to_string_lossy());
            let args = json!(["--mcp"]);
            match servers.get_mut(SERVER_NAME).and_then(Value::as_object_mut) {
                // Keep whatever else the person set on it, such as env; adding
                // it again turns it back on.
                Some(entry) => {
                    if entry.get("command") == Some(&command)
                        && entry.get("args") == Some(&args)
                        && entry.get("disabled") != Some(&Value::Bool(true))
                    {
                        return unchanged;
                    }
                    entry.insert("command".into(), command);
                    entry.insert("args".into(), args);
                    entry.remove("disabled");
                }
                None => {
                    servers.insert(
                        SERVER_NAME.into(),
                        json!({"command": command, "args": args}),
                    );
                }
            }
        }
        None => {
            let Some(servers) = root.get_mut("mcpServers").and_then(Value::as_object_mut) else {
                return unchanged;
            };
            if servers.remove(SERVER_NAME).is_none() {
                return unchanged;
            }
        }
    }
    let saved = || format!("{client}'s MCP settings could not be saved");
    let bytes =
        crate::json_text::rewrite(original.map(str::as_bytes), &value).map_err(|_| saved())?;
    String::from_utf8(bytes).map(Some).map_err(|_| saved())
}
