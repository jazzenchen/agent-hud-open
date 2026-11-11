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
    edit_command(
        client,
        original,
        executable.map(|path| path.to_string_lossy().into_owned()),
    )
}

/// [`edit`] for TRAE, which splits a `command` at its spaces unless the
/// whole of it is in double quotes (its extension host's command parser), so
/// `/Applications/Agent HUD.app/…` would start `/Applications/Agent`.
pub(super) fn edit_quoted(
    client: &str,
    original: Option<&str>,
    executable: Option<&Path>,
) -> Result<Option<String>, String> {
    edit_command(
        client,
        original,
        executable.map(|path| format!("\"{}\"", path.to_string_lossy())),
    )
}

fn edit_command(
    client: &str,
    original: Option<&str>,
    command: Option<String>,
) -> Result<Option<String>, String> {
    let unchanged = Ok(original.map(str::to_owned));
    let mut value = match original.filter(|text| !text.trim().is_empty()) {
        None if command.is_none() => return unchanged,
        None => Value::Object(Map::new()),
        Some(text) => serde_json::from_str(text).map_err(|_| not_plain(client))?,
    };
    let root = value.as_object_mut().ok_or_else(|| not_plain(client))?;
    match command {
        Some(command) => {
            let servers = root
                .entry("mcpServers")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| not_plain(client))?;
            let command = json!(command);
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
    let bytes = crate::json_text::rewrite(original.map(str::as_bytes), &value).unwrap();
    Ok(Some(String::from_utf8(bytes).unwrap()))
}

/// Whether ZCode's `config.json` lists Agent HUD's server under `mcp.servers`.
pub(super) fn listed_zcode(text: Option<&str>) -> bool {
    text.and_then(|text| serde_json::from_str::<Value>(text).ok())
        .is_some_and(|value| value["mcp"]["servers"][SERVER_NAME].is_object())
}

/// Updates ZCode's `config.json` (`mcp.servers.agent-hud` with `type: "stdio"`).
pub(super) fn edit_zcode(
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
            let mcp = root
                .entry("mcp")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| not_plain(client))?;
            let servers = mcp
                .entry("servers")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| not_plain(client))?;
            let command = json!(executable.to_string_lossy());
            let args = json!(["--mcp"]);
            let kind = json!("stdio");
            match servers.get_mut(SERVER_NAME).and_then(Value::as_object_mut) {
                Some(entry) => {
                    if entry.get("type") == Some(&kind)
                        && entry.get("command") == Some(&command)
                        && entry.get("args") == Some(&args)
                    {
                        return unchanged;
                    }
                    entry.insert("type".into(), kind);
                    entry.insert("command".into(), command);
                    entry.insert("args".into(), args);
                }
                None => {
                    servers.insert(
                        SERVER_NAME.into(),
                        json!({"type": kind, "command": command, "args": args}),
                    );
                }
            }
        }
        None => {
            let Some(servers) = root
                .get_mut("mcp")
                .and_then(Value::as_object_mut)
                .and_then(|mcp| mcp.get_mut("servers"))
                .and_then(Value::as_object_mut)
            else {
                return unchanged;
            };
            if servers.remove(SERVER_NAME).is_none() {
                return unchanged;
            }
        }
    }
    let bytes = crate::json_text::rewrite(original.map(str::as_bytes), &value).unwrap();
    Ok(Some(String::from_utf8(bytes).unwrap()))
}
