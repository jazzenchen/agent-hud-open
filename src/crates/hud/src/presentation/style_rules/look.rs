//! The look a screen has saved, a style draft drawn over it, and the look as get_hud
//! lists it.
use super::*;

/// The keys of a screen's settings that make up its look: every display setting but
/// its hardware, what it shows at all, the app's own settings and the panel's.
fn look_key(key: &str) -> bool {
    !HARDWARE.contains(&key)
        && !PRESENCE.contains(&key)
        && !GLOBAL.contains(&key)
        && !super::panel_keys::panel_key(key)
}

/// The look a screen draws from `saved` (ui/use-hud-config.ts `getScreenHUDConfig`):
/// the defaults, the notch mode on a screen with a notch and Simple on one without,
/// the shared glow, then the screen's own settings. A screen saved in the retired
/// Dynamic mode draws as Simple.
pub(super) fn drawn(saved: &Saved, key: &str, has_notch: bool) -> Map<String, Value> {
    let mut style = DEFAULTS.clone();
    style.insert(
        "mode".into(),
        json!(if has_notch { "notch" } else { "simple" }),
    );
    merge(&mut style, &saved.screen_defaults);
    let own = saved.screens.get(key).cloned().unwrap_or(Value::Null);
    if let Some(own) = own.as_object() {
        merge(&mut style, own);
    }
    if let Some(mode) = style.get("mode").and_then(Value::as_str) {
        let mode = drawn_mode(mode).to_owned();
        style.insert("mode".into(), json!(mode));
    }
    // A screen saved before the Simple style existed keeps what its rings switch drew.
    let screen_defaults = Value::Object(saved.screen_defaults.clone());
    style.insert(
        "simpleStyle".into(),
        json!(super::simple_style([&own, &screen_defaults])),
    );
    style.remove("showsRings");
    style.retain(|key, _| look_key(key));
    style
}

/// The look a screen has saved, with its style draft drawn over it.
pub(in crate::presentation) fn effective(
    s: &State,
    key: &str,
    has_notch: bool,
) -> Map<String, Value> {
    let mut style = drawn(&s.saved, key, has_notch);
    if let Some(draft) = s.styles.get(key).and_then(Value::as_object) {
        merge(&mut style, draft);
    }
    style
}

/// The default look a screen is given, as the HUD style's Reset to Default gives it.
pub(super) fn default_look(s: &State, has_notch: bool) -> Map<String, Value> {
    let mut style = DEFAULTS.clone();
    merge(&mut style, &s.saved.screen_defaults);
    let screen_defaults = Value::Object(s.saved.screen_defaults.clone());
    style.insert(
        "simpleStyle".into(),
        json!(super::simple_style([&screen_defaults])),
    );
    style.remove("showsRings");
    style.retain(|key, _| look_key(key));
    style.insert(
        "mode".into(),
        json!(if has_notch { "notch" } else { "simple" }),
    );
    style
}

/// `patch` without what repeats `saved`.
pub(super) fn reduced(
    patch: &Map<String, Value>,
    saved: &Map<String, Value>,
) -> Map<String, Value> {
    patch
        .iter()
        .filter(|(k, v)| !saved.get(*k).is_some_and(|old| same(old, v)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// A screen's look in `version` that differs from what it has saved now: the style
/// draft that brings that version back.
pub(in crate::presentation) fn restoring(
    version: &Saved,
    s: &State,
    key: &str,
    has_notch: bool,
) -> Map<String, Value> {
    reduced(
        &drawn(version, key, has_notch),
        &drawn(&s.saved, key, has_notch),
    )
}

/// The settings a hud's style fields write.
pub(super) fn patch(hud: &Value) -> Map<String, Value> {
    let mut patch = Map::new();
    for field in FIELDS {
        let Some(value) = hud.get(field.name).filter(|v| !v.is_null()) else {
            continue;
        };
        patch.insert(field.stored.into(), value.clone());
    }
    for field in GLOW {
        if let Some(value) = hud["glow"].get(field.name).filter(|v| !v.is_null()) {
            patch.insert(field.stored.into(), value.clone());
        }
    }
    patch
}

/// A screen's look in an agent's words, as get_hud lists it under `hud`.
pub(in crate::presentation) fn listing(style: &Map<String, Value>) -> Value {
    let mut hud = Map::new();
    for field in FIELDS {
        let value = style.get(field.stored).cloned().unwrap_or(Value::Null);
        hud.insert(field.name.into(), value);
    }
    let mut glow = Map::new();
    for field in GLOW {
        glow.insert(
            field.name.into(),
            style.get(field.stored).cloned().unwrap_or(Value::Null),
        );
    }
    hud.insert("glow".into(), Value::Object(glow));
    Value::Object(hud)
}
