//! The display preferences a snapshot takes at the host boundary: each client's live
//! status, and its saved default.
use super::*;

impl Presentation {
    /// Apply display preferences once at the host boundary, before any window or product relay sees data.
    pub fn project_snapshot(&self, snapshot: &mut Value) {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        self.apply_live_status_defaults(&mut s, snapshot);
        snapshot["sourcePreferences"] = Value::Object(s.saved.sources.clone());
        if let Some(sessions) = snapshot["sessions"]
            .as_array_mut()
            .filter(|_| s.saved.sources.values().any(|p| p["live"] == false))
        {
            for session in sessions {
                let source = session["source"]
                    .as_str()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if s.saved
                    .sources
                    .get(&source)
                    .is_some_and(|p| p["live"] == false)
                {
                    session["state"] = json!("idle");
                    if let Some(object) = session.as_object_mut() {
                        object.remove("liveFacts");
                    }
                }
            }
        }
    }
    /// Open's Settings.applyLiveStatusDefaults: a client with no saved choice
    /// starts with live status on only if it is detected on this Mac, and the
    /// choice is saved, so a later detection never overrides it. Detection runs
    /// before the first snapshot; the demo detects nothing and is left alone.
    /// Written without touching the layouts' previous version.
    fn apply_live_status_defaults(&self, s: &mut State, snapshot: &Value) {
        if snapshot["mode"] == "demo" {
            return;
        }
        let mut added = false;
        for provider in snapshot["providers"].as_array().into_iter().flatten() {
            let Some(id) = provider["id"].as_str().map(str::to_ascii_lowercase) else {
                continue;
            };
            let prefs = s.saved.sources.entry(id).or_insert_with(|| json!({}));
            if prefs.get("live").is_none() {
                if let Some(object) = prefs.as_object_mut() {
                    object.insert("live".into(), json!(provider["detected"] == true));
                    added = true;
                }
            }
        }
        if !added {
            return;
        }
        let written = self
            .path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| {
                let bytes = serde_json::to_vec_pretty(&s.saved).map_err(std::io::Error::other)?;
                let temporary = self.path.with_extension("json.tmp");
                fs::write(&temporary, bytes).and_then(|()| fs::rename(temporary, &self.path))
            });
        match written {
            Ok(()) => s.revision = s.revision.wrapping_add(1),
            Err(error) => eprintln!("Live status defaults could not be saved: {error}"),
        }
    }
}
