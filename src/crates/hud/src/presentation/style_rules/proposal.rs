//! A proposal's style: what it changes against what the screen has saved, and what
//! the screen's resting HUD would not draw as asked.
use super::look::{default_look, drawn, patch, reduced};
use super::*;

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
        found.push(format!("hud.mode: {screen} has no notch, so its HUD cannot be \"notch\". It can only be \"simple\"."));
    }
    let named = format!("the HUD of {screen} would be \"{mode}\"");
    for field in FIELDS.iter().filter(|f| f.name != "mode") {
        if changes.contains_key(field.stored) && !field.modes.contains(&mode.as_str()) {
            let modes = field
                .modes
                .iter()
                .map(|m| format!("\"{m}\""))
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
                    "{path}: {} is {}, as Settings sets it, which {number} is not.",
                    field.name,
                    numbered(range, Some(step)),
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
pub(in crate::presentation) struct StyleProposal {
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
pub(in crate::presentation) fn proposal(
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

/// Whether a call gives a screen's style alone: beside `screen`, `revision` and `app`
/// it names only a `hud`, every property of which it gives is a style field or
/// `reset`, and none `agents` (HUDTools.givesStyleAlone).
pub(in crate::presentation) fn gives_style_alone(arguments: &Map<String, Value>) -> bool {
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
