//! What Settings and the panel write: a screen's settings, a source's, a layout, and
//! the panel's Save, Discard and hold.
use super::*;

impl Presentation {
    pub fn update(&self, params: Value) -> Result<Value, String> {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let checkpoint = s.clone();
        let before = s.saved.clone();
        let id = params["screen"].as_str().unwrap_or("all");
        let patch = object(&params["patch"], "patch")?;
        if patch.keys().any(|key| panel_keys::panel_key(key)) {
            let mut layout = if id == "all" {
                s.saved.shared_layout.clone()
            } else {
                s.saved
                    .layouts
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| s.saved.shared_layout.clone())
            };
            panel_keys::panel_patch(&mut layout, patch);
            if id == "all" {
                s.saved.shared_layout = layout;
            } else {
                s.saved.layouts.insert(id.into(), layout);
            }
            s.layouts.remove(id);
        }
        for (k, v) in patch {
            clear_app_field(&mut s, k);
            if HARDWARE.contains(&k.as_str()) || panel_keys::panel_key(k) {
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

    pub fn source_update(&self, params: Value) -> Result<Value, String> {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
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
        live_status::choose(target, patch.keys().map(String::as_str));
        if let Err(error) = self.persist(&mut s, before) {
            *s = checkpoint;
            return Err(error);
        }
        Ok(Self::view_of(&mut s))
    }
    pub fn set_layout(&self, params: Value) -> Result<Value, String> {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
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
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let checkpoint = s.clone();
        let before = s.saved.clone();
        let patch = s
            .app
            .as_ref()
            .map(app_rules::app_settings)
            .unwrap_or(json!({}));
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
            app_rules::save_global_app(&mut s.saved.global, &app);
        }
        s.holds.remove(screen);
        if let Err(error) = self.persist(&mut s, before) {
            *s = checkpoint;
            return Err(error);
        }
        Ok(Self::view_of(&mut s))
    }
    pub fn discard_panel(&self, screen: &str) -> Value {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
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
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        s.holds.remove(screen);
        Self::view_of(&mut s)
    }
}
