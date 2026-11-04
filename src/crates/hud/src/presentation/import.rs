//! Display preferences carried over once from an earlier build: the legacy store at
//! setup, and a version 3 store a window sends before anything is saved.
use super::*;

impl Presentation {
    /// One-time, read-only legacy import. Existing native preferences always win.
    pub fn seed_if_absent(&self, legacy: &Value) -> Result<(), String> {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
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
        for value in seeded.screens.values_mut() {
            if let Some(settings) = value.as_object_mut() {
                settings.retain(|key, _| {
                    DEFAULTS.contains_key(key)
                        && !HARDWARE.contains(&key.as_str())
                        && !GLOBAL.contains(&key.as_str())
                        && !panel_keys::panel_key(key)
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

    pub fn import(&self, params: Value) -> Result<Value, String> {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if !s.initialized {
            if let Some(store) = params.get("store").filter(|v| v["version"] == 3) {
                let checkpoint = s.clone();
                let before = s.saved.clone();
                s.saved.global = store["global"].as_object().cloned().unwrap_or_default();
                s.saved.screens = store["screens"].as_object().cloned().unwrap_or_default();
                for (id, settings) in s.saved.screens.clone() {
                    if let Some(fields) = settings.as_object() {
                        if fields.keys().any(|key| panel_keys::panel_key(key)) {
                            let mut layout = json!({});
                            panel_keys::panel_patch(&mut layout, fields);
                            s.saved.layouts.insert(id.clone(), layout);
                        }
                        if let Some(settings) =
                            s.saved.screens.get_mut(&id).and_then(Value::as_object_mut)
                        {
                            settings.retain(|key, _| !panel_keys::panel_key(key));
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
}
