use super::DeepSeekChannel;
use crate::claude_plugin::{
    file_age, millis, valid_name, write_json, ClaudeChannel, PromptState, Refusal,
    CHECK_IN_LIFETIME,
};
use serde_json::{json, Value};
use std::{
    fs,
    time::{Duration, SystemTime},
};

pub(crate) const SELECTED_LIFETIME: Duration = Duration::from_secs(8);

impl DeepSeekChannel {
    pub(super) fn selected(&self) -> ClaudeChannel {
        ClaudeChannel::new(self.directory.join("selected"))
    }

    pub(crate) fn selected_generation(&self, session: &str, now: SystemTime) -> Option<String> {
        if !valid_name(session) {
            return None;
        }
        let file = self
            .directory
            .join("sessions")
            .join(format!("{session}.json"));
        if file_age(&file, now) >= CHECK_IN_LIFETIME {
            return None;
        }
        let check: Value = serde_json::from_slice(&fs::read(file).ok()?).ok()?;
        if check.get("ended").is_some() {
            return None;
        }
        check["selectedGeneration"]
            .as_str()
            .filter(|id| valid_name(id))
            .map(str::to_owned)
    }

    /// Separate from normal prompt files, so an old plugin cannot consume it.
    pub(crate) fn leave_selected(
        &self,
        session: &str,
        id: &str,
        text: &str,
        generation: &str,
        model: Option<&Value>,
        effort: Option<&str>,
        now: SystemTime,
    ) -> Result<(), Refusal> {
        let file = self.selected().prompt(session, id)?;
        if file.exists() {
            return Err(Refusal::IdentityConflict);
        }
        let at = millis(now);
        let mut entry = json!({
            "id": id, "text": text, "at": at, "generation": generation,
            "expiresAt": at.saturating_add(SELECTED_LIFETIME.as_millis() as u64),
        });
        if let Some(model) = model {
            entry["model"] = model.clone();
        }
        if let Some(effort) = effort {
            entry["reasoningEffort"] = json!(effort);
        }
        write_json(&file, &entry)
    }

    pub(crate) fn selected_outcome(
        &self,
        session: &str,
        id: &str,
    ) -> Result<Option<Value>, Refusal> {
        match fs::read(self.selected().prompt(session, id)?) {
            Ok(data) => serde_json::from_slice(&data)
                .map(Some)
                .map_err(|e| Refusal::Protocol(e.to_string())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub(super) fn selected_state(
        &self,
        session: &str,
        id: &str,
        now: SystemTime,
    ) -> Result<Option<(PromptState, Option<String>)>, Refusal> {
        self.selected().state(session, id, now)
    }
}
