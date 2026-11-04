//! What reading a proposed layout would forgive.
use super::*;

/// What reading a proposed layout would forgive, as sentences
/// (PanelLayout.misreadings(of:)): a value of the wrong type, a name this build does
/// not know, a number out of its range, a property a block, the style, the sessions,
/// the tokens or the hud does not have. The hud's style fields are told by
/// `style_rules::misreadings`. A value given as null is not given.
pub(in crate::presentation) fn misreadings(document: &Value) -> Vec<String> {
    let mut found = names(&document["order"], "order", None);
    let mut sections: Vec<&str> = SECTIONS.to_vec();
    sections.extend(
        document["custom"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(id, _)| id.as_str()),
    );
    found.extend(names(&document["sections"], "sections", Some(&sections)));
    found.extend(names(
        &document["hiddenSections"],
        "hiddenSections",
        Some(SECTIONS),
    ));
    if !document["countdown"].is_null() && !document["countdown"].is_boolean() {
        found.push("countdown is true or false.".into());
    }
    found.extend(style_misreadings(&document["style"]));
    found.extend(super::style_rules::misreadings(&document["hud"]));
    found.extend(session_misreadings(&document["sessions"]));
    found.extend(token_misreadings(&document["tokens"]));
    match &document["blocks"] {
        Value::Null => {}
        Value::Object(blocks) => {
            for (key, block) in blocks {
                found.extend(block_misreadings(block, &format!("blocks.{key}")));
            }
        }
        _ => found.push("blocks is an object keyed by block key.".into()),
    }
    found
}

/// What is wrong with a list of names: not a list, an entry that is not a string, or
/// one that is not `allowed`.
fn names(value: &Value, path: &str, allowed: Option<&[&str]>) -> Vec<String> {
    if value.is_null() {
        return vec![];
    }
    let Some(items) = value.as_array() else {
        return vec![format!("{path} is a list.")];
    };
    let mut found = vec![];
    for (index, item) in items.iter().enumerate() {
        let Some(name) = item.as_str() else {
            found.push(format!("{path}[{index}] is a string."));
            continue;
        };
        if let Some(allowed) = allowed.filter(|a| !a.contains(&name)) {
            found.push(format!(
                "{path}[{index}]: \"{name}\" is not one of {}.",
                quoted(allowed)
            ));
        }
    }
    found
}

/// What is wrong with a name: not one of `allowed`.
fn misnamed(value: &Value, path: &str, allowed: &[&str]) -> Vec<String> {
    if value.is_null() || value.as_str().is_some_and(|s| allowed.contains(&s)) {
        return vec![];
    }
    vec![format!("{path} is one of {}.", quoted(allowed))]
}

/// Empty entries of a list of names, each with the sentence `what`.
fn empty_names(value: &Value, path: &str, what: &str) -> Vec<String> {
    entries(value)
        .filter(|(_, s)| s.is_empty())
        .map(|(index, _)| format!("{path}[{index}] is {what}"))
        .collect()
}

fn block_misreadings(value: &Value, path: &str) -> Vec<String> {
    let Some(block) = value.as_object() else {
        return vec![format!("{path} is an object.")];
    };
    let mut found = vec![];
    let list =
        |v: &Value| v.is_null() || v.as_array().is_some_and(|a| a.iter().all(Value::is_string));
    for (property, value) in block {
        match property.as_str() {
            "widget" => found.extend(misnamed(value, &format!("{path}.widget"), WIDGETS)),
            "hidden" | "countdown" | "legend" => {
                if !value.is_null() && !value.is_boolean() {
                    found.push(format!("{path}.{property} is true or false."));
                }
            }
            "hiddenRows" | "rows" => {
                if !list(value) {
                    found.push(format!("{path}.{property} is a list of row keys."));
                }
            }
            "accounts" | "hiddenAccounts" => {
                if !list(value) {
                    found.push(format!("{path}.{property} is a list of account ids from get_hud."));
                }
            }
            "columns" => {
                if !value.is_null() && !whole(value).is_some_and(|n| (2..=6).contains(&n)) {
                    found.push(format!("{path}.columns is a whole number from 2 to 6."));
                }
            }
            "ringSize" => found.extend(misnamed(value, &format!("{path}.ringSize"), RING_SIZES)),
            "span" => found.extend(misnamed(value, &format!("{path}.span"), SPANS)),
            "period" => found.extend(misnamed(value, &format!("{path}.period"), PERIODS)),
            _ => found.push(format!("{path} has no property \"{property}\"; a block takes widget, hidden, hiddenRows, hiddenAccounts, rows, accounts, countdown, columns, ringSize, legend, span and period.")),
        }
    }
    found
}

fn style_misreadings(value: &Value) -> Vec<String> {
    if value.is_null() {
        return vec![];
    }
    let Some(style) = value.as_object() else {
        return vec!["style is an object.".into()];
    };
    let mut found = vec![];
    for (property, value) in style {
        match property.as_str() {
            "spacing" => found.extend(misnamed(value, "style.spacing", SPACINGS)),
            "separators" => {
                if !value.is_null() && !value.is_boolean() {
                    found.push("style.separators is true or false.".into());
                }
            }
            "width" => {
                if !value.is_null() && !whole(value).is_some_and(|n| WIDTHS.contains(&n)) {
                    found.push(format!(
                        "style.width is a whole number from {} to {}.",
                        WIDTHS.start(),
                        WIDTHS.end()
                    ));
                }
            }
            // The panel's style is no place for the HUD's look, which is one screen's.
            "appearance" | "transparency" => found.push(format!("style has no property \"{property}\"; the style takes spacing, separators and width. The HUD's {property} is hud.{property}, for one screen.")),
            _ => found.push(format!("style has no property \"{property}\"; the style takes spacing, separators and width.")),
        }
    }
    found
}

fn session_misreadings(value: &Value) -> Vec<String> {
    if value.is_null() {
        return vec![];
    }
    let Some(fields) = value.as_object() else {
        return vec!["sessions is an object.".into()];
    };
    let mut found = vec![];
    for (property, value) in fields {
        match property.as_str() {
            "limit" => {
                if !value.is_null() && !whole(value).is_some_and(|n| (1..=10).contains(&n)) {
                    found.push("sessions.limit is a whole number from 1 to 10.".into());
                }
            }
            "period" => found.extend(misnamed(value, "sessions.period", SESSION_PERIODS)),
            "agents" => {
                found.extend(names(value, "sessions.agents", None));
                found.extend(empty_names(value, "sessions.agents", "an agent's name from get_hud's sessions.offeredAgents."));
            }
            "projects" => {
                found.extend(names(value, "sessions.projects", None));
                found.extend(empty_names(value, "sessions.projects", "a project's id from get_hud's sessions.offeredProjects, or a path inside one."));
            }
            "sort" | "states" => found.push(format!("sessions.{property}: the panel lists sessions newest last message first and filters them by no state, so sessions take no {property}; they take limit, period, agents and projects.")),
            _ => found.push(format!("sessions has no property \"{property}\"; sessions take limit, period, agents and projects.")),
        }
    }
    found
}

fn token_misreadings(value: &Value) -> Vec<String> {
    if value.is_null() {
        return vec![];
    }
    let Some(fields) = value.as_object() else {
        return vec!["tokens is an object.".into()];
    };
    let mut found = vec![];
    for (property, value) in fields {
        match property.as_str() {
            "period" => found.extend(misnamed(value, "tokens.period", PERIODS)),
            "agents" => {
                found.extend(names(value, "tokens.agents", None));
                found.extend(empty_names(
                    value,
                    "tokens.agents",
                    "an agent's name from get_hud's tokens.offeredAgents.",
                ));
            }
            "models" | "hiddenModels" | "modelOrder" => {
                let path = format!("tokens.{property}");
                found.extend(names(value, &path, None));
                found.extend(empty_names(
                    value,
                    &path,
                    "a model's id from get_hud's tokens.offeredModels.",
                ));
            }
            _ => found.push(format!(
                "tokens has no property \"{property}\"; tokens take period, agents, models, hiddenModels and modelOrder."
            )),
        }
    }
    found
}

/// What is wrong with the hud's agents.
pub(in crate::presentation) fn hud_agent_misreadings(value: &Value) -> Vec<String> {
    let mut found = names(value, "hud.agents", None);
    found.extend(empty_names(
        value,
        "hud.agents",
        "an agent's name from get_hud's screens.",
    ));
    found
}
