//! One screen's resting-HUD style in an agent's words, as agent-hud-native-open checks and
//! reduces it (HUDStyle.swift, HUDStyleRules.swift, HUDTools+Style.swift): what
//! reading would forgive, what the screen would not draw as asked, and what a
//! proposal changes against what the screen has saved.
use super::rules::{listed, quoted, same};
use super::*;

const ALL: &[&str] = &["notch", "logos", "simple"];
const DOCKS: &[&str] = &["logos", "simple"];

/// A style field: its name in a hud, the setting it writes, the modes whose HUD it
/// changes as Settings shows it for them, and the range a number keeps within.
struct Field {
    name: &'static str,
    stored: &'static str,
    modes: &'static [&'static str],
    range: Option<(f64, f64)>,
}

const FIELDS: &[Field] = &[
    Field {
        name: "mode",
        stored: "mode",
        modes: ALL,
        range: None,
    },
    Field {
        name: "edge",
        stored: "edge",
        modes: DOCKS,
        range: None,
    },
    Field {
        name: "position",
        stored: "offset",
        modes: DOCKS,
        range: Some((0., 1.)),
    },
    Field {
        name: "logoSize",
        stored: "logoSize",
        modes: &["logos"],
        range: Some((20., 60.)),
    },
    Field {
        name: "logoGap",
        stored: "gapScale",
        modes: &["logos"],
        range: Some((0.1, 0.6)),
    },
    Field {
        name: "edgeDistance",
        stored: "logoEdgeDistance",
        modes: &["logos"],
        range: Some((0., 20.)),
    },
    Field {
        name: "showLogos",
        stored: "showsLogos",
        modes: &["logos"],
        range: None,
    },
    Field {
        name: "simpleSize",
        stored: "simpleSize",
        modes: &["simple"],
        range: Some((20., 60.)),
    },
    Field {
        name: "simpleGap",
        stored: "simpleGapScale",
        modes: &["simple"],
        range: Some((0.1, 0.6)),
    },
    Field {
        name: "ringWidth",
        stored: "ringWidth",
        modes: &["simple"],
        range: Some((1., 4.)),
    },
    Field {
        name: "simpleStyle",
        stored: "simpleStyle",
        modes: &["simple"],
        range: None,
    },
    Field {
        name: "showNumbers",
        stored: "showsNumbers",
        modes: &["simple"],
        range: None,
    },
    Field {
        name: "separateAgents",
        stored: "separatesAgents",
        modes: DOCKS,
        range: None,
    },
    Field {
        name: "appearance",
        stored: "appearance",
        modes: ALL,
        range: None,
    },
    Field {
        name: "transparency",
        stored: "solidTransparency",
        modes: ALL,
        range: Some((0., 1.)),
    },
];

/// A glow field: its name under `hud.glow`, the setting it writes, its range and the
/// step Settings' slider moves it by.
struct GlowField {
    name: &'static str,
    stored: &'static str,
    range: Option<(f64, f64)>,
    step: Option<f64>,
}

const GLOW: &[GlowField] = &[
    GlowField {
        name: "style",
        stored: "glowStyle",
        range: None,
        step: None,
    },
    GlowField {
        name: "effect",
        stored: "glowEffect",
        range: None,
        step: None,
    },
    GlowField {
        name: "brightness",
        stored: "glowBrightness",
        range: Some((0.2, 1.)),
        step: None,
    },
    GlowField {
        name: "range",
        stored: "glowRange",
        range: Some((0., 36.)),
        step: None,
    },
    GlowField {
        name: "blur",
        stored: "glowBlur",
        range: Some((0., 36.)),
        step: None,
    },
    GlowField {
        name: "speed",
        stored: "breathSeconds",
        range: Some((1., 12.)),
        step: None,
    },
    GlowField {
        name: "idleSpeed",
        stored: "idleBreathSeconds",
        range: Some((1., 24.)),
        step: Some(0.5),
    },
    GlowField {
        name: "amplitude",
        stored: "breathAmplitude",
        range: Some((0., 1.)),
        step: None,
    },
    GlowField {
        name: "outwardOnly",
        stored: "glowOutwardOnly",
        range: None,
        step: None,
    },
    GlowField {
        name: "gridPitch",
        stored: "glowGridPitch",
        range: Some((4., 12.)),
        step: Some(1.),
    },
    GlowField {
        name: "gridDensity",
        stored: "glowGridDensity",
        range: Some((0.5, 1.5)),
        step: Some(0.05),
    },
    GlowField {
        name: "gridCore",
        stored: "glowGridCore",
        range: Some((0., 8.)),
        step: Some(1.),
    },
    GlowField {
        name: "gridFade",
        stored: "glowGridFade",
        range: Some((0., 8.)),
        step: Some(1.),
    },
];

const MODES: &[&str] = &["notch", "dynamic", "simple"];
const EDGES: &[&str] = &["top", "right", "bottom", "left"];
const APPEARANCES: &[&str] = &["dark", "light", "system"];
const GLOW_STYLES: &[&str] = &["blur", "dots", "ascii", "blocks", "braille", "binary"];
const GLOW_EFFECTS: &[&str] = &["breathe", "flow", "scan", "ripple", "shimmer", "boot"];

/// A mode as an agent names it.
fn agent_mode(stored: &str) -> &str {
    if stored == "logos" {
        "dynamic"
    } else {
        stored
    }
}

/// A bound as a sentence writes it: 12, 0.1.
fn written(value: f64) -> String {
    format!("{value}")
}

/// A number's range as a sentence writes it, with the step it keeps to.
fn numbered((low, high): (f64, f64), step: Option<f64>) -> String {
    let bounds = format!("from {} to {}", written(low), written(high));
    match step {
        None => format!("a number {bounds}"),
        Some(1.) => format!("a whole number {bounds}"),
        Some(step) => format!("a number {bounds} in steps of {}", written(step)),
    }
}

/// Whether `value` lies on a step of `step` from `start`, within what floating point
/// leaves of a decimal step.
fn keeps(value: f64, step: f64, start: f64) -> bool {
    let steps = (value - start) / step;
    (steps - steps.round()).abs() < 0.000_001
}

fn number_in(value: &Value, (low, high): (f64, f64)) -> bool {
    value
        .as_f64()
        .is_some_and(|n| n.is_finite() && n >= low && n <= high)
}

/// The names a hud takes beside `agents`.
fn names() -> Vec<&'static str> {
    FIELDS.iter().map(|f| f.name).chain(["glow"]).collect()
}

// MARK: What reading would forgive

/// What is wrong with a hud as an agent sent it (HUDStyle.misreadings(ofField:)): a
/// value of the wrong type or out of its range, a name this build does not know, a
/// property the hud or its glow does not have. A value given as null is not given.
pub(super) fn misreadings(value: &Value) -> Vec<String> {
    if value.is_null() {
        return vec![];
    }
    let Some(hud) = value.as_object() else {
        return vec!["hud is an object.".into()];
    };
    let mut found = vec![];
    for (property, value) in hud {
        match property.as_str() {
            "agents" => found.extend(super::rules::hud_agent_misreadings(value)),
            "reset" => {
                if !value.is_null() && !value.is_boolean() {
                    found.push("hud.reset is true or false.".into());
                }
            }
            "glow" => found.extend(glow_misreadings(value)),
            name => {
                let Some(field) = FIELDS.iter().find(|f| f.name == name) else {
                    let taken: Vec<&str> = ["agents"]
                        .into_iter()
                        .chain(names())
                        .chain(["reset"])
                        .collect();
                    found.push(format!(
                        "hud has no property \"{name}\"; the hud takes {}.",
                        listed(&taken)
                    ));
                    continue;
                };
                if value.is_null() {
                    continue;
                }
                let path = format!("hud.{name}");
                let choices = match name {
                    "mode" => Some(MODES),
                    "edge" => Some(EDGES),
                    "appearance" => Some(APPEARANCES),
                    "simpleStyle" => Some(super::SIMPLE_STYLES),
                    _ => None,
                };
                if let Some(choices) = choices {
                    if !value.as_str().is_some_and(|s| choices.contains(&s)) {
                        found.push(format!("{path} is one of {}.", quoted(choices)));
                    }
                } else if let Some(range) = field.range {
                    if !number_in(value, range) {
                        found.push(format!("{path} is {}.", numbered(range, None)));
                    }
                } else if !value.is_boolean() {
                    found.push(format!("{path} is true or false."));
                }
            }
        }
    }
    found
}

fn glow_misreadings(value: &Value) -> Vec<String> {
    if value.is_null() {
        return vec![];
    }
    let Some(glow) = value.as_object() else {
        return vec!["hud.glow is an object.".into()];
    };
    let mut found = vec![];
    for (property, value) in glow {
        let path = format!("hud.glow.{property}");
        let Some(field) = GLOW.iter().find(|f| f.name == property) else {
            let taken: Vec<&str> = GLOW.iter().map(|f| f.name).collect();
            found.push(format!(
                "hud.glow has no property \"{property}\"; the glow takes {}.",
                listed(&taken)
            ));
            continue;
        };
        if value.is_null() {
            continue;
        }
        let choices = match field.name {
            "style" => Some(GLOW_STYLES),
            "effect" => Some(GLOW_EFFECTS),
            _ => None,
        };
        if let Some(choices) = choices {
            if !value.as_str().is_some_and(|s| choices.contains(&s)) {
                found.push(format!("{path} is one of {}.", quoted(choices)));
            }
        } else if let Some(range) = field.range {
            if !number_in(value, range) {
                found.push(format!("{path} is {}.", numbered(range, field.step)));
            }
        } else if !value.is_boolean() {
            found.push(format!("{path} is true or false."));
        }
    }
    found
}

// MARK: What a screen has saved

/// The keys of a screen's settings that make up its look: every display setting but
/// its hardware, the app's own settings and the panel's.
fn look_key(key: &str) -> bool {
    !HARDWARE.contains(&key) && !GLOBAL.contains(&key) && !super::tools::panel_key(key)
}

/// The look a screen draws from `saved` (ui/use-hud-config.ts `getScreenHUDConfig`):
/// the defaults, the notch mode on a screen with a notch and a dock on one without,
/// the shared glow, then the screen's own settings.
pub(super) fn drawn(saved: &Saved, key: &str, has_notch: bool) -> Map<String, Value> {
    let defaults: Value = serde_json::from_str(DEFAULTS).expect("checked-in defaults");
    let mut style = defaults.as_object().cloned().unwrap_or_default();
    style.insert(
        "mode".into(),
        json!(if has_notch { "notch" } else { "logos" }),
    );
    merge(&mut style, &saved.screen_defaults);
    let own = saved.screens.get(key).cloned().unwrap_or(Value::Null);
    if let Some(own) = own.as_object() {
        merge(&mut style, own);
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
pub(super) fn effective(s: &State, key: &str, has_notch: bool) -> Map<String, Value> {
    let mut style = drawn(&s.saved, key, has_notch);
    if let Some(draft) = s.styles.get(key).and_then(Value::as_object) {
        merge(&mut style, draft);
    }
    style
}

/// The default look a screen is given, as the HUD style's Reset to Default gives it.
fn default_look(s: &State, has_notch: bool) -> Map<String, Value> {
    let defaults: Value = serde_json::from_str(DEFAULTS).expect("checked-in defaults");
    let mut style = defaults.as_object().cloned().unwrap_or_default();
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
        json!(if has_notch { "notch" } else { "logos" }),
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
pub(super) fn restoring(
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
fn patch(hud: &Value) -> Map<String, Value> {
    let mut patch = Map::new();
    for field in FIELDS {
        let Some(value) = hud.get(field.name).filter(|v| !v.is_null()) else {
            continue;
        };
        let value = match field.name {
            "mode" if value == "dynamic" => json!("logos"),
            _ => value.clone(),
        };
        patch.insert(field.stored.into(), value);
    }
    for field in GLOW {
        if let Some(value) = hud["glow"].get(field.name).filter(|v| !v.is_null()) {
            patch.insert(field.stored.into(), value.clone());
        }
    }
    patch
}

// MARK: What a screen can draw

/// Every field a reduced style sets that the screen's resting HUD would not draw as
/// asked (HUDStyle.issues): `notch` without a notch, a field the mode the HUD would
/// have does not draw, a glow on a Simple panel, a glow field of another glow style, a
/// breath's depth under another effect, and a glow number between Settings' steps.
fn issues(
    changes: &Map<String, Value>,
    saved: &Map<String, Value>,
    has_notch: bool,
    screen: &str,
) -> Vec<String> {
    let mut placed = saved.clone();
    merge(&mut placed, changes);
    let text = |key: &str| {
        placed
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let mode = text("mode");
    let mut found = vec![];
    if changes.get("mode") == Some(&json!("notch")) && !has_notch {
        found.push(format!("hud.mode: {screen} has no notch, so its HUD cannot be \"notch\". It can be \"dynamic\" or \"simple\"."));
    }
    let named = format!("the HUD of {screen} would be \"{}\"", agent_mode(&mode));
    for field in FIELDS.iter().filter(|f| f.name != "mode") {
        if changes.contains_key(field.stored) && !field.modes.contains(&mode.as_str()) {
            let modes = field
                .modes
                .iter()
                .map(|m| format!("\"{}\"", agent_mode(m)))
                .collect::<Vec<_>>()
                .join(" or ");
            found.push(format!(
                "hud.{}: {} sets a {modes} HUD, and {named}. Leave it out, or set mode to {modes}.",
                field.name, field.name
            ));
        }
    }
    let (style, effect) = (text("glowStyle"), text("glowEffect"));
    let grids = quoted(&GLOW_STYLES[1..]);
    for field in GLOW {
        let Some(value) = changes.get(field.stored) else {
            continue;
        };
        let path = format!("hud.glow.{}", field.name);
        let drawn_in_style = match field.name {
            "range" | "blur" | "outwardOnly" => style == "blur",
            "gridPitch" | "gridDensity" | "gridCore" | "gridFade" => style != "blur",
            _ => true,
        };
        if mode == "simple" {
            found.push(format!(
                "{path}: A \"simple\" HUD draws no glow, and {named}."
            ));
        } else if !drawn_in_style {
            let styles = if style == "blur" {
                format!("a grid glow style, one of {grids},")
            } else {
                "the \"blur\" glow style,".to_owned()
            };
            found.push(format!(
                "{path}: {} sets {styles} and the glow of {screen} would be \"{style}\".",
                field.name
            ));
        } else if field.name == "amplitude" && effect != "breathe" {
            found.push(format!("{path}: amplitude sets the \"breathe\" effect, and the glow of {screen} would play \"{effect}\"."));
        } else if let (Some(step), Some(range), Some(number)) =
            (field.step, field.range, value.as_f64())
        {
            if !keeps(number, step, range.0) {
                found.push(format!(
                    "{path}: {} is {}, as Settings sets it, which {} is not.",
                    field.name,
                    numbered(range, Some(step)),
                    written(number)
                ));
            }
        }
    }
    found
}

// MARK: A proposal's style

/// The style part of a proposal (HUDTools.StyleProposal): the style fields of its
/// `hud`, checked against the one screen they are for. A proposal is whole: a field
/// it leaves out is what the screen has saved, and a field set to what is saved
/// changes nothing and is no issue, so the style get_hud lists can be sent back. The
/// style a waiting draft lists asks nothing new. `reset` asks for the default look
/// whole, in place of every other style field.
pub(super) struct StyleProposal {
    /// Whether the scope is a screen attached now, which has a resting HUD.
    pub attached: bool,
    /// What the proposal changes about what the screen has saved, in settings' keys.
    pub reduced: Map<String, Value>,
    /// The mode the screen's resting HUD would have; nil without a screen.
    pub mode: Option<String>,
    /// Why the style cannot be drawn as asked.
    pub reasons: Vec<String>,
    /// Whether a dock leaving the notch was placed clear of it, which was not asked.
    pub placed_clear: bool,
}

/// Whether a hud field as sent sets anything: null is not given, and neither is a
/// glow without a property that is not null.
fn sets(name: &str, value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Object(glow)) if name == "glow" => glow.values().any(|v| !v.is_null()),
        Some(_) => true,
    }
}

/// - scope: "all" for the shared layout, else a screen key.
/// - screen: the attached screen of that key; nil where it is away.
/// - name: the screen as a sentence names it, such as "\"Main display\"".
pub(super) fn proposal(
    hud: &Value,
    scope: &str,
    screen: Option<&Value>,
    s: &State,
    name: &str,
    attached: &str,
) -> StyleProposal {
    let given: Vec<String> = names()
        .into_iter()
        .filter(|n| sets(n, hud.get(*n)))
        .map(|n| format!("hud.{n}"))
        .collect();
    let resets = hud["reset"] == true;
    let mut all_given = given.clone();
    if resets {
        all_given.push("hud.reset".into());
    }
    let sets_word = if all_given.len() == 1 { "sets" } else { "set" };
    let paths = listed(&all_given);
    let none = |reasons: Vec<String>| StyleProposal {
        attached: false,
        reduced: Map::new(),
        mode: None,
        reasons,
        placed_clear: false,
    };
    if scope == "all" {
        return none(if all_given.is_empty() {
            vec![]
        } else {
            vec![format!("{paths} {sets_word} the resting HUD of one screen, so the proposal needs that screen's key; with screen \"all\" the hud takes agents alone. {attached}")]
        });
    }
    let Some(screen) = screen else {
        return none(if all_given.is_empty() {
            vec![]
        } else {
            vec![format!("{paths} {sets_word} the resting HUD of an attached screen, and {name} has none to draw it on.")]
        });
    };
    let has_notch = screen["hasNotch"] == true;
    let saved = drawn(&s.saved, scope, has_notch);
    let asked = reduced(&patch(hud), &saved);
    let (changes, placed_clear, reasons) = if resets {
        let reasons = if given.is_empty() {
            vec![]
        } else {
            vec![format!("hud.reset proposes the default style of the resting HUD of {name} whole, so the hud takes no other style field with it: leave out {}, or leave out hud.reset.", listed(&given))]
        };
        (reduced(&default_look(s, has_notch), &saved), false, reasons)
    } else if s
        .styles
        .get(scope)
        .is_some_and(|waiting| same(waiting, &Value::Object(asked.clone())))
    {
        (asked, false, vec![])
    } else {
        let mut placed = patch(hud);
        let clear = placed_clear_of_the_notch(&mut placed, &saved, has_notch);
        let changes = reduced(&placed, &saved);
        let reasons = issues(&changes, &saved, has_notch, name);
        (changes, clear, reasons)
    };
    let mut after = saved.clone();
    merge(&mut after, &changes);
    StyleProposal {
        attached: true,
        mode: after.get("mode").and_then(Value::as_str).map(str::to_owned),
        reduced: changes,
        reasons,
        placed_clear,
    }
}

/// A dock that leaves the notch for the top edge, where it has never been moved from
/// the centre, starts at the top left, clear of the hardware, as Settings places one
/// (HUDStyle.placedClearOfTheNotch). Returns whether it was placed.
fn placed_clear_of_the_notch(
    patch: &mut Map<String, Value>,
    saved: &Map<String, Value>,
    has_notch: bool,
) -> bool {
    let saved_offset = saved.get("offset").and_then(Value::as_f64).unwrap_or(0.5);
    let docked = patch
        .get("mode")
        .and_then(Value::as_str)
        .is_some_and(|m| m != "notch");
    let at_centre = patch
        .get("offset")
        .and_then(Value::as_f64)
        .is_none_or(|offset| offset == saved_offset);
    let top = patch
        .get("edge")
        .or_else(|| saved.get("edge"))
        .and_then(Value::as_str)
        .is_none_or(|edge| edge == "top");
    if has_notch
        && docked
        && saved.get("mode") == Some(&json!("notch"))
        && at_centre
        && top
        && saved_offset == 0.5
    {
        patch.insert("offset".into(), json!(0));
        return true;
    }
    false
}

/// A screen's look in an agent's words, as get_hud lists it under `hud`.
pub(super) fn listing(style: &Map<String, Value>) -> Value {
    let mut hud = Map::new();
    for field in FIELDS {
        let value = style.get(field.stored).cloned().unwrap_or(Value::Null);
        let value = match (field.name, value.as_str()) {
            ("mode", Some(mode)) => json!(agent_mode(mode)),
            _ => value,
        };
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

/// Whether a call gives a screen's style alone: beside `screen`, `revision` and `app`
/// it names only a `hud`, every property of which it gives is a style field or
/// `reset`, and none `agents` (HUDTools.givesStyleAlone).
pub(super) fn gives_style_alone(arguments: &Map<String, Value>) -> bool {
    let named = arguments
        .iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, _)| k.as_str());
    if !named
        .clone()
        .all(|k| ["screen", "revision", "hud", "app"].contains(&k))
    {
        return false;
    }
    let Some(hud) = arguments.get("hud").and_then(Value::as_object) else {
        return false;
    };
    let style: Vec<&str> = names().into_iter().chain(["reset"]).collect();
    let given: Vec<&str> = hud
        .iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, _)| k.as_str())
        .collect();
    !given.is_empty() && given.iter().all(|k| style.contains(k))
}
