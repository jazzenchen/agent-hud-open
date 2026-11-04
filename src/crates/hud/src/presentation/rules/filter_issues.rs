//! The sessions', the token chart's and the resting HUD's agents a proposal names
//! that are not there now.
use super::marks::{collapse, merged_by_mark, source_id};
use super::*;

/// The agents whose sessions the panel lists now, the names `sessions.agents` takes.
pub(in crate::presentation) fn session_agents(snapshot: &Value) -> Vec<String> {
    let mut agents: Vec<String> = vec![];
    for session in snapshot["sessions"].as_array().into_iter().flatten() {
        if let Some(source) = session["source"].as_str() {
            push_unique(&mut agents, source);
        }
    }
    agents.sort();
    agents
}

/// Every agent of `sessions.agents` that no session has now, of `tokens.agents` that
/// no model in the token chart has now, every model of `tokens.models` the chart
/// cannot draw now, and every one whose agent `tokens.agents` leaves out
/// (PanelLayout.filterIssues). An agent or a model a `held` layout already lists is
/// no issue. Without the chart's catalog, its agents and models are not checked.
pub(in crate::presentation) fn filter_issues(
    document: &Value,
    snapshot: &Value,
    catalog: Option<&Value>,
    held: &[Value],
) -> Vec<String> {
    let mut found = vec![];
    let normalize = |v: &str| collapse(v, |c| c == ' ' || c == '_');
    let with_sessions = session_agents(snapshot);
    let offered = if with_sessions.is_empty() {
        "No agent has a session now.".to_owned()
    } else {
        format!("Agents with sessions now: {}.", quoted(&with_sessions))
    };
    let kept: Vec<String> = held
        .iter()
        .flat_map(|l| strings(&l["sessions"]["agents"]))
        .map(|a| source_id(snapshot, &a))
        .collect();
    for (index, agent) in entries(&document["sessions"]["agents"]) {
        let id = source_id(snapshot, agent);
        if agent.is_empty()
            || with_sessions.iter().any(|s| normalize(s) == normalize(&id))
            || kept.contains(&id)
        {
            continue;
        }
        found.push(format!(
            "sessions.agents[{index}]: \"{agent}\" has no session now. {offered}"
        ));
    }
    let Some(catalog) = catalog else {
        return found;
    };
    let ids = |list: &Value| -> Vec<String> {
        list.as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["id"].as_str().map(str::to_owned))
            .collect()
    };
    let agents = ids(&catalog["available"]["providers"]);
    let offered = if agents.is_empty() {
        "The chart has no agent's tokens now.".to_owned()
    } else {
        format!("Agents in the chart now: {}.", quoted(&agents))
    };
    let kept: Vec<String> = held
        .iter()
        .flat_map(|l| strings(&l["tokens"]["agents"]))
        .collect();
    for (index, agent) in entries(&document["tokens"]["agents"]) {
        if agent.is_empty() || agents.iter().any(|a| a == agent) || kept.iter().any(|a| a == agent)
        {
            continue;
        }
        found.push(format!(
            "tokens.agents[{index}]: \"{agent}\" has no model in the token chart now. {offered}"
        ));
    }
    let models: Vec<&Value> = catalog["available"]["models"]
        .as_array()
        .into_iter()
        .flatten()
        .collect();
    let mut model_ids: Vec<String> = vec![];
    for model in &models {
        if let Some(id) = model["id"].as_str() {
            push_unique(&mut model_ids, id);
        }
    }
    let in_chart = if model_ids.is_empty() {
        "The chart has no model's tokens now.".to_owned()
    } else {
        format!("Models in the chart now: {}.", quoted(&model_ids))
    };
    let kept: Vec<String> = held
        .iter()
        .flat_map(|l| strings(&l["tokens"]["models"]))
        .collect();
    let named = strings(&document["tokens"]["agents"]);
    for (index, id) in entries(&document["tokens"]["models"]) {
        if id.is_empty() {
            continue;
        }
        let Some(model) = models.iter().find(|m| m["id"] == id) else {
            if !kept.iter().any(|k| k == id) {
                found.push(format!(
                    "tokens.models[{index}]: \"{id}\" is no model in the token chart now. {in_chart}"
                ));
            }
            continue;
        };
        let vendor = model["provider"].as_str().unwrap_or("");
        if !named.is_empty() && !named.iter().any(|a| a == vendor) {
            found.push(format!("tokens.models[{index}]: \"{id}\" is {vendor}'s, and tokens.agents leaves {vendor} out, so the chart would not draw it. Name \"{vendor}\" in tokens.agents too, or name no agent."));
        }
    }
    found
}

/// Every agent of `hud.agents` the screens the layout is for cannot show now, matched
/// by mark against `available` (PanelLayout.hudIssues). An agent a `held` layout
/// already lists is no issue.
/// - whose: who cannot show it, such as "the resting HUD of \"Main display\"".
pub(in crate::presentation) fn hud_issues(
    document: &Value,
    available: &[String],
    whose: &str,
    held: &[Value],
) -> Vec<String> {
    let shown: Vec<String> = available.iter().map(|a| mark(a)).collect();
    let kept: Vec<String> = held
        .iter()
        .flat_map(|l| strings(&l["hud"]["agents"]))
        .map(|a| mark(&a))
        .collect();
    let offered = if available.is_empty() {
        "It shows no agent now.".to_owned()
    } else {
        format!(
            "It can show: {}.",
            quoted(&merged_by_mark(available.to_vec()))
        )
    };
    let mut found = vec![];
    for (index, agent) in entries(&document["hud"]["agents"]) {
        let key = mark(agent);
        if agent.is_empty() || shown.contains(&key) || kept.contains(&key) {
            continue;
        }
        found.push(format!(
            "hud.agents[{index}]: \"{agent}\" is not an agent {whose} can show now. {offered}"
        ));
    }
    found
}
