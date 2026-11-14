//! WorkBuddy's plan ceilings: the 429 answers that say a model's window is
//! spent, when it resets, and how the window is named.
use crate::transcript::text;
use chrono::{FixedOffset, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkBuddyLimitHit {
    pub model: String,
    pub route: Option<String>,
    pub window: String,
    pub reset_ms: i64,
    pub observed_ms: i64,
}

impl WorkBuddyLimitHit {
    pub fn row_id(&self) -> String {
        format!("limit:{}:{}", self.model, self.window)
    }
}

fn error_status(error: &Value) -> Option<i64> {
    error["status"]
        .as_i64()
        .or_else(|| {
            error["status"]
                .as_f64()
                .filter(|f| f.is_finite() && *f == f.round())
                .map(|f| f as i64)
        })
        .or_else(|| error["status"].as_str().and_then(|s| s.parse::<i64>().ok()))
}

pub fn parse_beijing_reset_ms(message: &str) -> Option<i64> {
    let suffix_idx = message.find(" 重置")?; // l10n-ignore vendor message match
    if suffix_idx < 19 {
        return None;
    }
    let slice = message.get(suffix_idx - 19..suffix_idx)?;
    let naive = NaiveDateTime::parse_from_str(slice, "%Y-%m-%d %H:%M:%S").ok()?;
    let tz = FixedOffset::east_opt(8 * 3600)?;
    Some(tz.from_local_datetime(&naive).single()?.timestamp_millis())
}

pub fn parse_limit_hit(
    v: &Value,
    observed_ms: Option<i64>,
    last_call: Option<(&str, Option<&str>)>,
) -> Option<WorkBuddyLimitHit> {
    let observed_ms = observed_ms?;
    let (model, route) = last_call?;
    if v["status"].as_str() != Some("incomplete") {
        return None;
    }
    let error = &v["providerData"]["error"];
    if error_status(error) != Some(429) {
        return None;
    }
    let message = text(&error["message"])?;
    let window = [
        ("5h", "5 小时"),  // l10n-ignore vendor message match
        ("5h", "5小时"),   // l10n-ignore vendor message match
        ("day", "每日"),   // l10n-ignore vendor message match
        ("week", "每周"),  // l10n-ignore vendor message match
        ("month", "每月"), // l10n-ignore vendor message match
    ]
    .into_iter()
    .find_map(|(w, phrase)| message.contains(phrase).then_some(w))?;
    let reset_ms = parse_beijing_reset_ms(message)?;
    Some(WorkBuddyLimitHit {
        model: model.to_owned(),
        route: route.filter(|r| !r.is_empty()).map(str::to_owned),
        window: window.to_owned(),
        reset_ms,
        observed_ms,
    })
}

pub fn window_label(window: &str) -> &'static str {
    match window {
        "5h" => "5-hour usage limit",
        "day" => "Daily usage limit",
        "week" => "Weekly usage limit",
        _ => "Monthly usage limit",
    }
}

pub fn window_duration_seconds(window: &str) -> f64 {
    match window {
        "5h" => 5.0 * 3600.0,
        "day" => 86_400.0,
        "week" => 7.0 * 86_400.0,
        _ => 30.0 * 86_400.0,
    }
}
