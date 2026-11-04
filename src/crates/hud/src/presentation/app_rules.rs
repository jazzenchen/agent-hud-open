//! The app's own settings an agent may propose, as agent-hud-native-open checks and reduces
//! them (AppDraft.swift, HUDTools+App.swift): what reading would forgive, what the
//! app would not draw as asked, and what a proposal changes against what is saved.
use super::rules::{listed, quoted, same, whole};
use super::*;

const APPEARANCES: &[&str] = &["system", "dark", "light"];
/// The fields under `app`, beside `hover`.
const FIELDS: &[&str] = &["appearance", "matchScreen", "botLogos", "showMenuBarIcon"];
/// Hover behavior's fields, under `app.hover`.
const HOVER: &[&str] = &[
    "pressOptionToOpen",
    "openAtTopEdge",
    "delayMs",
    "collapseDelayMs",
];
const READ_ONLY: &[&str] = &["language", "writtenIn"];
/// Every field of an app draft, `hover.` before one under `app.hover`, with the
/// setting it is saved as, and whether the display preferences' `global` keeps that
/// setting rather than the app's settings.
pub(super) const SAVED_AS: &[(&str, &str, bool)] = &[
    ("appearance", "appearance", false),
    ("matchScreen", "appearanceFollowsScreen", false),
    ("showMenuBarIcon", "showMenuBarIcon", false),
    ("botLogos", "botLogos", true),
    ("hover.delayMs", "hoverDelayMs", true),
    ("hover.collapseDelayMs", "collapseDelayMs", true),
    ("hover.pressOptionToOpen", "requiresOptionToOpen", true),
    ("hover.openAtTopEdge", "openImmediatelyAtTop", true),
];
/// The milliseconds Settings sets a delay in, and the step its slider moves by.
const DELAY_RANGE: (i64, i64) = (0, 1500);
const DELAY_STEP: i64 = 50;

/// The app's settings as they are saved, in an agent's words.
pub(super) fn saved(s: &State, snapshot: &Value) -> Value {
    let settings = &snapshot["settings"];
    let global = &s.saved.global;
    let flag =
        |key: &str, default: bool| global.get(key).and_then(Value::as_bool).unwrap_or(default);
    let delay =
        |key: &str, default: u64| global.get(key).and_then(Value::as_u64).unwrap_or(default);
    json!({
        "appearance": settings["appearance"],
        "matchScreen": settings["appearanceFollowsScreen"].as_bool().unwrap_or(false),
        "botLogos": flag("botLogos", true),
        "showMenuBarIcon": settings["showMenuBarIcon"].as_bool().unwrap_or(true),
        "hover": {
            "delayMs": delay("hoverDelayMs", 400),
            "collapseDelayMs": delay("collapseDelayMs", 200),
            "pressOptionToOpen": flag("requiresOptionToOpen", false),
            "openAtTopEdge": flag("openImmediatelyAtTop", false),
        }
    })
}

pub(super) fn app_settings(app: &Value) -> Value {
    let mut v = Map::new();
    for (field, saved, _) in SAVED_AS.iter().filter(|(.., global)| !global) {
        if let Some(value) = app.get(*field) {
            v.insert((*saved).into(), value.clone());
        }
    }
    Value::Object(v)
}

pub(super) fn save_global_app(global: &mut Map<String, Value>, app: &Value) {
    for (field, saved, _) in SAVED_AS.iter().filter(|(.., global)| *global) {
        let value = match field.strip_prefix("hover.") {
            Some(field) => app["hover"].get(field),
            None => app.get(*field),
        };
        if let Some(value) = value {
            global.insert((*saved).into(), value.clone());
        }
    }
}

/// The app part of a proposal (HUDTools.AppProposal). A call that names no app leaves
/// the app's draft as it is; one that names it is whole, so a field it leaves out is
/// what is saved, and a field set to what is saved changes nothing and is no issue.
pub(super) struct AppProposal {
    pub given: bool,
    /// What it changes about what is saved; nil where that is nothing.
    pub reduced: Option<Value>,
    pub reasons: Vec<String>,
}

fn path(field: &str) -> String {
    if HOVER.contains(&field) {
        format!("app.hover.{field}")
    } else {
        format!("app.{field}")
    }
}

/// A field's value as a draft keeps it; nil for a value of another kind.
fn read(field: &str, value: &Value) -> Option<Value> {
    match field {
        "appearance" => value
            .as_str()
            .filter(|s| APPEARANCES.contains(s))
            .map(|s| json!(s)),
        "delayMs" | "collapseDelayMs" => whole(value).map(|n| json!(n)),
        _ => value.as_bool().map(|b| json!(b)),
    }
}

fn field_misreading(field: &str, value: &Value) -> Option<String> {
    if value.is_null() || read(field, value).is_some() {
        return None;
    }
    let path = path(field);
    Some(match field {
        "appearance" => format!("{path} is one of {}.", quoted(APPEARANCES)),
        "delayMs" | "collapseDelayMs" => format!("{path} is a whole number of milliseconds."),
        _ => format!("{path} is true or false."),
    })
}

/// What reading an app as an agent sent it would forgive (AppDraft.misreadings).
fn misreadings(value: &Value) -> Vec<String> {
    let Some(app) = value.as_object() else {
        return vec!["app is an object.".into()];
    };
    let named: Vec<&str> = FIELDS.iter().copied().chain(["hover"]).collect();
    let mut found = vec![];
    for (property, value) in app {
        if property == "hover" {
            if value.is_null() {
                continue;
            }
            let Some(hover) = value.as_object() else {
                found.push("app.hover is an object.".into());
                continue;
            };
            for (property, value) in hover {
                if HOVER.contains(&property.as_str()) {
                    found.extend(field_misreading(property, value));
                } else {
                    found.push(format!(
                        "app.hover has no property \"{property}\"; the hover takes {}.",
                        listed(HOVER)
                    ));
                }
            }
        } else if READ_ONLY.contains(&property.as_str()) {
            continue;
        } else if FIELDS.contains(&property.as_str()) {
            found.extend(field_misreading(property, value));
        } else {
            let mut sentence = format!(
                "app has no property \"{property}\"; the app takes {}, and {} as get_hud lists them.",
                listed(&named),
                listed(READ_ONLY)
            );
            if HOVER.contains(&property.as_str()) {
                sentence += &format!(" Hover behavior's {property} is app.hover.{property}.");
            }
            found.push(sentence);
        }
    }
    found
}

/// - listing: the app as get_hud lists it now, the draft included.
pub(super) fn proposal(
    app: Option<&Value>,
    s: &State,
    snapshot: &Value,
    listing: &Value,
) -> AppProposal {
    let Some(value) = app.filter(|v| !v.is_null()) else {
        return AppProposal {
            given: false,
            reduced: None,
            reasons: vec![],
        };
    };
    let saved = saved(s, snapshot);
    let mut reasons = misreadings(value);
    let mut draft = Map::new();
    let mut hover = Map::new();
    for field in FIELDS {
        if let Some(asked) = read(field, &value[*field]) {
            if !same(&asked, &saved[*field]) {
                draft.insert((*field).into(), asked);
            }
        }
    }
    for field in HOVER {
        if let Some(asked) = read(field, &value["hover"][*field]) {
            if !same(&asked, &saved["hover"][*field]) {
                hover.insert((*field).into(), asked);
            }
        }
    }
    // What the app would not draw as asked (AppDraft.issues(against:)), of what the
    // draft changes, so a delay saved outside Settings' range, sent back, is none.
    for field in ["delayMs", "collapseDelayMs"] {
        let Some(value) = hover.get(field).and_then(Value::as_i64) else {
            continue;
        };
        let (low, high) = DELAY_RANGE;
        if value < low || value > high || (value - low) % DELAY_STEP != 0 {
            reasons.push(format!("{}: {field} is a whole number of milliseconds from {low} to {high} in steps of {DELAY_STEP}, as Settings sets it, which {value} is not.", path(field)));
        }
    }
    let matches = draft
        .get("matchScreen")
        .unwrap_or(&saved["matchScreen"])
        .as_bool()
        .unwrap_or(false);
    if draft.contains_key("appearance") && matches {
        reasons.push(format!("{}: appearance sets the app's windows and the menu-bar menu, and with matchScreen on, as it would be, each takes the look of the HUD on its screen instead, which is why Settings greys the Appearance out. Leave it out, or set matchScreen to false.", path("appearance")));
    }
    for name in READ_ONLY {
        let sent = &value[*name];
        if sent.is_null() || same(sent, &listing[*name]) {
            continue;
        }
        let what = if *name == "language" {
            "the Language chosen in Settings, which only Settings changes"
        } else {
            "the language the panel's names are written in now"
        };
        let listed = listing[*name]
            .as_str()
            .map(|s| format!("\"{s}\""))
            .unwrap_or_else(|| "none".into());
        reasons.push(format!("app.{name} is read only: it is {what}, {listed} as get_hud lists it. Leave it out, or give it as listed."));
    }
    if !hover.is_empty() {
        draft.insert("hover".into(), Value::Object(hover));
    }
    AppProposal {
        given: true,
        reduced: (!draft.is_empty()).then_some(Value::Object(draft)),
        reasons,
    }
}

/// What a proposal that names an app did with the app's settings (HUDTools.appNote).
pub(super) fn note(draft: Option<&Value>) -> String {
    let Some(draft) = draft else {
        return "The app's own settings are as they are saved, so there is no draft of them."
            .into();
    };
    let mut note = "The app's own settings show as an unsaved draft on every display: the user keeps it by hovering the Agent HUD on any of them and pressing Save in the panel, which keeps whatever else that panel shows unsaved with it, or drops it with Discard; it is dropped when Agent HUD restarts, and a setting of it also when the user changes that setting in Settings.".to_owned();
    if draft["showMenuBarIcon"] == false {
        note += " With the menu-bar icon hidden, Save, Discard and the Settings button stay in the HUD's panel.";
    }
    if draft["hover"]["pressOptionToOpen"] == true {
        note += " With Press Option to open on, the user holds Option while hovering the HUD to open that panel.";
    }
    note
}

/// Whether a call gives the app's settings alone (HUDTools.givesAppAlone).
pub(super) fn gives_app_alone(arguments: &Map<String, Value>) -> bool {
    let named: Vec<&str> = arguments
        .iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, _)| k.as_str())
        .collect();
    named.contains(&"app")
        && named
            .iter()
            .all(|k| ["screen", "revision", "app"].contains(k))
}
