//! What Agent HUD's MCP tools do with the panel's layouts, a screen's resting-HUD
//! style and the app's own settings, as agent-hud-native-open's HUDTools.swift does: read
//! them, and draft a change no person has saved. A proposal is checked as an agent
//! sent it and refused whole, with every reason, when the panel cannot draw it as
//! asked; it is kept as what it changes, so one that draws what is saved leaves no
//! draft.
mod choices;
mod draft;
mod listing;

use super::rules::{self, normalize_layout, quoted, same, strings};
use super::*;
use super::{app_rules, style_rules, tree};
use draft::draft;
use listing::listing;

/// The keys of a layout in a proposal.
const LAYOUT_KEYS: &[&str] = &[
    "blocks",
    "order",
    "sections",
    "hiddenSections",
    "countdown",
    "style",
    "hud",
    "sessions",
    "tokens",
];
const DRAFT_NOTE: &str = "Shown in the panel as an unsaved draft. The user keeps it by hovering the Agent HUD and pressing Save in the panel; it is dropped when Agent HUD restarts.";
const SAVED_NOTE: &str = "This is the layout already saved, so there is no draft.";
/// The two ways a proposal spells the panel as a tree: whole, or as edits of it.
const TREE_KEYS: &[&str] = &["panel", "edit"];
/// How long an edit holds a panel open, renewed by each edit.
const HOLD_MS: i64 = 120_000;

// MARK: Screens

/// A display's role, as an agent is told it, never its system name, which can be a
/// personal device name: "Built-in display" for the Mac's own (the one with a notch),
/// "Main display" for the primary one, else "Display" and its place among the
/// displays, the main display counted first.
fn role(s: &State, key: &str) -> Option<String> {
    let screen = s.screens.iter().find(|v| v["id"] == key)?;
    if screen["hasNotch"] == true {
        return Some("Built-in display".into());
    }
    let mut order: Vec<&Value> = s.screens.iter().filter(|v| v["primary"] == true).collect();
    order.extend(s.screens.iter().filter(|v| v["primary"] != true));
    let index = order.iter().position(|v| v["id"] == key).unwrap_or(0);
    Some(if index == 0 {
        "Main display".into()
    } else {
        format!("Display {}", index + 1)
    })
}

/// A screen as a sentence names it: its role in quotes, or its key where it is not
/// attached now.
fn screen_name(s: &State, key: &str, starting: bool) -> String {
    match role(s, key) {
        Some(role) => format!("\"{role}\""),
        None => format!(
            "{} screen \"{key}\", which is not attached now,",
            if starting { "The" } else { "the" }
        ),
    }
}

fn attached_list(s: &State) -> String {
    if s.screens.is_empty() {
        return "No screen is attached now.".into();
    }
    let listed = s
        .screens
        .iter()
        .filter_map(|v| v["id"].as_str())
        .map(|key| format!("\"{key}\" ({})", role(s, key).unwrap_or_default()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("The screens attached now: {listed}.")
}

fn attached<'a>(s: &'a State, key: &str) -> Option<&'a Value> {
    s.screens.iter().find(|v| v["id"] == key)
}

/// The document a call's `screen` names: "all" for the shared layout, given as "all"
/// or not at all, else a screen attached now or one that keeps a layout or a draft
/// while it is away.
fn scope(params: &Value, s: &State) -> Result<String, String> {
    match params.get("screen") {
        None | Some(Value::Null) => Ok("all".into()),
        Some(Value::String(key)) if key == "all" => Ok("all".into()),
        Some(Value::String(key)) => {
            let kept = s.saved.layouts.contains_key(key)
                || s.layouts.contains_key(key)
                || s.styles.contains_key(key);
            if attached(s, key).is_some() || kept {
                Ok(key.clone())
            } else {
                Err(format!(
                    "No screen has the key \"{key}\". {} Use \"all\" for the shared layout.",
                    attached_list(s)
                ))
            }
        }
        Some(_) => Err(
            "screen is a screen key from get_hud's screens, or \"all\" for the shared layout."
                .into(),
        ),
    }
}

/// Answers one MCP tool call. A call that is refused changes nothing.
/// - catalog: the token chart's models and agents, a statistics answer, or an object
///   with `error` where it could not be read; nil leaves them unchecked.
pub(super) fn call(
    owner: &Presentation,
    name: &str,
    params: Value,
    snapshot: &Value,
    catalog: Option<&Value>,
) -> Result<Value, String> {
    let mut guard = owner.state.lock().unwrap_or_else(PoisonError::into_inner);
    let mut transaction = guard.clone();
    let s = &mut transaction;
    let changes = |s: &State| (s.layouts.clone(), s.styles.clone(), s.app.clone());
    let before = changes(s);
    let prefix = if name == "get_hud" {
        ""
    } else {
        "Nothing was changed. "
    };
    if name == "draft_hud" {
        let taken: Vec<&str> = LAYOUT_KEYS
            .iter()
            .copied()
            .chain(TREE_KEYS.iter().copied())
            .chain(["app", "from", "revision", "screen"])
            .collect();
        if let Some(fields) = params.as_object() {
            let unknown: Vec<&String> = fields
                .keys()
                .filter(|k| !taken.contains(&k.as_str()))
                .collect();
            if !unknown.is_empty() {
                return Err(format!(
                    "Nothing was changed. Unknown: {}. It takes: {}.",
                    quoted(&unknown),
                    quoted(&taken)
                ));
            }
        }
    }
    let id = scope(&params, s).map_err(|e| format!("{prefix}{e}"))?;
    if let Some(given) = params.get("revision").filter(|v| !v.is_null()) {
        match given.as_u64() {
        Some(read) if read == s.revision => {}
        Some(_) => {
            return Err(format!(
                "{prefix}The layout changed since you read it: its revision is now {}. Call get_hud again, then propose.",
                s.revision
            ))
        }
        None => return Err(format!("{prefix}revision is the whole number get_hud returned.")),
    }
    }
    let result = match name {
        "get_hud" => Ok(listing(s, &id, snapshot, catalog, None)),
        "draft_hud" => draft(s, &id, &params, snapshot, catalog),
        "discard_hud_draft" => {
            s.layouts.remove(&id);
            s.styles.remove(&id);
            if id == "all" {
                s.holds.clear();
            } else {
                s.holds.remove(&id);
            }
            Ok(listing(
                s,
                &id,
                snapshot,
                catalog,
                Some("Layout and HUD style draft discarded; the app draft is unchanged."),
            ))
        }
        "finish_hud_edit" => {
            if id == "all" {
                s.holds.clear();
            } else {
                s.holds.remove(&id);
            }
            Ok(listing(
                s,
                &id,
                snapshot,
                catalog,
                Some("Edit hold ended. Unsaved drafts still await the user's Save."),
            ))
        }
        _ => Err("Unknown presentation tool".into()),
    };
    let mut result = result?;
    // One revision for each change of a draft, none for a call that changed none.
    if changes(s) != before {
        s.revision = s.revision.wrapping_add(1);
        result["revision"] = json!(s.revision);
    }
    *guard = transaction;
    Ok(result)
}
