//! Doubao's usage list. A miss leaves the quotas already read.
//! `entries[].usage` carries `occurred_at_ms`, `display_name`, and a nested
//! `quota_source`.
use super::client::{post, refusal};
use super::fields::{first_instant, first_number, first_text, instant, normalize_time, payload};
use super::share::{percent_text, round_share, share_in};
use super::TIMELINE;
use crate::account::{Host, Reading};
use serde_json::{json, Value};

pub(super) fn attach_usage(reading: &mut Reading, host: &dyn Host, cookie: &str) {
    let Ok(response) = post(host, cookie, TIMELINE, Some(&json!({"page_size": 50}))) else {
        return;
    };
    if refusal(&response).is_some() {
        return;
    }
    let records = usage_records(payload(&response).unwrap_or(&response));
    if records.is_empty() {
        return;
    }
    if let Some(account) = reading.account.as_mut() {
        account["usageDetail"] = json!({ "records": records });
    }
}

pub(super) fn usage_records(value: &Value) -> Vec<Value> {
    let mut found = Vec::new();
    collect_usage(value, &mut found);
    found.sort_by(|left, right| right["at"].as_str().cmp(&left["at"].as_str()));
    found.truncate(40);
    found
}

fn collect_usage(value: &Value, out: &mut Vec<Value>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_usage(item, out);
            }
        }
        Value::Object(object) => {
            if let Some(record) = usage_record(object) {
                out.push(record);
                return;
            }
            for child in object.values() {
                collect_usage(child, out);
            }
        }
        _ => {}
    }
}

/// A short label written for a person, such as a scene name, not an id or URL.
fn human_text(object: &serde_json::Map<String, Value>) -> Option<String> {
    object.values().find_map(|value| {
        let text = value.as_str()?.trim();
        let count = text.chars().count();
        ((2..=40).contains(&count) && !text.contains("://") && !text.is_ascii())
            .then(|| text.to_owned())
    })
}

fn any_time(object: &serde_json::Map<String, Value>) -> Option<String> {
    first_instant(
        object,
        &[
            "occurred_at_ms",
            "occurred_at",
            "consume_time",
            "occur_time",
            "event_time",
            "create_time",
            "created_at",
            "stat_time",
            "used_at",
            "usage_time",
            "record_time",
            "timestamp",
            "time",
            "ts",
        ],
    )
    .or_else(|| {
        object.iter().find_map(|(key, value)| {
            let name = key.to_ascii_lowercase();
            if name.contains("end")
                || name.contains("start")
                || name.contains("reset")
                || name.contains("expire")
            {
                return None;
            }
            if !(name.contains("time")
                || name.contains("date")
                || name.contains("occurred")
                || name.ends_with("_at")
                || name.ends_with("_ms")
                || name == "ts")
            {
                return None;
            }
            value
                .as_str()
                .and_then(normalize_time)
                .or_else(|| value.as_i64().and_then(instant))
                .or_else(|| value.as_f64().and_then(|n| instant(n as i64)))
        })
    })
}

fn usage_record(object: &serde_json::Map<String, Value>) -> Option<Value> {
    let at = any_time(object)?;
    let title = first_text(
        object,
        &[
            "scene_name",
            "scene",
            "consume_scene",
            "feature_name",
            "title",
            "name",
            "display_name",
        ],
    )
    .or_else(|| human_text(object));
    let source = source_label(object);
    let model = first_text(object, &["model_name", "model", "model_display_name"]);
    let percent = first_number(
        object,
        &[
            "used_percent",
            "window_percent",
            "occupy_percent",
            "percent",
            "ratio",
        ],
    )
    .or_else(|| share_in(object));
    let amount = first_number(
        object,
        &["used_amount", "consume_amount", "amount", "cost_amount"],
    );
    // A window reset is announced beside the rows. It has a time and a label,
    // and neither a source nor a share, so it is not a consumption.
    if percent.is_none() && amount.is_none() && source.is_none() {
        return None;
    }
    Some(json!({
        "at": at,
        "title": title.unwrap_or_default(),
        "source": source,
        "model": model.filter(|text| text != "-" && text != "—" && text != "--"),
        "percent": percent.map(round_share),
        "amount": amount,
    }))
}

/// `quota_source` is either the label itself or `{display_name, display_text, ...}`.
fn source_label(object: &serde_json::Map<String, Value>) -> Option<String> {
    if let Some(text) = first_text(
        object,
        &[
            "source_name",
            "pool_name",
            "feature_group_name",
            "quota_pool",
            "source",
        ],
    ) {
        return Some(text);
    }
    match object.get("quota_source") {
        Some(Value::String(text)) => label_text(text),
        Some(value) => label_in(value),
        None => None,
    }
}

fn label_text(text: &str) -> Option<String> {
    let text = text.trim();
    let count = text.chars().count();
    if text.is_empty()
        || count > 40
        || text.contains("://")
        || percent_text(text).is_some()
        || text.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some(text.to_owned())
}

fn label_in(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => label_text(text),
        Value::Object(object) => {
            for key in [
                "display_name",
                "plain_text",
                "text",
                "content",
                "default",
                "value",
                "name",
                "title",
                "display_text",
            ] {
                if let Some(child) = object.get(key) {
                    if let Some(text) = label_in(child) {
                        return Some(text);
                    }
                }
            }
            object.values().find_map(label_in)
        }
        Value::Array(items) => items.iter().find_map(label_in),
        _ => None,
    }
}
