//! DeepSeek Harness's home patch, `$DSH_HOME/cordis.patch.yml`: a YAML list
//! of loader patches that every profile reads, the Desktop app's included,
//! and that a running host reloads when it changes. Harness has no command
//! that adds an MCP server and its Settings only writes a profile's own
//! patch, so Agent HUD keeps one marked block of its own in this file.
//!
//! The file must stay a top-level list: an empty or comments-only file stops
//! every profile from starting. So a removal that leaves nothing writes `[]`,
//! and an edit is checked by parsing the result before it is written.
use super::SERVER_NAME;
use serde_json::Value;
use std::path::Path;

const START: &str = "# >>> agent-hud (added by Agent HUD; remove it in Agent HUD's Settings)";
const END: &str = "# <<< agent-hud";
const CLIENT: &str = "@deepseek-ai/dsh-mcp-client";
const BY_HAND: &str = "DeepSeek Harness's cordis.patch.yml already lists an agent-hud MCP server that Agent HUD did not add. Change or remove it there by hand.";
const LAYOUT: &str = "DeepSeek Harness's cordis.patch.yml is not a list Agent HUD can add to safely. Add the agent-hud MCP server there by hand.";
const LAYOUT_REMOVE: &str = "DeepSeek Harness's cordis.patch.yml is not a list Agent HUD can change safely. Remove the agent-hud MCP server there by hand.";

fn block(executable: &Path) -> String {
    let command = Value::from(executable.to_string_lossy()).to_string();
    format!(
        "{START}\n\
         - insert:\n\
         \x20   - id: {SERVER_NAME}\n\
         \x20     name: \"{CLIENT}\"\n\
         \x20     config:\n\
         \x20       serverName: {SERVER_NAME}\n\
         \x20       transport: stdio\n\
         \x20       command: {command}\n\
         \x20       args: [\"--mcp\"]\n\
         {END}\n"
    )
}

/// The text without Agent HUD's block, and whether it had one.
fn strip(text: &str) -> Result<(String, bool), String> {
    let mut rest = String::with_capacity(text.len());
    let mut inside = false;
    let mut found = false;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end();
        if !inside && bare == START {
            inside = true;
            found = true;
        } else if inside && bare == END {
            inside = false;
        } else if !inside {
            rest.push_str(line);
        }
    }
    if inside {
        return Err(LAYOUT_REMOVE.into());
    }
    Ok((rest, found))
}

/// The lines that are neither blank nor comments.
fn content(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

fn with_newline(mut text: String) -> String {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn items(text: &str) -> Option<Vec<Value>> {
    match serde_saphyr::from_str::<Value>(text).ok()? {
        Value::Array(items) => Some(items),
        _ => None,
    }
}

/// How many patches insert the MCP client for a server called agent-hud,
/// by its plugin id or its server name.
fn servers(items: &[Value]) -> usize {
    items
        .iter()
        .filter_map(|item| item["insert"].as_array())
        .flatten()
        .filter(|plugin| {
            plugin["name"] == CLIENT
                && (plugin["id"] == SERVER_NAME || plugin["config"]["serverName"] == SERVER_NAME)
        })
        .count()
}

pub(super) fn listed(text: Option<&str>) -> bool {
    text.and_then(items)
        .is_some_and(|items| servers(&items) > 0)
}

/// The file's new text with Agent HUD's server added (`Some(executable)`) or
/// removed; None to delete a file that held only Agent HUD's block; the
/// original when nothing changes.
pub(super) fn edit(
    original: Option<&str>,
    executable: Option<&Path>,
) -> Result<Option<String>, String> {
    let unchanged = Ok(original.map(str::to_owned));
    let text = original.unwrap_or("");
    let (rest, found) = strip(text)?;
    let Some(executable) = executable else {
        if !found {
            return if listed(original) {
                Err(BY_HAND.into())
            } else {
                unchanged
            };
        }
        if content(&rest).is_empty() {
            return Ok(if rest.trim().is_empty() {
                None
            } else {
                Some(with_newline(rest) + "[]\n")
            });
        }
        if items(&rest).is_none_or(|items| servers(&items) != 0) {
            return Err(LAYOUT_REMOVE.into());
        }
        return Ok(Some(rest));
    };
    let block = block(executable);
    if found && text.contains(&block) {
        return unchanged;
    }
    if listed(Some(&rest)) {
        return Err(BY_HAND.into());
    }
    let lines = content(&rest);
    let updated = if lines.is_empty() {
        with_newline(rest) + &block
    } else if lines == ["[]"] {
        // An empty flow list cannot be followed by block items: replace it.
        let mut updated = String::with_capacity(rest.len() + block.len());
        for line in rest.split_inclusive('\n') {
            if line.trim() == "[]" {
                updated.push_str(&block);
            } else {
                updated.push_str(line);
            }
        }
        updated
    } else if lines[0] == "-" || lines[0].starts_with("- ") {
        if items(&rest).is_none() {
            return Err(LAYOUT.into());
        }
        with_newline(rest) + &block
    } else {
        return Err(LAYOUT.into());
    };
    if items(&updated).is_none_or(|items| servers(&items) != 1) {
        return Err(LAYOUT.into());
    }
    Ok(Some(updated))
}
