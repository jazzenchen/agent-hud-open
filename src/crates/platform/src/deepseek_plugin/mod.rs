//! Agent HUD's plugin for DeepSeek Harness (`agent-hud.mjs`), and the folder
//! it shares with the app. With it, Agent HUD knows which Harness session
//! called one of its MCP tools, can leave words for a running session as its
//! next message, and shows the questions and approvals a session waits on in
//! the HUD, next to Harness's own cards: whichever is answered first wins.
//!
//! Installing it writes the module and one marked row in Harness's home
//! patch (`clients::deepseek::patch`), which every Harness host reads, the
//! Desktop app's included, and reloads when it changes. The row's id and the
//! file's name carry the module's version, because Harness does not reload a
//! new file under an id it already runs.
//!
//! The folder holds what the plugin writes (`sessions/` check-ins, `calls/`
//! notes, `asks/` waiting questions and approvals) and what the app writes
//! (`prompts/` words for a session, `asks/<id>.answer`, `app.json` while the
//! app watches asks). Check-ins and prompts are the Claude Code plugin's
//! protocol, read and written with `ClaudeChannel`'s code.
use crate::claude_plugin::{valid_name, ClaudeChannel, PromptState, Refusal};
use crate::clients::deepseek::patch::{self, Block};
use crate::hud_tools::Places;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

pub mod asks;
#[cfg(test)]
mod tests;

/// The plugin module Harness loads.
pub const SCRIPT: &str = include_str!("agent-hud.mjs");
/// A note older than this belongs to a call that ended without removing it.
pub const CALL_LIFETIME: Duration = Duration::from_secs(120);
/// Notes, answers and asks a crashed host left behind go after a day.
const LEFTOVER_LIFETIME: Duration = Duration::from_secs(86_400);
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

/// The row that loads `file` and points it at `channel`.
fn block(file: &Path, channel: &Path) -> String {
    format!(
        "{start}\n\
         - insert:\n\
         \x20   - id: {ROW}-{version}\n\
         \x20     name: {file}\n\
         \x20     config:\n\
         \x20       channel: {channel}\n\
         {end}\n",
        start = PLUGIN.start,
        end = PLUGIN.end,
        version = version(),
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

/// The folder the plugin and the app share.
#[derive(Clone, Debug)]
pub struct DeepSeekChannel {
    directory: PathBuf,
    prompts: ClaudeChannel,
}

impl DeepSeekChannel {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            prompts: ClaudeChannel::new(directory.clone()),
            directory,
        }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The sessions that checked in lately and have not ended.
    pub fn loaded(&self, now: SystemTime) -> HashSet<String> {
        self.prompts.loaded(now)
    }

    /// When `session` last checked in; None when it never did or has ended.
    pub fn last_check_in(&self, session: &str) -> Option<SystemTime> {
        self.prompts.last_check_in(session)
    }

    /// Leaves `text` for `session` as its next message, under the caller's
    /// identity `id`. The caller checks the session is loaded first.
    pub fn leave(
        &self,
        session: &str,
        id: &str,
        text: &str,
        now: SystemTime,
    ) -> Result<(), Refusal> {
        self.prompts.leave(session, id, text, false, now)
    }

    /// Leaves `text` for a loaded `session`; refused when it is empty or
    /// starts with a slash. Returns its identity.
    pub fn hand(&self, session: &str, text: &str, now: SystemTime) -> Result<String, Refusal> {
        self.prompts.hand(session, text, now)
    }

    /// How the words `id` left for `session` stand.
    pub fn state(
        &self,
        session: &str,
        id: &str,
        now: SystemTime,
    ) -> Result<Option<(PromptState, Option<String>)>, Refusal> {
        self.prompts.state(session, id, now)
    }

    /// Asks the plugin not to submit `id`.
    pub fn cancel(&self, session: &str, id: &str) -> Result<PromptState, Refusal> {
        self.prompts.cancel(session, id)
    }

    /// The session calling `tool` with `arguments` in the Harness host whose
    /// process is `host`, from the note the plugin leaves while the call
    /// runs. None when no note matches, or when two sessions make the same
    /// call at once.
    pub fn caller(
        &self,
        host: u32,
        tool: &str,
        arguments: &Value,
        now: SystemTime,
    ) -> Option<String> {
        let folder = self.directory.join("calls").join(host.to_string());
        let mut found: Option<String> = None;
        for entry in fs::read_dir(folder).ok()?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json")
                || age(&path, now) > CALL_LIFETIME
            {
                continue;
            }
            let Some(note) = fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            else {
                continue;
            };
            if note["tool"] != tool || note["arguments"] != *arguments {
                continue;
            }
            let Some(session) = note["sessionId"].as_str().filter(|s| valid_name(s)) else {
                continue;
            };
            match &found {
                None => found = Some(session.to_owned()),
                Some(earlier) if earlier == session => {}
                Some(_) => return None,
            }
        }
        found
    }

    /// Writes the module into `folder` (an absolute path) and makes the
    /// channel's folders. Returns the module's path. Other versions stay
    /// until the row points at this one (`remove_other_versions`), so the
    /// row never names a missing file.
    pub fn prepare_plugin(&self, folder: &Path) -> Result<PathBuf, String> {
        if !folder.is_absolute() {
            return Err("The plugin folder must be absolute".into());
        }
        for name in ["", "sessions", "prompts", "calls", "asks"] {
            crate::os::private_directory(&self.directory.join(name))?;
        }
        crate::os::private_directory(folder)?;
        let file = folder.join(file_name());
        if fs::read(&file).ok().as_deref() != Some(SCRIPT.as_bytes()) {
            crate::claude_plugin::write_private(&file, SCRIPT.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        Ok(file)
    }

    /// Forgets every check-in, word, note and ask, when the plugin is removed.
    pub fn clear(&self) -> Result<(), String> {
        self.prompts.clear()?;
        for name in ["calls", "asks"] {
            match fs::remove_dir_all(self.directory.join(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        let _ = fs::remove_file(self.directory.join("app.json"));
        Ok(())
    }

    /// Removes old check-ins and words, notes of calls that ended long ago,
    /// and asks and answers a stopped host left behind.
    pub fn sweep(&self, now: SystemTime) {
        self.prompts.sweep(now);
        if let Ok(hosts) = fs::read_dir(self.directory.join("calls")) {
            for host in hosts.flatten() {
                if let Ok(notes) = fs::read_dir(host.path()) {
                    for note in notes.flatten() {
                        if age(&note.path(), now) > CALL_LIFETIME {
                            let _ = fs::remove_file(note.path());
                        }
                    }
                }
                let _ = fs::remove_dir(host.path());
            }
        }
        if let Ok(files) = fs::read_dir(self.directory.join("asks")) {
            for file in files.flatten() {
                if age(&file.path(), now) > LEFTOVER_LIFETIME {
                    let _ = fs::remove_file(file.path());
                }
            }
        }
    }
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

fn age(path: &Path, now: SystemTime) -> Duration {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .unwrap_or_default()
}
