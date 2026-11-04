//! What reading a hud as an agent sent it would forgive.
use super::*;

/// What is wrong with a hud as an agent sent it (HUDStyle.misreadings(ofField:)): a
/// value of the wrong type or out of its range, a name this build does not know, a
/// property the hud or its glow does not have. A value given as null is not given.
pub(in crate::presentation) fn misreadings(value: &Value) -> Vec<String> {
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
