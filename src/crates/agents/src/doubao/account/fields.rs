//! Small readers shared by the quota parser and the usage list.
use chrono::{DateTime, Utc};
use serde_json::Value;

pub(super) fn payload(body: &Value) -> Option<&Value> {
    let code = body.get("code");
    let ok = match code {
        Some(Value::Number(n)) => n.as_i64() == Some(0) || n.as_i64() == Some(200),
        Some(Value::String(text)) => text == "0" || text == "200",
        Some(Value::Null) | None => body.get("success") == Some(&Value::Bool(true)),
        _ => false,
    } || body.get("success") == Some(&Value::Bool(true));
    ok.then(|| body.get("data")).flatten()
}

pub(super) fn first_number(object: &serde_json::Map<String, Value>, names: &[&str]) -> Option<f64> {
    names
        .iter()
        .find_map(|name| object.get(*name).and_then(number))
}

pub(super) fn first_instant(
    object: &serde_json::Map<String, Value>,
    names: &[&str],
) -> Option<String> {
    names.iter().find_map(|name| {
        let value = object.get(*name)?;
        value
            .as_str()
            .and_then(normalize_time)
            .or_else(|| value.as_i64().and_then(instant))
            .or_else(|| value.as_f64().and_then(|n| instant(n as i64)))
    })
}

pub(super) fn first_text(
    object: &serde_json::Map<String, Value>,
    names: &[&str],
) -> Option<String> {
    names.iter().find_map(|name| {
        object
            .get(*name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    })
}

pub(super) fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
        .filter(|n| n.is_finite() && *n >= 0.0)
}

pub(super) fn normalize_time(text: &str) -> Option<String> {
    if let Ok(time) = DateTime::parse_from_rfc3339(text) {
        return Some(time.with_timezone(&Utc).to_rfc3339());
    }
    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(time) = chrono::NaiveDateTime::parse_from_str(text, format) {
            if let Some(local) = time.and_local_timezone(chrono::Local).single() {
                return Some(local.to_rfc3339());
            }
        }
    }
    chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .and_then(|time| time.and_local_timezone(chrono::Local).single())
        .map(|time| time.to_rfc3339())
        .or_else(|| text.parse::<i64>().ok().and_then(instant))
}

pub(super) fn instant(raw: i64) -> Option<String> {
    let millis = if raw.abs() < 10_000_000_000 {
        raw.saturating_mul(1000)
    } else {
        raw
    };
    DateTime::from_timestamp_millis(millis).map(|time| time.to_rfc3339())
}
