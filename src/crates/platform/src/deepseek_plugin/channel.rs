//! The folder the plugin and the app share: check-ins and words for a
//! session, the notes that name a call's session, and the module itself.
use super::{file_name, CALL_LIFETIME, SCRIPT};
use crate::claude_plugin::{file_age, valid_name, ClaudeChannel, PromptState, Refusal};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// Notes, answers and asks a crashed host left behind go after a day.
const LEFTOVER_LIFETIME: Duration = Duration::from_secs(86_400);

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
                || file_age(&path, now) > CALL_LIFETIME
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
                        if file_age(&note.path(), now) > CALL_LIFETIME {
                            let _ = fs::remove_file(note.path());
                        }
                    }
                }
                let _ = fs::remove_dir(host.path());
            }
        }
        if let Ok(files) = fs::read_dir(self.directory.join("asks")) {
            for file in files.flatten() {
                if file_age(&file.path(), now) > LEFTOVER_LIFETIME {
                    let _ = fs::remove_file(file.path());
                }
            }
        }
    }
}
