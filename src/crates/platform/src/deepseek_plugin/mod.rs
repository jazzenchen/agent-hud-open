//! Agent HUD's plugin for DeepSeek Harness (`agent-hud.mjs`), and the folder
//! it shares with the app. With it, Agent HUD knows which Harness session
//! called one of its MCP tools, can leave words for a running session as its
//! next message, and shows the questions and approvals a session waits on in
//! the HUD, next to Harness's own cards: whichever is answered first wins.
//!
//! Installing it writes the module and one marked row in Harness's home
//! patch (`clients::deepseek::patch`), which every Harness host reads, the
//! Desktop app's included, and reloads when it changes. The row's id carries
//! the module's version and where the app's folder is, and the file's name
//! the version, because Harness does not reload a new file or folder under an
//! id it already runs.
//!
//! The folder holds what the plugin writes (`sessions/` check-ins, `calls/`
//! notes, `asks/` waiting questions and approvals) and what the app writes
//! (`prompts/` words for a session, `asks/<id>.answer`, `app.json` while the
//! app watches asks). Check-ins and prompts are the Claude Code plugin's
//! protocol, read and written with `ClaudeChannel`'s code.
use crate::clients::deepseek::patch::{self, Block};
use crate::hud_tools::Places;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub mod asks;
mod channel;

pub use channel::DeepSeekChannel;

/// The plugin module Harness loads.
pub const SCRIPT: &str = include_str!("agent-hud.mjs");
/// A note older than this belongs to a call that ended without removing it.
pub const CALL_LIFETIME: Duration = Duration::from_secs(120);
/// Every version's row id starts with this.
const ROW: &str = "agent-hud-plugin";

const PLUGIN: Block = Block {
    start: "# >>> agent-hud-plugin (added by Agent HUD; remove it in Agent HUD's Settings)",
    end: "# <<< agent-hud-plugin",
    count: rows,
    by_hand: "DeepSeek Harness's cordis.patch.yml already loads an Agent HUD plugin that Agent HUD did not add. Change or remove it there by hand.",
    layout: "DeepSeek Harness's cordis.patch.yml is not a list Agent HUD can add to safely. Add Agent HUD's plugin there by hand.",
    layout_remove: "DeepSeek Harness's cordis.patch.yml is not a list Agent HUD can change safely. Remove Agent HUD's plugin there by hand.",
};

/// The module's version: the start of its text's SHA-256.
pub fn version() -> String {
    Sha256::digest(SCRIPT.as_bytes())
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The module's file name, which changes with its version.
pub fn file_name() -> String {
    format!("agent-hud-{}.mjs", version())
}

/// The channel folder in the app's data folder.
pub fn channel_directory(data: &Path) -> PathBuf {
    data.join("deepseek-channel")
}

/// The folder the module is written to, in the app's data folder.
pub fn plugin_directory(data: &Path) -> PathBuf {
    data.join("deepseek-plugin")
}

/// How many patches load a plugin under one of Agent HUD's row ids.
fn rows(items: &[Value]) -> usize {
    patch::inserted(items)
        .filter(|plugin| plugin["id"].as_str().is_some_and(|id| id.starts_with(ROW)))
        .count()
}

/// The row's id: the module's version, then a short hash of `channel`. A
/// moved data folder gets a new id, so Harness drops the module that writes
/// to the old folder and loads the one in the new folder.
fn row_id(channel: &Path) -> String {
    let place: String = Sha256::digest(channel.to_string_lossy().as_bytes())
        .iter()
        .take(3)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{ROW}-{}-{place}", version())
}

/// The row that loads `file` and points it at `channel`.
fn block(file: &Path, channel: &Path) -> String {
    format!(
        "{start}\n\
         - insert:\n\
         \x20   - id: {id}\n\
         \x20     name: {file}\n\
         \x20     config:\n\
         \x20       channel: {channel}\n\
         {end}\n",
        start = PLUGIN.start,
        end = PLUGIN.end,
        id = row_id(channel),
        file = patch::quoted(&file.to_string_lossy()),
        channel = patch::quoted(&channel.to_string_lossy()),
    )
}

fn patch_file(places: &Places) -> PathBuf {
    places.deepseek.join("cordis.patch.yml")
}

/// Whether Harness's home patch loads the plugin through Agent HUD's row.
pub fn installed(places: &Places) -> bool {
    crate::hud_tools::read(&patch_file(places))
        .ok()
        .flatten()
        .is_some_and(|text| PLUGIN.current(&text).is_some() && PLUGIN.listed(Some(&text)))
}

/// Whether Agent HUD's row loads this version of `file` with `channel`.
pub fn installed_as(places: &Places, file: &Path, channel: &Path) -> bool {
    crate::hud_tools::read(&patch_file(places))
        .ok()
        .flatten()
        .is_some_and(|text| PLUGIN.current(&text) == Some(block(file, channel)))
}

/// Adds Agent HUD's row for `file` and `channel` (`Some`), or takes it out
/// (`None`). Returns whether the file changed. A file Agent HUD cannot edit
/// without risk to the rest of it is refused, as is a row it did not write.
pub fn set_row(places: &Places, plugin: Option<(&Path, &Path)>) -> Result<bool, String> {
    let file = patch_file(places);
    let target = crate::hooks::settings_target(&file)?;
    let original = crate::hud_tools::read(&target)?;
    let block = plugin.map(|(file, channel)| block(file, channel));
    let updated = PLUGIN.edit(original.as_deref(), block.as_deref())?;
    if updated == original {
        return Ok(false);
    }
    crate::hooks::write_settings(&target, updated.as_deref().map(str::as_bytes))?;
    Ok(true)
}

/// Removes the module's other versions from `folder`, keeping `keep`.
pub fn remove_other_versions(folder: &Path, keep: &Path) {
    let Ok(files) = fs::read_dir(folder) else {
        return;
    };
    for entry in files.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path != keep && name.starts_with("agent-hud-") && name.ends_with(".mjs") {
            let _ = fs::remove_file(path);
        }
    }
}

/// Removes every version of the module from `folder`.
pub fn remove_plugin(folder: &Path) {
    remove_other_versions(folder, Path::new(""));
    let _ = fs::remove_dir(folder);
}

#[cfg(test)]
mod tests;
