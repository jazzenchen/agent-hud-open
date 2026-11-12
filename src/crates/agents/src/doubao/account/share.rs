//! The share of a quota window inside a usage row.
use super::fields::number;
use serde_json::{Map, Value};

/// A share of the quota window: a percent field, or text like `0.11%` / `<0.01%`.
pub(super) fn share_in(object: &Map<String, Value>) -> Option<f64> {
    let mut found = None;
    let mut below = false;
    for (key, child) in object {
        walk_share(key, child, &mut found, &mut below);
        if found.is_some() {
            break;
        }
    }
    found
        .or(below.then_some(0.005))
        .or_else(|| object.get("quota_source").and_then(loose_share))
}

/// A small number under the quota source, when the response never names it
/// as a percent. Codes, ids and timestamps are not a share of the window.
fn loose_share(value: &Value) -> Option<f64> {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let name = key.to_ascii_lowercase();
                let skip = name.contains("code")
                    || name.contains("id")
                    || name.contains("type")
                    || name.contains("level")
                    || name.contains("status")
                    || name.contains("time")
                    || name.ends_with("_ms");
                if skip {
                    if let Some(n) = loose_share(child) {
                        return Some(n);
                    }
                    continue;
                }
                if let Some(n) = number(child).filter(|n| *n <= 100.0) {
                    return Some(n);
                }
                if let Some(n) = loose_share(child) {
                    return Some(n);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(loose_share),
        _ => None,
    }
}

fn walk_share(key: &str, value: &Value, found: &mut Option<f64>, below: &mut bool) {
    if found.is_some() {
        return;
    }
    let name = key.to_ascii_lowercase();
    if name.contains("less_than") && value.as_bool() == Some(true) {
        *below = true;
    }
    if let Some(n) = value.as_str().and_then(percent_text) {
        *found = Some(n);
        return;
    }
    let skip_number = name.contains("time")
        || name.contains("id")
        || name.ends_with("_ms")
        || name.contains("cursor")
        || name.contains("code")
        || name.contains("type")
        || name.contains("level")
        || name.contains("status");
    if !skip_number
        && (name.contains("percent")
            || name.contains("ratio")
            || name.contains("occupy")
            || name.contains("share"))
    {
        if let Some(n) = number(value) {
            *found = Some(n);
            return;
        }
    }
    match value {
        Value::Object(object) => {
            for (child_key, child) in object {
                walk_share(child_key, child, found, below);
                if found.is_some() {
                    return;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                walk_share(key, item, found, below);
                if found.is_some() {
                    return;
                }
            }
        }
        _ => {}
    }
}

pub(super) fn percent_text(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    let below = trimmed.starts_with('<');
    let bare = trimmed.trim_start_matches('<').trim();
    let bare = bare.strip_suffix('%')?.trim();
    let n: f64 = bare.parse().ok()?;
    if !n.is_finite() || !(0.0..=100.0).contains(&n) {
        return None;
    }
    if below {
        return Some((n / 2.0).clamp(0.0001, n));
    }
    Some(n)
}

pub(super) fn round_share(n: f64) -> f64 {
    (n * 10_000.0).round() / 10_000.0
}
