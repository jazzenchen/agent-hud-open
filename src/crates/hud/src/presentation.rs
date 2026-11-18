//! One durable presentation owner for all windows and MCP. Drafts and edit holds
//! are memory-only; a tool cannot turn its proposal into the user's saved choice.
mod app_rules;
mod import;
mod live_status;
mod panel_keys;
mod rules;
mod settings;
mod simple_style;
mod style_rules;
mod tools;
mod tree;
pub use rules::{mark, resting_agents, QUEUE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
pub use simple_style::{simple_style, SIMPLE_STYLES};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{LazyLock, Mutex, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};
static DEFAULTS: LazyLock<Map<String, Value>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../packages/protocol/hud-defaults.json"))
        .expect("checked-in defaults")
});
const GLOBAL: &[&str] = &[
    "botLogos",
    "hoverDelayMs",
    "collapseDelayMs",
    "messageFadeSeconds",
    "requiresOptionToOpen",
    "openImmediatelyAtTop",
    "animations",
];
const HARDWARE: &[&str] = &["monitorIndex", "hasNotch", "notchWidth", "notchHeight"];
/// What a screen shows at all: only the person sets these, in Settings; they
/// are no part of the look a tool reads, drafts or restores.
const PRESENCE: &[&str] = &[
    "hidden",
    "hidesMessages",
    "hidesRequests",
    "hidesFlights",
    "autoHideAtEdge",
];
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
    let Some((field, ..)) = app_rules::SAVED_AS
        .iter()
        .find(|(_, saved, _)| *saved == key)
    else {
        return false;
    };
    let Some(draft) = s.app.as_mut().and_then(Value::as_object_mut) else {
        return false;
    };
    let changed = if let Some(field) = field.strip_prefix("hover.") {
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
        draft.remove(*field).is_some()
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
        let mut saved: Saved = if initialized {
            serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Display preferences: {e}"))?
        } else {
            Saved::default()
        };
        live_status::forget_defaults(&mut saved.sources);
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
        fs::write(&temporary, bytes)
            .and_then(|_| fs::rename(temporary, &self.path))
            .map_err(|e| e.to_string())?;
        state.previous = Some(before);
        state.initialized = true;
        state.revision = state.revision.wrapping_add(1);
        Ok(())
    }

    /// Bumped by every change to the display preferences, the sources'
    /// included, so a host knows when its projection of a snapshot is stale.
    pub fn revision(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .revision
    }

    pub fn view(&self) -> Value {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
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

    /// Shows or hides the notch glow.
    pub fn toggle_glow(&self) -> Value {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        s.glow_hidden = !s.glow_hidden;
        s.revision = s.revision.wrapping_add(1);
        Self::view_of(&mut s)
    }
    pub fn manual_app_edit(&self, key: &str) -> Option<Value> {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if !clear_app_field(&mut s, key) {
            return None;
        }
        s.revision = s.revision.wrapping_add(1);
        Some(Self::view_of(&mut s))
    }
    pub fn screens(&self, screens: Vec<Value>) {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if s.screens != screens {
            s.screens = screens;
            s.revision = s.revision.wrapping_add(1);
        }
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
mod live_status_tests;
