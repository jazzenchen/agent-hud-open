//! A screen's display preferences as its HUD draws them, over the checked-in
//! defaults; the coordinator reads the primary screen's for the menu bar.
use super::Mode;
use serde_json::{json, Value};
use std::sync::OnceLock;

fn defaults() -> &'static Value {
    static DEFAULTS: OnceLock<Value> = OnceLock::new();
    DEFAULTS.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../packages/protocol/hud-defaults.json"
        ))
        .expect("checked-in HUD defaults")
    })
}

/// A screen's display preferences as its HUD draws them (use-hud-config.ts
/// `getScreenHUDConfig`): the screen's own, else the screens' defaults, else the
/// app's; a value of the wrong type is no value.
pub(crate) struct Screen<'a> {
    view: &'a Value,
    id: &'a str,
    has_notch: bool,
}

impl<'a> Screen<'a> {
    /// `view` is the presentation owner's view, drafts included.
    pub(crate) fn new(view: &'a Value, id: &'a str, has_notch: bool) -> Self {
        Self {
            view,
            id,
            has_notch,
        }
    }

    pub(super) fn value(&self, key: &str) -> Value {
        let fallback = &defaults()[key];
        let fits = |value: &&Value| match fallback {
            Value::Number(_) => value.as_f64().is_some_and(f64::is_finite),
            Value::Bool(_) => value.is_boolean(),
            Value::String(_) => value.is_string(),
            _ => false,
        };
        let store = &self.view["store"];
        [
            &store["screens"][self.id][key],
            &store["screenDefaults"][key],
        ]
        .into_iter()
        .find(fits)
        .cloned()
        .unwrap_or_else(|| match key {
            "mode" => json!(if self.has_notch { "notch" } else { "simple" }),
            _ => fallback.clone(),
        })
    }

    pub(super) fn number(&self, key: &str) -> f64 {
        self.value(key).as_f64().unwrap_or(0.0)
    }

    pub(super) fn flag(&self, key: &str) -> bool {
        self.value(key).as_bool().unwrap_or(false)
    }

    pub(crate) fn text(&self, key: &str) -> String {
        self.value(key).as_str().unwrap_or("").to_owned()
    }

    /// How the screen's Simple marks draw usage, read as Settings reads it.
    pub(super) fn simple_style(&self) -> &'static str {
        let store = &self.view["store"];
        agent_hud_hud::presentation::simple_style([
            &store["screens"][self.id],
            &store["screenDefaults"],
        ])
    }

    pub(crate) fn mode(&self) -> Mode {
        serde_json::from_value(self.value("mode")).unwrap_or(if self.has_notch {
            Mode::Notch
        } else {
            Mode::Simple
        })
    }

    pub(crate) fn mode_name(&self) -> &'static str {
        match self.mode() {
            Mode::Notch => "notch",
            Mode::Simple => "simple",
        }
    }

    /// The panel layout the screen draws: its own, else the shared one.
    pub(crate) fn layout(&self) -> &'a Value {
        match &self.view["layouts"][self.id] {
            Value::Null => &self.view["sharedLayout"],
            layout => layout,
        }
    }
}
