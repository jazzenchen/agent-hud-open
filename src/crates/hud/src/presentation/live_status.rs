//! The display preferences a snapshot takes at the host boundary: each client's live
//! status and completion reminders, as the person chose them or by default.
use super::*;

/// Where a client's preferences list the switches the person turned in Settings.
/// A value without its key here is not theirs: an earlier build saved one for
/// every client it saw.
const CHOSEN: &str = "chosen";
const SWITCHES: [&str; 2] = ["live", "reminders"];

fn chosen(prefs: &Map<String, Value>, key: &str) -> bool {
    prefs
        .get(CHOSEN)
        .and_then(Value::as_array)
        .is_some_and(|keys| keys.iter().any(|k| k == key))
}

/// Records that the person set these switches of a client themselves.
pub(super) fn choose<'a>(prefs: &mut Map<String, Value>, keys: impl IntoIterator<Item = &'a str>) {
    let set: Vec<&str> = keys.into_iter().collect();
    let list: Vec<Value> = SWITCHES
        .iter()
        .filter(|key| set.contains(key) || chosen(prefs, key))
        .map(|key| json!(key))
        .collect();
    if !list.is_empty() {
        prefs.insert(CHOSEN.into(), Value::Array(list));
    }
}

/// Earlier builds, and the Swift app before them, saved a live status for every
/// client the first time they saw it (on only if it was found on this Mac) and
/// kept it, so a client installed later stayed off; the Swift app kept
/// completion reminders on that same switch. A live status not marked chosen is
/// forgotten, so it follows detection until the person sets it; one a person
/// set in those builds cannot be told apart and goes too. A reminders value that
/// only repeats it goes with it; one of its own was set in Settings and is kept
/// as chosen.
pub(super) fn forget_defaults(sources: &mut Map<String, Value>) {
    for prefs in sources.values_mut() {
        let Some(prefs) = prefs.as_object_mut() else {
            continue;
        };
        let live = if chosen(prefs, "live") {
            None
        } else {
            prefs.remove("live")
        };
        if !chosen(prefs, "reminders") {
            match prefs.get("reminders").cloned() {
                Some(reminders) if live.as_ref() == Some(&reminders) => {
                    prefs.remove("reminders");
                }
                Some(reminders) if reminders.is_boolean() => choose(prefs, ["reminders"]),
                _ => {}
            }
        }
    }
    sources.retain(|_, prefs| prefs.as_object().is_some_and(|p| !p.is_empty()));
}

impl Presentation {
    /// Apply display preferences once at the host boundary, before any window or product relay sees data.
    pub fn project_snapshot(&self, snapshot: &mut Value) {
        let preferences = {
            let s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            Self::preferences(&s.saved.sources, snapshot)
        };
        if preferences.values().any(|p| p["live"] == false) {
            for session in snapshot["sessions"].as_array_mut().into_iter().flatten() {
                let source = session["source"]
                    .as_str()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if preferences.get(&source).is_some_and(|p| p["live"] == false) {
                    session["state"] = json!("idle");
                    if let Some(object) = session.as_object_mut() {
                        object.remove("liveFacts");
                    }
                }
            }
        }
        snapshot["sourcePreferences"] = Value::Object(preferences);
    }

    /// Each client's switches as they stand: what the person chose, otherwise
    /// live status on only while the client is found on this Mac (Open's
    /// Settings.applyLiveStatusDefaults). Nothing is saved for a default, so a
    /// client found later turns on by itself. The demo finds nothing and shows
    /// every client live.
    fn preferences(saved: &Map<String, Value>, snapshot: &Value) -> Map<String, Value> {
        let mut preferences = Map::new();
        for (id, prefs) in saved {
            let mut prefs = prefs.as_object().cloned().unwrap_or_default();
            if !chosen(&prefs, "live") {
                prefs.remove("live");
            }
            prefs.remove(CHOSEN);
            preferences.insert(id.clone(), Value::Object(prefs));
        }
        if snapshot["mode"] == "demo" {
            return preferences;
        }
        for provider in snapshot["providers"].as_array().into_iter().flatten() {
            let Some(id) = provider["id"].as_str().map(str::to_ascii_lowercase) else {
                continue;
            };
            if let Some(prefs) = preferences
                .entry(id)
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .filter(|prefs| !prefs.contains_key("live"))
            {
                prefs.insert("live".into(), json!(provider["detected"] == true));
            }
        }
        preferences
    }
}
