//! draft_hud: a proposal checked whole and kept as what it changes.
use super::choices::{
    canonical, choice, effective, hold_for_edit, proposal_note, propose_choice, propose_shared,
    propose_style, saved_layout,
};
use super::listing::{app_listing, listing};
use super::*;

/// The agents a scope's layout can list in `hud.agents` now, and who shows them: an
/// attached screen's own in `mode`, the mode it has now where none is given, else what
/// any attached screen can show.
fn hud_agents(s: &State, id: &str, mode: Option<&str>, snapshot: &Value) -> (Vec<String>, String) {
    let mode_of = |key: &str, has_notch: bool| {
        style_rules::effective(s, key, has_notch)
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("notch")
            .to_owned()
    };
    if let Some(screen) = attached(s, id) {
        let mode = mode
            .map(str::to_owned)
            .unwrap_or_else(|| mode_of(id, screen["hasNotch"] == true));
        return (
            rules::resting_agents(snapshot, &mode, now()),
            format!("the resting HUD of {}", screen_name(s, id, false)),
        );
    }
    let mut agents = vec![];
    for screen in &s.screens {
        let key = screen["id"].as_str().unwrap_or("");
        agents.extend(rules::resting_agents(
            snapshot,
            &mode_of(key, screen["hasNotch"] == true),
            now(),
        ));
    }
    (agents, "any attached screen's resting HUD".into())
}

fn refusal(reasons: Vec<String>) -> String {
    format!("Nothing was changed. {}", reasons.join(" "))
}

/// draft_hud: a layout for a scope, a screen's style, the app's settings, or `from`
/// a version (HUDTools.propose).
pub(super) fn draft(
    s: &mut State,
    id: &str,
    params: &Value,
    snapshot: &Value,
    catalog: Option<&Value>,
) -> Result<Value, String> {
    let arguments = object(params, "proposal")?;
    let screen = attached(s, id).cloned();
    let app = app_rules::proposal(params.get("app"), s, snapshot, &app_listing(s, snapshot));
    let present = rules::present_blocks(snapshot);
    let checked = catalog.filter(|c| c["error"].is_null());
    if let Some(from) = params.get("from").filter(|v| !v.is_null()) {
        if LAYOUT_KEYS
            .iter()
            .chain(TREE_KEYS)
            .any(|k| !params[*k].is_null())
        {
            return Err("Nothing was changed. Pass either from or a layout, not both.".into());
        }
        if !app.reasons.is_empty() {
            return Err(refusal(app.reasons));
        }
        // A proposal is whole: for a screen, the previous version brings its look back,
        // as Revert to Previous does; the default layout and the shared one leave the
        // look as saved, so a style draft they do not bring is dropped.
        match from.as_str() {
            Some("default") => {
                if id == "all" {
                    propose_shared(s, json!({}), &present);
                } else {
                    propose_choice(s, id, Some(json!({})));
                    s.styles.remove(id);
                }
            }
            Some("previous") => {
                let Some(previous) = s.previous.clone() else {
                    return Err(if id == "all" {
                        "Nothing was changed. There is no previous saved layout.".into()
                    } else {
                        format!(
                            "Nothing was changed. {} has no previous saved layout.",
                            screen_name(s, id, true)
                        )
                    });
                };
                if id == "all" {
                    propose_shared(s, canonical(&previous.shared_layout), &present);
                } else {
                    propose_choice(s, id, previous.layouts.get(id).map(canonical));
                    match &screen {
                        Some(screen) => {
                            let look = style_rules::restoring(
                                &previous,
                                s,
                                id,
                                screen["hasNotch"] == true,
                            );
                            if look.is_empty() {
                                s.styles.remove(id);
                            } else {
                                s.styles.insert(id.into(), Value::Object(look));
                            }
                        }
                        None => {
                            s.styles.remove(id);
                        }
                    }
                }
            }
            Some("shared") => {
                if id == "all" {
                    return Err("Nothing was changed. from \"shared\" is for a screen: it proposes that the screen use the shared layout, which is screen \"all\".".into());
                }
                propose_choice(s, id, None);
                s.styles.remove(id);
            }
            _ => {
                return Err(
                    "Nothing was changed. from is \"default\", \"previous\" or \"shared\".".into(),
                )
            }
        }
        if app.given {
            s.app = app.reduced.clone();
        }
        hold_for_edit(s, id);
        let mut note = proposal_note(s, id);
        if app.given {
            note += &format!(" {}", app_rules::note(s.app.as_ref()));
        }
        return Ok(listing(s, id, snapshot, catalog, Some(&note)));
    }
    let current = effective(s, id).clone();
    let whole_tree = params.get("panel").filter(|v| !v.is_null());
    let edits = params.get("edit").filter(|v| !v.is_null());
    let mut tree_reasons = vec![];
    let mut tree_notes = vec![];
    let mut document = if whole_tree.is_some() || edits.is_some() {
        if whole_tree.is_some() && edits.is_some() {
            return Err(
                "Nothing was changed. Pass either panel, the whole tree, or edit, steps that change it, not both."
                    .into(),
            );
        }
        let spelled: Vec<&str> = LAYOUT_KEYS
            .iter()
            .copied()
            .filter(|k| *k != "hud" && !params[*k].is_null())
            .collect();
        if !spelled.is_empty() {
            return Err(format!(
                "Nothing was changed. {} spell the layout the older way; with panel or edit, set them in the tree: the panel's spacing, separators, width and countdown, the blocks' and sections' fields, and their order as the children's order.",
                quoted(&spelled)
            ));
        }
        let mut panel = match whole_tree {
            Some(given) => given.clone(),
            None => tree::tree(&current, &present),
        };
        if let Some(edits) = edits {
            let (issues, notes) = tree::apply(&mut panel, edits);
            tree_reasons.extend(issues);
            tree_notes = notes;
        }
        let read = tree::layout_from(&panel, &present);
        tree_reasons.extend(read.issues);
        let mut document = read.layout;
        // The tree does not spell the resting HUD's agents: they stay unless given.
        let mut hud = params["hud"].clone();
        if hud["agents"].is_null() && !current["hud"]["agents"].is_null() {
            if !hud.is_object() {
                hud = json!({});
            }
            hud["agents"] = current["hud"]["agents"].clone();
        }
        if !hud.is_null() {
            document["hud"] = hud;
        }
        document
    } else {
        let mut document = Value::Object(
            arguments
                .iter()
                .filter(|(k, _)| LAYOUT_KEYS.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );
        tree::carry(&mut document, &current);
        document
    };
    let name = screen_name(s, id, false);
    let held = [canonical(effective(s, id)), canonical(saved_layout(s, id))];
    // A project named by a path inside it, an agent's own working directory among
    // them, is listed by its id.
    let projects = rules::project_match(&document["sessions"]["projects"], snapshot, &held);
    // Every reason at once: what reading would forgive, then what the rest asks that
    // the panel, the screen's resting HUD or the app cannot draw as asked.
    let mut reasons = tree_reasons;
    reasons.extend(rules::misreadings(&document));
    let style = style_rules::proposal(
        &params["hud"],
        id,
        screen.as_ref(),
        s,
        &name,
        &attached_list(s),
    );
    reasons.extend(rules::issues(&document, &present, &held));
    reasons.extend(rules::filter_issues(&document, snapshot, checked, &held));
    reasons.extend(projects.issues.iter().cloned());
    reasons.extend(style.reasons.iter().cloned());
    let (available, whose) = hud_agents(s, id, style.mode.as_deref(), snapshot);
    reasons.extend(rules::hud_issues(&document, &available, &whose, &held));
    reasons.extend(app.reasons.iter().cloned());
    if !reasons.is_empty() {
        return Err(refusal(reasons));
    }
    // A proposal that draws what is saved is the saved layout, however it is spelled
    // out: it leaves no draft. A proposal is whole: a style it leaves out is the style
    // the screen has saved. A call with the style alone leaves the layout as it is, and
    // a call with the app's settings alone leaves the layout and the style.
    let style_alone = style_rules::gives_style_alone(arguments);
    let app_alone = app_rules::gives_app_alone(arguments);
    if document["sessions"]["projects"].is_array() {
        document["sessions"]["projects"] = json!(projects.ids);
    }
    let asked = rules::reduced(&normalize_layout(&document)?, &present);
    if !style_alone && !app_alone {
        if id == "all" {
            propose_shared(s, asked, &present);
        } else {
            let choice = choice(s, id, &asked, &present);
            propose_choice(s, id, choice);
        }
    }
    if id != "all" && !app_alone {
        propose_style(s, id, &style);
    }
    if app.given {
        s.app = app.reduced.clone();
    }
    // A layout edit holds its screen's panel open; the style alone closes it, so the
    // HUD it restyled shows; the app's settings alone hold nothing and close nothing.
    if style_alone {
        s.holds.remove(id);
    } else if !app_alone {
        hold_for_edit(s, id);
    }
    if app_alone {
        let note = format!(
            "{} A call with only app leaves every layout and every screen's HUD style as it is.",
            app_rules::note(s.app.as_ref())
        );
        return Ok(listing(s, id, snapshot, catalog, Some(&note)));
    }
    let mut note = proposal_note(s, id);
    for added in &tree_notes {
        note += &format!(" {added}");
    }
    if style.placed_clear {
        note += " The dock it becomes starts at position 0, the top left, clear of the notch, as Settings places one; another position can be proposed once the user has saved it.";
    }
    for (given, id) in &projects.paths {
        note += &format!(
            " \"{given}\" names the project \"{id}\", which sessions.projects lists by that id."
        );
    }
    if style_alone {
        note += &format!(
            " The layout of the panel on {name} is unchanged: a call with only hud style fields leaves the layout as it is."
        );
    }
    if app.given {
        note += &format!(" {}", app_rules::note(s.app.as_ref()));
    }
    Ok(listing(s, id, snapshot, catalog, Some(&note)))
}
