//! What Agent HUD's MCP tools do with the panel's layouts, a screen's resting-HUD
//! style and the app's own settings, as agent-hud-native-open's HUDTools.swift does: read
//! them, and draft a change no person has saved. A proposal is checked as an agent
//! sent it and refused whole, with every reason, when the panel cannot draw it as
//! asked; it is kept as what it changes, so one that draws what is saved leaves no
//! draft.
use super::rules::{self, normalize_layout, quoted, same, strings};
use super::*;
use super::{app_rules, style_rules, tree};

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

// MARK: Layouts

/// The layout a scope's panel draws now, a draft included.
fn effective<'a>(s: &'a State, id: &str) -> &'a Value {
    if let Some(value) = s.layouts.get(id) {
        if !value.is_null() {
            return value;
        }
    }
    if let Some(value) = s.saved.layouts.get(id) {
        if !s.layouts.get(id).is_some_and(Value::is_null) {
            return value;
        }
    }
    shared_effective(s)
}

/// The shared layout as the panels that use it draw it: its draft, else as saved.
fn shared_effective(s: &State) -> &Value {
    s.layouts.get("all").unwrap_or(&s.saved.shared_layout)
}

/// The layout a scope keeps saved: the shared one, or the screen's own, else the
/// shared one it uses.
fn saved_layout<'a>(s: &'a State, id: &str) -> &'a Value {
    if id == "all" {
        return &s.saved.shared_layout;
    }
    s.saved.layouts.get(id).unwrap_or(&s.saved.shared_layout)
}

/// Whether the panel on a screen draws the shared layout now, a draft included.
fn uses_shared(s: &State, id: &str) -> bool {
    match s.layouts.get(id) {
        Some(value) => value.is_null(),
        None => !s.saved.layouts.contains_key(id),
    }
}

fn canonical(layout: &Value) -> Value {
    normalize_layout(layout).unwrap_or_else(|_| json!({}))
}

/// Shows a layout as the shared draft; one that draws what the saved layout draws is
/// the saved layout and clears the draft instead.
fn propose_shared(s: &mut State, asked: Value, present: &[rules::Block]) {
    let current = canonical(&s.saved.shared_layout);
    let proposed = if same(&asked, &rules::reduced(&current, present)) {
        current.clone()
    } else {
        asked
    };
    if same(&proposed, &current) {
        s.layouts.remove("all");
    } else {
        s.layouts.insert("all".into(), proposed);
    }
}

/// What a screen is proposed for a layout sent for it (HUDTools.choice): its saved own
/// layout where the layout draws that; the shared layout where the screen draws the
/// shared layout and the layout draws what that draws, so the layout get_hud returned
/// for such a screen, sent back, changes nothing; else the layout as its own. None is
/// the shared layout.
fn choice(s: &State, id: &str, asked: &Value, present: &[rules::Block]) -> Option<Value> {
    let own = s.saved.layouts.get(id).map(canonical);
    if let Some(own) = &own {
        if same(asked, &rules::reduced(own, present)) {
            return Some(own.clone());
        }
    }
    if (own.is_none() || uses_shared(s, id))
        && same(asked, &rules::reduced(shared_effective(s), present))
    {
        return None;
    }
    Some(asked.clone())
}

/// Shows a choice as a screen's draft: a layout of its own, or the shared layout
/// (None). A choice that is the saved one clears the draft instead.
fn propose_choice(s: &mut State, id: &str, choice: Option<Value>) {
    let current = s.saved.layouts.get(id).map(canonical);
    let unchanged = match (&choice, &current) {
        (None, None) => true,
        (Some(asked), Some(saved)) => same(&canonical(asked), saved),
        _ => false,
    };
    if unchanged {
        s.layouts.remove(id);
    } else {
        s.layouts.insert(id.into(), choice.unwrap_or(Value::Null));
    }
}

/// Shows a proposal's style as a screen's style draft: what it changes, so one that
/// changes nothing, or a screen that is away, drops the draft; the style a waiting
/// draft already lists keeps it.
fn propose_style(s: &mut State, id: &str, style: &style_rules::StyleProposal) {
    let reduced = Value::Object(style.reduced.clone());
    if !style.attached || style.reduced.is_empty() {
        s.styles.remove(id);
    } else if !s
        .styles
        .get(id)
        .is_some_and(|waiting| same(waiting, &reduced))
    {
        s.styles.insert(id.into(), reduced);
    }
}

/// Holds the panel of a scope open while the agent works: the screen's, or for the
/// shared layout the first screen's.
fn hold_for_edit(s: &mut State, id: &str) {
    let screen = if id == "all" {
        s.screens
            .first()
            .and_then(|v| v["id"].as_str())
            .map(str::to_owned)
    } else {
        Some(id.to_owned())
    };
    if let Some(screen) = screen {
        s.holds.insert(screen, now() + HOLD_MS);
    }
}

// MARK: Reading

pub(super) fn app_settings(app: &Value) -> Value {
    let mut v = Map::new();
    for (field, saved, _) in app_rules::SAVED_AS.iter().filter(|(.., global)| !global) {
        if let Some(value) = app.get(*field) {
            v.insert((*saved).into(), value.clone());
        }
    }
    Value::Object(v)
}

pub(super) fn save_global_app(global: &mut Map<String, Value>, app: &Value) {
    for (field, saved, _) in app_rules::SAVED_AS.iter().filter(|(.., global)| *global) {
        let value = match field.strip_prefix("hover.") {
            Some(field) => app["hover"].get(field),
            None => app.get(*field),
        };
        if let Some(value) = value {
            global.insert((*saved).into(), value.clone());
        }
    }
}

/// The app's settings as get_hud lists them: as the app draws them now, its draft
/// included, with the language chosen and the one the panel's names are written in.
fn app_listing(s: &State, snapshot: &Value) -> Value {
    let mut app = app_rules::saved(s, snapshot);
    if let Some(fields) = s.app.as_ref().and_then(Value::as_object) {
        for (k, v) in fields {
            match (app[k].as_object_mut(), v.as_object()) {
                (Some(hover), Some(fields)) if k == "hover" => merge(hover, fields),
                _ => app[k] = v.clone(),
            }
        }
    }
    app["language"] = snapshot["settings"]["language"].clone();
    app["writtenIn"] = json!(if written_in_chinese(&snapshot["settings"]["language"]) {
        "zh-Hans"
    } else {
        "en"
    });
    app
}

/// The blocks get_hud lists, in the panel's order.
fn blocks(snapshot: &Value, layout: &Value) -> Vec<Value> {
    let present = rules::present_blocks(snapshot);
    let mut result = present
        .iter()
        .map(|block| {
            let key = block.key.as_str();
            let kind = match block.kind {
                rules::Kind::Quota => "quota",
                rules::Kind::Balance => "balance",
            };
            let mut group = json!({"key":key,"kind":kind,"accounts":[],"rows":[],"widgets":block.kind.widgets(),"measures":block.kind.measures("number")});
            if block.kind == rules::Kind::Quota {
                for quota in snapshot["quotas"].as_array().into_iter().flatten() {
                    if quota["vendor"].as_str().unwrap_or("unknown") != key {
                        continue;
                    }
                    let mut row = quota.clone();
                    if quota["legacyWindowId"].is_string() {
                        row["id"] = quota["legacyWindowId"].clone();
                    }
                    group["rows"].as_array_mut().unwrap().push(row);
                }
                for (id, _) in &block.accounts {
                    let quota = snapshot["quotas"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|q| {
                            q["vendor"].as_str().unwrap_or("unknown") == key
                                && q["accountId"]
                                    .as_str()
                                    .or_else(|| q["account"].as_str())
                                    .unwrap_or("unassigned")
                                    == id
                        })
                        .cloned()
                        .unwrap_or(Value::Null);
                    group["accounts"].as_array_mut().unwrap().push(
                        json!({"id":id,"name":quota["account"],"plan":quota["plan"]}),
                    );
                }
            } else {
                for balance in snapshot["billing"].as_array().into_iter().flatten() {
                    if format!("api:{}", balance["vendor"].as_str().unwrap_or("unknown")) == key {
                        group["accounts"].as_array_mut().unwrap().push(balance.clone());
                    }
                }
            }
            group["settings"] = layout["blocks"][key].clone();
            group["hidden"] = json!(layout["blocks"][key]["hidden"] == true);
            group
        })
        .collect::<Vec<_>>();
    let order = strings(&layout["order"]);
    result.sort_by_key(|row| {
        order
            .iter()
            .position(|key| row["key"] == *key)
            .unwrap_or(usize::MAX)
    });
    result
}

fn listing(
    s: &mut State,
    id: &str,
    snapshot: &Value,
    catalog: Option<&Value>,
    note: Option<&str>,
) -> Value {
    let layout = effective(s, id).clone();
    let mut view = Presentation::view_of(s);
    let mut screens = s.screens.clone();
    for screen in &mut screens {
        let key = screen["id"].as_str().unwrap_or("").to_owned();
        screen["key"] = json!(key);
        let style = style_rules::effective(s, &key, screen["hasNotch"] == true);
        screen["hud"] = style_rules::listing(&style);
        screen["held"] = json!(s.holds.get(&key).is_some_and(|until| *until > now()));
        screen["holdSecondsLeft"] = json!(s
            .holds
            .get(&key)
            .map(|until| ((until - now()).max(0) + 999) / 1000)
            .unwrap_or(0));
    }
    view["screen"] = json!(id);
    view["layout"] = layout.clone();
    view["panel"] = tree::tree(&layout, &rules::present_blocks(snapshot));
    view["catalog"] = tree::catalog();
    view["blocks"] = json!(blocks(snapshot, &layout));
    view["app"] = app_listing(s, snapshot);
    view["appUnsaved"] = json!(s.app.is_some());
    view["unsaved"] = json!(s.layouts.contains_key(id) || s.styles.contains_key(id));
    view["screens"] = json!(screens);
    view["sessions"] = json!({"offeredAgents":rules::session_agents(snapshot),"offeredProjects":rules::offered_projects(snapshot).into_iter().map(|id|json!({"name":rules::project_title(&id),"id":id})).collect::<Vec<_>>(),"rows":snapshot["sessions"],"filter":layout["sessions"]});
    view["tokens"] = json!({"filter":layout["tokens"],"total":snapshot["totals"]});
    match catalog {
        Some(catalog) if catalog["error"].is_null() => {
            view["tokens"]["offeredModels"] = catalog["available"]["models"].clone();
            view["tokens"]["offeredAgents"] = catalog["available"]["providers"].clone();
        }
        Some(catalog) => {
            view["tokens"]["catalogNotice"] = catalog["error"].clone();
            view["tokens"]["offeredModels"] = json!([]);
            view["tokens"]["offeredAgents"] = json!([]);
        }
        None => {}
    }
    if let Some(note) = note {
        view["note"] = json!(note);
    }
    view
}

// MARK: Proposing

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

/// What a proposal did to a scope, as the agent is told it (HUDTools.proposalNote).
fn proposal_note(s: &State, id: &str) -> String {
    if id == "all" {
        if !s.layouts.contains_key("all") {
            return SAVED_NOTE.into();
        }
        let own: Vec<String> = s
            .screens
            .iter()
            .filter_map(|v| v["id"].as_str())
            .filter(|key| !uses_shared(s, key))
            .filter_map(|key| role(s, key))
            .collect();
        if own.is_empty() {
            return DRAFT_NOTE.into();
        }
        return format!(
            "{DRAFT_NOTE} Screens with a layout of their own do not show it: {}.",
            quoted(&own)
        );
    }
    let name = screen_name(s, id, false);
    let what = |draft: &Value| {
        if draft.is_null() {
            "an unsaved draft that uses the shared layout"
        } else {
            "an unsaved draft of its own"
        }
    };
    if s.styles.contains_key(id) {
        let dropped =
            "and the style also when the user moves that HUD or changes that display in Settings.";
        let both = format!("The user keeps both by hovering the Agent HUD on that display and pressing Save in the panel; they are dropped when Agent HUD restarts, {dropped}");
        if let Some(draft) = s.layouts.get(id) {
            return format!(
                "Shown on {name} as {}, with its resting HUD's style as an unsaved draft too. {both}",
                what(draft)
            );
        }
        if uses_shared(s, id) && s.layouts.contains_key("all") {
            return format!("The resting HUD of {name} shows its style as an unsaved draft, and its panel the shared layout's unsaved draft. {both}");
        }
        return format!("The resting HUD of {name} shows its style as an unsaved draft; its panel's layout is the one it has saved. The user keeps the style by hovering the Agent HUD on that display and pressing Save in the panel; it is dropped when Agent HUD restarts, {dropped}");
    }
    let Some(draft) = s.layouts.get(id) else {
        return format!("This is what {name} already has saved, so there is no draft for it.");
    };
    if attached(s, id).is_none() {
        return format!("Kept for {name} as {}. It shows in that display's panel if it returns before Agent HUD restarts, which drops it; the user keeps it with Save in that panel.", what(draft));
    }
    format!("Shown in the panel on {name} as {}. The user keeps it by hovering the Agent HUD on that display and pressing Save in the panel; it is dropped when Agent HUD restarts.", what(draft))
}

fn refusal(reasons: Vec<String>) -> String {
    format!("Nothing was changed. {}", reasons.join(" "))
}

/// draft_hud: a layout for a scope, a screen's style, the app's settings, or `from`
/// a version (HUDTools.propose).
fn draft(
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

pub(super) fn panel_key(key: &str) -> bool {
    [
        "panelLayout",
        "sessionList",
        "quotaDetails",
        "tokenUsage",
        "resetCountdown",
        "panelWidth",
        "layoutDensity",
        "panelSections",
        "panelOrder",
    ]
    .contains(&key)
}
pub(super) fn panel_patch(layout: &mut Value, patch: &Map<String, Value>) {
    if let Some(value) = patch.get("panelLayout").filter(|v| v.is_object()) {
        *layout = normalize_layout(value).unwrap_or(json!({}));
    }
    if !layout.is_object() {
        *layout = json!({});
    }
    for (key, section) in [
        ("sessionList", "sessions"),
        ("quotaDetails", "blocks"),
        ("tokenUsage", "tokens"),
    ] {
        if let Some(enabled) = patch.get(key).and_then(Value::as_bool) {
            let mut hidden = strings(&layout["hiddenSections"]);
            hidden.retain(|s| s != section);
            if !enabled {
                hidden.push(section.into());
            }
            layout["hiddenSections"] = json!(hidden);
        }
    }
    if let Some(value) = patch.get("resetCountdown").filter(|v| v.is_boolean()) {
        layout["countdown"] = value.clone();
    }
    for key in ["panelWidth", "layoutDensity"] {
        if let Some(value) = patch.get(key) {
            if !layout["style"].is_object() {
                layout["style"] = json!({});
            }
            if key == "panelWidth" {
                layout["style"]["width"] = value.clone();
            } else if let Some(density) = value.as_str() {
                layout["style"]["spacing"] = json!(match density {
                    "comfortable" => "roomy",
                    "compact" => "compact",
                    _ => "regular",
                });
            }
        }
    }
    if let Some(value) = patch.get("panelSections") {
        layout["sections"] = value.clone();
    }
    if let Some(value) = patch.get("panelOrder") {
        layout["order"] = value.clone();
    }
}

/// The language the app is shown in: the system's when the setting is
/// "system" (the default), as agent-hud-native-open resolves it.
fn written_in_chinese(language: &Value) -> bool {
    match language.as_str().unwrap_or("system") {
        "system" => agent_hud_core::l10n::system_chinese(),
        language => language.starts_with("zh"),
    }
}
