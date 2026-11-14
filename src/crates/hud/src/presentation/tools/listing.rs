//! What get_hud lists: the panel's layout and tree, its blocks, the screens and their
//! resting HUDs, the app's settings, and the sessions and tokens the filters offer.
use super::choices::effective;
use super::*;

/// The app's settings as get_hud lists them: as the app draws them now, its draft
/// included, with the language chosen and the one the panel's names are written in.
pub(super) fn app_listing(s: &State, snapshot: &Value) -> Value {
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
    app["writtenIn"] =
        json!(agent_hud_core::l10n::Language::of_setting(&snapshot["settings"]["language"]).code());
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
            let mut group = json!({"key":key,"kind":kind,"accounts":[],"rows":[],"widgets":block.kind.widgets()});
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

pub(super) fn listing(
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
