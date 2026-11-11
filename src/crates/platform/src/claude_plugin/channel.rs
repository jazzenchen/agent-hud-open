use super::{
    file_age, identity, millis, valid_name, write_json, write_private, PromptState, Refusal,
    CHECK_IN_LIFETIME, PROMPT_LIFETIME, SCRIPT,
};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// The folder the plugin and the app share.
#[derive(Clone, Debug)]
pub struct ClaudeChannel {
    directory: PathBuf,
}

impl ClaudeChannel {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The sessions that checked in within `CHECK_IN_LIFETIME` and have not ended.
    pub fn loaded(&self, now: SystemTime) -> HashSet<String> {
        let Ok(files) = fs::read_dir(self.directory.join("sessions")) else {
            return HashSet::new();
        };
        files
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let id = path.file_stem()?.to_str()?;
                if path.extension()?.to_str()? != "json" || !valid_name(id) {
                    return None;
                }
                let metadata = fs::symlink_metadata(&path).ok()?;
                if !metadata.is_file() {
                    return None;
                }
                let age = now
                    .duration_since(metadata.modified().ok()?)
                    .unwrap_or_default();
                if age >= CHECK_IN_LIFETIME {
                    return None;
                }
                let value: Value = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
                value.get("ended").is_none().then(|| id.to_string())
            })
            .collect()
    }

    /// When `session` last checked in, however long ago; None when it never
    /// did or has ended.
    pub fn last_check_in(&self, session: &str) -> Option<SystemTime> {
        if !valid_name(session) {
            return None;
        }
        let path = self
            .directory
            .join("sessions")
            .join(format!("{session}.json"));
        let metadata = fs::symlink_metadata(&path).ok()?;
        if !metadata.is_file() {
            return None;
        }
        let value: Value = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
        value
            .get("ended")
            .is_none()
            .then(|| metadata.modified().ok())?
    }

    /// Writes the plugin's files into `folder` (an absolute path), pointing
    /// it at this channel, and makes the channel's folders. Returns whether a
    /// file changed. Only what differs is written.
    pub fn prepare_plugin(&self, folder: &Path) -> Result<bool, String> {
        if !folder.is_absolute() {
            return Err("The plugin folder must be absolute".into());
        }
        crate::os::private_directory(&self.directory)?;
        crate::os::private_directory(&self.directory.join("sessions"))?;
        crate::os::private_directory(&self.directory.join("prompts"))?;
        let channel = serde_json::to_string(&self.directory.to_string_lossy()).unwrap();
        let files = [
            (
                ".claude-plugin/marketplace.json",
                json!({"name":"agent-hud","owner":{"name":"Agent HUD"},"plugins":[{"name":"agent-hud","source":"./"}]}).to_string(),
            ),
            (
                ".claude-plugin/plugin.json",
                json!({"name":"agent-hud","version":"1.1.0","description":"Delivers user-authorized Agent HUD input to this existing session.","author":{"name":"Agent HUD"}}).to_string(),
            ),
            ("hooks/hooks.json", json!({"modules":["./register.ts"]}).to_string()),
            ("hooks/register.ts", SCRIPT.replace("__AGENT_HUD_CHANNEL_JSON__", &channel)),
        ];
        crate::os::private_directory(folder)?;
        let mut changed = false;
        for (relative, text) in files {
            let file = folder.join(relative);
            if fs::read(&file).ok().as_deref() == Some(text.as_bytes()) {
                continue;
            }
            crate::os::private_directory(file.parent().ok_or("The plugin folder is unavailable")?)?;
            write_private(&file, text.as_bytes()).map_err(|e| e.to_string())?;
            changed = true;
        }
        Ok(changed)
    }

    /// Forgets every check-in and every word left, when the plugin is removed.
    pub fn clear(&self) -> Result<(), String> {
        for name in ["sessions", "prompts"] {
            match fs::remove_dir_all(self.directory.join(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        Ok(())
    }

    /// Leaves `text` for `session` under the caller's identity `id`, as its
    /// user's own words (`as_user`) or labelled as coming from Agent HUD. The
    /// caller checks the session is loaded first.
    pub fn leave(
        &self,
        session: &str,
        id: &str,
        text: &str,
        as_user: bool,
        now: SystemTime,
    ) -> Result<(), Refusal> {
        let file = self.prompt(session, id)?;
        if file.exists() {
            return Err(Refusal::IdentityConflict);
        }
        let at = millis(now);
        write_json(
            &file,
            &json!({"id":id,"text":text,"at":at,"expiresAt":at.saturating_add(PROMPT_LIFETIME.as_millis() as u64),"asUser":as_user}),
        )
    }

    /// Leaves `text` for `session` as the answer to its question: refused when
    /// it is empty, starts with a slash (Claude Code would run a command as the
    /// user) or the session is not running the plugin. Returns its identity.
    /// Each answer clears what earlier ones left, so the folder does not grow.
    pub fn hand(&self, session: &str, text: &str, now: SystemTime) -> Result<String, Refusal> {
        if text.trim().is_empty() {
            return Err(Refusal::InvalidInput("Nothing to send".into()));
        }
        if text.trim_start().starts_with('/') {
            return Err(Refusal::Rejected(
                "Claude plugins cannot submit slash commands".into(),
            ));
        }
        if !valid_name(session) {
            return Err(Refusal::InvalidInput("identity".into()));
        }
        if !self.loaded(now).contains(session) {
            return Err(Refusal::NotLoaded);
        }
        self.sweep(now);
        let id = identity(now);
        self.leave(session, &id, text, false, now)?;
        Ok(id)
    }

    /// How the words `id` left for `session` stand, and the plugin's reason
    /// when it refused them; None when there is no such file.
    pub fn state(
        &self,
        session: &str,
        id: &str,
        now: SystemTime,
    ) -> Result<Option<(PromptState, Option<String>)>, Refusal> {
        let value: Value = match fs::read(self.prompt(session, id)?) {
            Ok(data) => {
                serde_json::from_slice(&data).map_err(|e| Refusal::Protocol(e.to_string()))?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let reason = value
            .get("refused")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let state = if value.get("cancelledAt").is_some() {
            PromptState::Cancelled
        } else if value.get("expiredAt").is_some() {
            PromptState::Expired
        } else if value.get("refused").is_some() {
            PromptState::Refused
        } else if value.get("startedAt").is_some() {
            PromptState::Started
        } else if value.get("takenAt").is_some() {
            PromptState::Taken
        } else if value
            .get("expiresAt")
            .and_then(Value::as_u64)
            .is_some_and(|at| at <= millis(now))
        {
            PromptState::Expired
        } else if self.cancel_path(session, id)?.exists() {
            PromptState::CancelRequested
        } else {
            PromptState::Waiting
        };
        Ok(Some((state, reason)))
    }

    /// Asks the plugin not to submit `id`. Only the plugin can confirm a
    /// withdrawal; once it took the words, they can no longer be withdrawn.
    pub fn cancel(&self, session: &str, id: &str) -> Result<PromptState, Refusal> {
        let Some((state, _)) = self.state(session, id, SystemTime::now())? else {
            return Err(Refusal::Unsupported(
                "Prompt file no longer exists; acceptance cannot be inferred".into(),
            ));
        };
        match state {
            PromptState::Waiting | PromptState::CancelRequested => {
                write_json(
                    &self.cancel_path(session, id)?,
                    &json!({"cancelledAt":millis(SystemTime::now())}),
                )?;
                Ok(PromptState::CancelRequested)
            }
            PromptState::Cancelled | PromptState::Expired | PromptState::Refused => Ok(state),
            PromptState::Taken | PromptState::Started => Err(Refusal::Unsupported(
                "The plugin already took this input; it cannot be withdrawn safely".into(),
            )),
        }
    }

    /// Removes check-ins older than a day, and words taken a minute ago or
    /// older than an hour.
    pub fn sweep(&self, now: SystemTime) {
        if let Ok(files) = fs::read_dir(self.directory.join("sessions")) {
            for entry in files.flatten() {
                let path = entry.path();
                if file_age(&path, now) > Duration::from_secs(86_400) {
                    let _ = fs::remove_file(path);
                }
            }
        }
        if let Ok(folders) = fs::read_dir(self.directory.join("prompts")) {
            for folder in folders.flatten() {
                if let Ok(files) = fs::read_dir(folder.path()) {
                    for entry in files.flatten() {
                        let path = entry.path();
                        let age = file_age(&path, now);
                        let taken = fs::read(&path)
                            .ok()
                            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                            .is_some_and(|value| value.get("takenAt").is_some());
                        if age > PROMPT_LIFETIME || (taken && age > Duration::from_secs(60)) {
                            let _ = fs::remove_file(path);
                        }
                    }
                }
                let _ = fs::remove_dir(folder.path());
            }
        }
    }

    fn prompt(&self, session: &str, id: &str) -> Result<PathBuf, Refusal> {
        if !valid_name(session) || !valid_name(id) {
            return Err(Refusal::InvalidInput("identity".into()));
        }
        Ok(self
            .directory
            .join("prompts")
            .join(session)
            .join(format!("{id}.json")))
    }

    pub(super) fn cancel_path(&self, session: &str, id: &str) -> Result<PathBuf, Refusal> {
        Ok(self.prompt(session, id)?.with_extension("cancel"))
    }
}
