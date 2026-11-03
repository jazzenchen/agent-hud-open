//! One durable presentation owner for all windows and MCP. Drafts and edit holds
//! are memory-only; a tool cannot turn its proposal into the user's saved choice.
mod app_rules;
mod rules;
mod simple_style;
mod style_rules;
mod tools;
pub use rules::{mark, resting_agents};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
pub use simple_style::{simple_style, SIMPLE_STYLES};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
const DEFAULTS: &str = include_str!("../../../packages/protocol/hud-defaults.json");
const GLOBAL: &[&str] = &[
    "botLogos",
    "hoverDelayMs",
    "collapseDelayMs",
    "requiresOptionToOpen",
    "openImmediatelyAtTop",
    "animations",
];
const HARDWARE: &[&str] = &["monitorIndex", "hasNotch", "notchWidth", "notchHeight"];
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Saved {
    version: u8,
    global: Map<String, Value>,
    screens: Map<String, Value>,
    screen_defaults: Map<String, Value>,
    shared_layout: Value,
    layouts: Map<String, Value>,
    sources: Map<String, Value>,
}
impl Default for Saved {
    fn default() -> Self {
        Self {
            version: 3,
            global: Map::new(),
            screens: Map::new(),
            screen_defaults: Map::new(),
            shared_layout: json!({}),
            layouts: Map::new(),
            sources: Map::new(),
        }
    }
}
#[derive(Clone)]
struct State {
    saved: Saved,
    previous: Option<Saved>,
    initialized: bool,
    revision: u64,
    layouts: HashMap<String, Value>,
    styles: HashMap<String, Value>,
    app: Option<Value>,
    holds: HashMap<String, i64>,
    screens: Vec<Value>,
    /// The menu bar's Hide notch glow, for every screen until the app quits
    /// (Open's `UsageStore.glowHidden`).
    glow_hidden: bool,
}
pub struct Presentation {
    path: PathBuf,
    state: Mutex<State>,
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{label} must be an object"))
}
fn merge(target: &mut Map<String, Value>, patch: &Map<String, Value>) {
    for (k, v) in patch {
        target.insert(k.clone(), v.clone());
    }
}
/// A direct Settings edit supersedes only that setting of an app draft.
fn clear_app_field(s: &mut State, key: &str) -> bool {
    let field = match key {
        "appearance" => "appearance",
        "appearanceFollowsScreen" => "matchScreen",
        "showMenuBarIcon" => "showMenuBarIcon",
        "botLogos" => "botLogos",
        "hoverDelayMs" => "hover:delayMs",
        "collapseDelayMs" => "hover:collapseDelayMs",
        "requiresOptionToOpen" => "hover:pressOptionToOpen",
        "openImmediatelyAtTop" => "hover:openAtTopEdge",
        _ => return false,
    };
    let Some(draft) = s.app.as_mut().and_then(Value::as_object_mut) else {
        return false;
    };
    let changed = if let Some(field) = field.strip_prefix("hover:") {
        if let Some(hover) = draft.get_mut("hover").and_then(Value::as_object_mut) {
            let removed = hover.remove(field).is_some();
            if hover.is_empty() {
                draft.remove("hover");
            }
            removed
        } else {
            false
        }
    } else {
        draft.remove(field).is_some()
    };
    if draft.is_empty() {
        s.app = None;
    }
    changed
}
impl Presentation {
    pub fn new(directory: PathBuf) -> Result<Self, String> {
        let path = directory.join("presentation.json");
        let initialized = path.exists();
        let saved = if initialized {
            serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Display preferences: {e}"))?
        } else {
            Saved::default()
        };
        Ok(Self {
            path,
            state: Mutex::new(State {
                saved,
                previous: None,
                initialized,
                revision: 0,
                layouts: HashMap::new(),
                styles: HashMap::new(),
                app: None,
                holds: HashMap::new(),
                screens: vec![],
                glow_hidden: false,
            }),
        })
    }
    fn persist(&self, state: &mut State, before: Saved) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(&state.saved).map_err(|e| e.to_string())?;
        let temporary = self.path.with_extension("json.tmp");
        if let Err(e) = fs::write(&temporary, bytes).and_then(|_| fs::rename(temporary, &self.path))
        {
            state.saved = before;
            return Err(e.to_string());
        }
        state.previous = Some(before);
        state.initialized = true;
        state.revision = state.revision.wrapping_add(1);
        Ok(())
    }
    /// One-time, read-only legacy import. Existing native preferences always win.
    pub fn seed_if_absent(&self, legacy: &Value) -> Result<(), String> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| "Display preference owner unavailable")?;
        if s.initialized {
            return Ok(());
        }
        let Some(fields) = legacy.as_object() else {
            return Ok(());
        };
        let checkpoint = s.clone();
        let before = s.saved.clone();
        let object = |key: &str| {
            fields
                .get(key)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default()
        };
        let mut seeded = Saved {
            global: object("global"),
            screen_defaults: object("screenDefaults"),
            screens: object("screens"),
            ..Saved::default()
        };
        seeded
            .global
            .retain(|key, _| GLOBAL.contains(&key.as_str()));
        seeded.screen_defaults.retain(|key, _| {
            key.starts_with("glow")
                || ["breathSeconds", "idleBreathSeconds", "breathAmplitude"].contains(&key.as_str())
        });
        let defaults: Value = serde_json::from_str(DEFAULTS).expect("checked-in defaults");
        for value in seeded.screens.values_mut() {
            if let Some(settings) = value.as_object_mut() {
                settings.retain(|key, _| {
                    defaults.get(key).is_some()
                        && !HARDWARE.contains(&key.as_str())
                        && !GLOBAL.contains(&key.as_str())
                        && !tools::panel_key(key)
                });
            } else {
                *value = json!({});
            }
        }
        seeded.shared_layout =
            rules::normalize_layout(fields.get("sharedLayout").unwrap_or(&json!({})))?;
        if let Some(layouts) = fields.get("layouts").and_then(Value::as_object) {
            for (id, layout) in layouts {
                seeded
                    .layouts
                    .insert(id.clone(), rules::normalize_layout(layout)?);
            }
        }
        if let Some(sources) = fields.get("sources").and_then(Value::as_object) {
            for (id, prefs) in sources {
                let mut values = Map::new();
                for key in ["live", "reminders"] {
                    if let Some(value) = prefs.get(key).filter(|v| v.is_boolean()) {
                        values.insert(key.into(), value.clone());
                    }
                }
                if !values.is_empty() {
                    seeded.sources.insert(id.clone(), Value::Object(values));
                }
            }
        }
        s.saved = seeded;
        if let Err(error) = self.persist(&mut s, before) {
            *s = checkpoint;
            return Err(error);
        }
        Ok(())
    }
    /// Bumped by every change to the display preferences, the sources'
    /// included, so a host knows when its projection of a snapshot is stale.
    pub fn revision(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .revision
    }

    pub fn view(&self) -> Value {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        Self::view_of(&mut state)
    }
    fn view_of(s: &mut State) -> Value {
        s.holds.retain(|_, until| *until > now());
        let mut screens = s.saved.screens.clone();
        for (id, patch) in &s.styles {
            merge(
                screens
                    .entry(id.clone())
                    .or_insert(json!({}))
                    .as_object_mut()
                    .unwrap(),
                patch.as_object().unwrap(),
            );
        }
        let shared = s.layouts.get("all").unwrap_or(&s.saved.shared_layout);
        let mut layouts = s.saved.layouts.clone();
        for (id, value) in &s.layouts {
            if id != "all" {
                if value.is_null() {
                    layouts.remove(id);
                } else {
                    layouts.insert(id.clone(), value.clone());
                }
            }
        }
        json!({"revision":s.revision,"initialized":s.initialized,"glowHidden":s.glow_hidden,"store":{"version":3,"global":s.saved.global,"screens":screens,"screenDefaults":s.saved.screen_defaults},"sharedLayout":shared,"layouts":layouts,"sources":s.saved.sources,"appDraft":s.app,"drafts":{"layouts":s.layouts.keys().collect::<Vec<_>>(),"styles":s.styles.keys().collect::<Vec<_>>(),"app":s.app.is_some()},"holds":s.holds,"screens":s.screens})
    }
    /// Apply display preferences once at the host boundary, before any window or product relay sees data.
    pub fn project_snapshot(&self, snapshot: &mut Value) {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
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
    /// Shows or hides the notch glow.
    pub fn toggle_glow(&self) -> Value {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        s.glow_hidden = !s.glow_hidden;
        s.revision = s.revision.wrapping_add(1);
        Self::view_of(&mut s)
    }
    pub fn manual_app_edit(&self, key: &str) -> Option<Value> {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if !clear_app_field(&mut s, key) {
            return None;
        }
        s.revision = s.revision.wrapping_add(1);
        Some(Self::view_of(&mut s))
    }
    pub fn screens(&self, screens: Vec<Value>) {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if s.screens != screens {
            s.screens = screens;
            s.revision = s.revision.wrapping_add(1);
        }
    }
    pub fn update(&self, params: Value) -> Result<Value, String> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| "Display preference owner unavailable")?;
        let checkpoint = s.clone();
        let before = s.saved.clone();
        let id = params["screen"].as_str().unwrap_or("all");
        let patch = object(&params["patch"], "patch")?;
        if patch.keys().any(|key| tools::panel_key(key)) {
            let mut layout = if id == "all" {
                s.saved.shared_layout.clone()
            } else {
                s.saved
                    .layouts
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| s.saved.shared_layout.clone())
            };
            tools::panel_patch(&mut layout, patch);
            if id == "all" {
                s.saved.shared_layout = layout;
            } else {
                s.saved.layouts.insert(id.into(), layout);
            }
            s.layouts.remove(id);
        }
        for (k, v) in patch {
            clear_app_field(&mut s, k);
            if HARDWARE.contains(&k.as_str()) || tools::panel_key(k) {
                continue;
            }
            if GLOBAL.contains(&k.as_str()) {
                s.saved.global.insert(k.clone(), v.clone());
            } else if id != "all" {
                s.saved
                    .screens
                    .entry(id)
                    .or_insert(json!({}))
                    .as_object_mut()
                    .unwrap()
                    .insert(k.clone(), v.clone());
            }
        }
        if id != "all" {
            s.styles.remove(id);
            s.holds.remove(id);
        }
        if let Err(error) = self.persist(&mut s, before) {
            *s = checkpoint;
            return Err(error);
        }
        Ok(Self::view_of(&mut s))
    }
    pub fn import(&self, params: Value) -> Result<Value, String> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| "Display preference owner unavailable")?;
        if !s.initialized {
            if let Some(store) = params.get("store").filter(|v| v["version"] == 3) {
                let checkpoint = s.clone();
                let before = s.saved.clone();
                s.saved.global = store["global"].as_object().cloned().unwrap_or_default();
                s.saved.screens = store["screens"].as_object().cloned().unwrap_or_default();
                for (id, settings) in s.saved.screens.clone() {
                    if let Some(fields) = settings.as_object() {
                        if fields.keys().any(|key| tools::panel_key(key)) {
                            let mut layout = json!({});
                            tools::panel_patch(&mut layout, fields);
                            s.saved.layouts.insert(id.clone(), layout);
                        }
                        if let Some(settings) =
                            s.saved.screens.get_mut(&id).and_then(Value::as_object_mut)
                        {
                            settings.retain(|key, _| !tools::panel_key(key));
                        }
                    }
                }
                if let Err(error) = self.persist(&mut s, before) {
                    *s = checkpoint;
                    return Err(error);
                }
            }
        }
        Ok(Self::view_of(&mut s))
    }
    pub fn source_update(&self, params: Value) -> Result<Value, String> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| "Display preference owner unavailable")?;
        let checkpoint = s.clone();
        let before = s.saved.clone();
        let source = params["source"].as_str().ok_or("Source is required")?;
        let patch = object(&params["patch"], "patch")?;
        let target = s
            .saved
            .sources
            .entry(source)
            .or_insert(json!({}))
            .as_object_mut()
            .unwrap();
        merge(target, patch);
        if let Err(error) = self.persist(&mut s, before) {
            *s = checkpoint;
            return Err(error);
        }
        Ok(Self::view_of(&mut s))
    }
    pub fn set_layout(&self, params: Value) -> Result<Value, String> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| "Display preference owner unavailable")?;
        let checkpoint = s.clone();
        let before = s.saved.clone();
        let screen = params["screen"].as_str().unwrap_or("all");
        let layout = rules::normalize_layout(params.get("layout").unwrap_or(&json!({})))?;
        if screen == "all" {
            s.saved.shared_layout = layout;
        } else {
            s.saved.layouts.insert(screen.into(), layout);
        }
        s.layouts.remove(screen);
        s.holds.remove(screen);
        if let Err(error) = self.persist(&mut s, before) {
            *s = checkpoint;
            return Err(error);
        }
        Ok(Self::view_of(&mut s))
    }
    pub fn save_panel_with(
        &self,
        screen: &str,
        commit: impl FnOnce(&Value) -> Result<(), String>,
    ) -> Result<Value, String> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| "Display preference owner unavailable")?;
        let checkpoint = s.clone();
        let before = s.saved.clone();
        let patch = s.app.as_ref().map(tools::app_settings).unwrap_or(json!({}));
        commit(&patch)?;
        let scope = if s.layouts.contains_key(screen) {
            screen
        } else {
            "all"
        };
        if let Some(layout) = s.layouts.remove(scope) {
            if scope == "all" {
                s.saved.shared_layout = layout;
            } else if layout.is_null() {
                s.saved.layouts.remove(scope);
            } else {
                s.saved.layouts.insert(scope.into(), layout);
            }
        }
        if let Some(style) = s.styles.remove(screen) {
            merge(
                s.saved
                    .screens
                    .entry(screen)
                    .or_insert(json!({}))
                    .as_object_mut()
                    .unwrap(),
                style.as_object().unwrap(),
            );
        }
        if let Some(app) = s.app.take() {
            tools::save_global_app(&mut s.saved.global, &app);
        }
        s.holds.remove(screen);
        if let Err(error) = self.persist(&mut s, before) {
            *s = checkpoint;
            return Err(error);
        }
        Ok(Self::view_of(&mut s))
    }
    pub fn discard_panel(&self, screen: &str) -> Value {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let scope = if s.layouts.contains_key(screen) {
            screen
        } else {
            "all"
        };
        s.layouts.remove(scope);
        s.styles.remove(screen);
        s.app = None;
        s.holds.remove(screen);
        s.revision = s.revision.wrapping_add(1);
        Self::view_of(&mut s)
    }
    pub fn release_hold(&self, screen: &str) -> Value {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        s.holds.remove(screen);
        Self::view_of(&mut s)
    }
    /// One MCP tool call. `catalog` is the token chart's statistics answer, whose
    /// models and agents a proposal may name, or `{"error": …}` where it could not be
    /// read; nil leaves them unchecked.
    pub fn tool(
        &self,
        name: &str,
        params: Value,
        snapshot: &Value,
        catalog: Option<&Value>,
    ) -> Result<Value, String> {
        tools::call(self, name, params, snapshot, catalog)
    }
}

#[cfg(test)]
mod tool_tests;

#[cfg(test)]
mod live_status_tests {
    use super::Presentation;
    use serde_json::json;

    #[test]
    fn a_client_starts_live_only_when_detected_and_keeps_that_choice() {
        let folder = tempfile::tempdir().unwrap();
        let owner = Presentation::new(folder.path().to_path_buf()).unwrap();
        let snapshot = |codex: bool| json!({"mode": "live", "providers": [{"id": "claude", "detected": true}, {"id": "codex", "detected": codex}], "sessions": []});
        let mut first = snapshot(false);
        owner.project_snapshot(&mut first);
        assert_eq!(first["sourcePreferences"]["claude"]["live"], true);
        assert_eq!(first["sourcePreferences"]["codex"]["live"], false);
        // Codex installed later: the saved choice stays, across a restart too.
        let reopened = Presentation::new(folder.path().to_path_buf()).unwrap();
        let mut later = snapshot(true);
        reopened.project_snapshot(&mut later);
        assert_eq!(later["sourcePreferences"]["codex"]["live"], false);
        // The demo detects nothing and changes nothing.
        let demo = Presentation::new(tempfile::tempdir().unwrap().path().to_path_buf()).unwrap();
        let mut sample = json!({"mode": "demo", "providers": [{"id": "codex", "detected": false}], "sessions": []});
        demo.project_snapshot(&mut sample);
        assert!(sample["sourcePreferences"].get("codex").is_none());
    }
}
