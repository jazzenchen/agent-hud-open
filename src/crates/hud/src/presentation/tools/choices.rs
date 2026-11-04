//! The drafts a proposal leaves: the layout a scope draws and keeps, a layout or a
//! style shown as a draft, the panel held open, and what the agent is told it did.
use super::*;

// MARK: Layouts

/// The layout a scope's panel draws now, a draft included.
pub(super) fn effective<'a>(s: &'a State, id: &str) -> &'a Value {
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
pub(super) fn saved_layout<'a>(s: &'a State, id: &str) -> &'a Value {
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

pub(super) fn canonical(layout: &Value) -> Value {
    normalize_layout(layout).unwrap_or_else(|_| json!({}))
}

/// Shows a layout as the shared draft; one that draws what the saved layout draws is
/// the saved layout and clears the draft instead.
pub(super) fn propose_shared(s: &mut State, asked: Value, present: &[rules::Block]) {
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
pub(super) fn choice(
    s: &State,
    id: &str,
    asked: &Value,
    present: &[rules::Block],
) -> Option<Value> {
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
pub(super) fn propose_choice(s: &mut State, id: &str, choice: Option<Value>) {
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
pub(super) fn propose_style(s: &mut State, id: &str, style: &style_rules::StyleProposal) {
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
pub(super) fn hold_for_edit(s: &mut State, id: &str) {
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

/// What a proposal did to a scope, as the agent is told it (HUDTools.proposalNote).
pub(super) fn proposal_note(s: &State, id: &str) -> String {
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
