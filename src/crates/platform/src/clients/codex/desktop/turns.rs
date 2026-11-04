//! The turns in a thread's state: its canonical history with the overlay
//! still in flight, the current turn, and which questions were answered.
use super::super::desktop_io::IpcError;
use serde_json::Value;
use std::collections::HashSet;

pub(super) fn turns(state: &Value, include_overlay: bool) -> Result<Vec<&Value>, IpcError> {
    let overlay = match state.get("turns") {
        None => vec![],
        Some(value) => value
            .as_array()
            .ok_or(IpcError::Protocol)?
            .iter()
            .collect::<Vec<_>>(),
    };
    if overlay.iter().any(|turn| !turn.is_object()) {
        return Err(IpcError::Protocol);
    }
    let Some(storage) = state.get("turnHistory").filter(|v| !v.is_null()) else {
        return Ok(overlay);
    };
    if !storage.is_object() {
        return Err(IpcError::Protocol);
    }
    if storage["kind"] != "canonical" {
        return Ok(overlay);
    }
    let history = &storage["history"];
    let islands = history["islands"].as_array().ok_or(IpcError::Protocol)?;
    let entities = history["entitiesByKey"]
        .as_object()
        .ok_or(IpcError::Protocol)?;
    let mut canonical = Vec::new();
    for island in islands {
        for entry in island["entries"].as_array().ok_or(IpcError::Protocol)? {
            let key = entry["value"].as_str().ok_or(IpcError::Protocol)?;
            let turn = entities
                .get(key)
                .filter(|v| v.is_object())
                .ok_or(IpcError::Protocol)?;
            canonical.push(turn);
        }
    }
    let ids = canonical
        .iter()
        .filter_map(|turn| turn["turnId"].as_str())
        .collect::<HashSet<_>>();
    if overlay
        .iter()
        .any(|turn| turn["turnId"].as_str().is_some_and(|id| ids.contains(id)))
    {
        return Err(IpcError::Protocol);
    }
    if include_overlay {
        canonical.extend(overlay);
    }
    Ok(canonical)
}
pub(super) fn current_turn(state: &Value) -> Result<Option<&str>, IpcError> {
    Ok(turns(state, false)?
        .into_iter()
        .rev()
        .find(|turn| {
            !turn["turnId"].is_null()
                || !matches!(turn["status"].as_str(), Some("completed" | "failed"))
        })
        .and_then(|turn| turn["turnId"].as_str()))
}
pub(super) fn answered_ids(item: &Value) -> Vec<String> {
    let content = match item["type"].as_str() {
        Some("userMessage") => &item["content"],
        Some("steeringUserMessage") if item["status"] == "accepted" => &item["input"],
        _ => return vec![],
    };
    let Some(parts) = content
        .as_array()
        .filter(|v| v.len() == 1 && v[0]["type"] == "text")
    else {
        return vec![];
    };
    let Some(text) = parts[0]["text"].as_str() else {
        return vec![];
    };
    let Some(payload) = text
        .trim()
        .strip_prefix("<send_user_message_question_reply>")
        .and_then(|v| v.strip_suffix("</send_user_message_question_reply>"))
    else {
        return vec![];
    };
    let Ok(value) = serde_json::from_str::<Value>(payload) else {
        return vec![];
    };
    let answers = if let Some(values) = value.as_array().filter(|v| !v.is_empty()) {
        values.clone()
    } else if value.is_object() {
        vec![value]
    } else {
        return vec![];
    };
    if answers.iter().any(|v| {
        !v["questionItemId"].is_string() || !v["question"].is_string() || !v["answer"].is_string()
    }) {
        return vec![];
    }
    answers
        .into_iter()
        .filter_map(|v| v["questionItemId"].as_str().map(str::to_owned))
        .collect()
}
