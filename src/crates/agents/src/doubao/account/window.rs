//! One quota window inside Doubao's subscription or gift payload.
use super::fields::{first_instant, first_number, first_text};
use crate::naming::window_names::Period;
use serde_json::Value;

pub(super) fn period_short(text: &str) -> Option<String> {
    Period::from_seconds(period_seconds(text)).map(|period| period.after_word())
}

pub(super) struct Window {
    pub(super) used: f64,
    pub(super) total: f64,
    pub(super) has_capacity: bool,
    pub(super) percent: f64,
    pub(super) period: Option<String>,
    pub(super) reset: Option<String>,
    pub(super) duration: Option<f64>,
}

pub(super) fn collect(value: &Value, out: &mut Vec<Window>) {
    collect_at(None, value, out);
}

/// A quota may sit in a list, or under a key such as `5h` / `7d`. A parent
/// that only sums its children yields those children.
fn collect_at(key: Option<&str>, value: &Value, out: &mut Vec<Window>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_at(None, item, out);
            }
        }
        Value::Object(object) => {
            if object.values().any(contains_window) {
                for (child_key, child) in object {
                    collect_at(Some(child_key), child, out);
                }
                return;
            }
            if let Some(mut window) = window_of(object) {
                if window.duration.is_none() {
                    if let Some(seconds) = key.and_then(period_seconds) {
                        window.period = key.map(str::to_owned);
                        window.duration = Some(seconds);
                    }
                }
                out.push(window);
            }
        }
        _ => {}
    }
}

fn contains_window(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(contains_window),
        Value::Object(object) => {
            window_of(object).is_some() || object.values().any(contains_window)
        }
        _ => false,
    }
}

pub(super) fn window_of(object: &serde_json::Map<String, Value>) -> Option<Window> {
    let total = first_number(
        object,
        &[
            "total_quota",
            "totalQuota",
            "total_amount",
            "totalAmount",
            "total",
            "limit",
            "capacity",
            "quota",
        ],
    );
    let used = first_number(
        object,
        &[
            "used_quota",
            "usedQuota",
            "used",
            "used_amount",
            "usedAmount",
            "consumed",
            "consume",
        ],
    );
    let remain = first_number(
        object,
        &[
            "remain_quota",
            "remainQuota",
            "remaining_quota",
            "remainingQuota",
            "remain",
            "remaining",
            "left",
            "available",
        ],
    );
    let reported = first_number(
        object,
        &[
            "total_used_percent",
            "used_percent",
            "usedPercent",
            "usage_percent",
        ],
    );
    let (used, total, has_capacity, percent) = match (used, remain, total, reported) {
        (Some(used), _, Some(total), _) if total > 0.0 => {
            (used, total, true, (used / total * 100.0).clamp(0.0, 100.0))
        }
        (_, Some(remain), Some(total), _) if total > 0.0 => {
            let used = (total - remain).max(0.0);
            (used, total, true, (used / total * 100.0).clamp(0.0, 100.0))
        }
        (_, _, _, Some(percent)) => (0.0, 0.0, false, percent.clamp(0.0, 100.0)),
        _ => return None,
    };
    let (period, duration) = window_period(object);
    let reset = first_instant(
        object,
        &[
            "reset_at",
            "resetAt",
            "reset_time",
            "resetTime",
            "refresh_time",
            "refreshTime",
            "expire_at",
            "expireAt",
            "expire_time",
            "expireTime",
            "end_time",
            "endTime",
        ],
    );
    Some(Window {
        used,
        total,
        has_capacity,
        percent,
        period,
        reset,
        duration,
    })
}

/// `window_type` is the quota window. 1, 2 and 3 are the 5-hour, 7-day and
/// monthly windows the client shows.
fn window_period(object: &serde_json::Map<String, Value>) -> (Option<String>, Option<f64>) {
    if let Some(text) = first_text(
        object,
        &["period", "window", "quota_window", "cycle", "cycle_type"],
    ) {
        if let Some(seconds) = period_seconds(&text) {
            return (Some(text), Some(seconds));
        }
    }
    let kind = first_number(object, &["window_type", "windowType"]).map(|n| n as i64);
    let label = match kind {
        Some(1) => Some("5h"),
        Some(2) => Some("7d"),
        Some(3) => Some("1m"),
        _ => None,
    };
    (label.map(str::to_owned), label.and_then(period_seconds))
}

pub(super) fn period_seconds(text: &str) -> Option<f64> {
    let key: String = text
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != '-')
        .flat_map(|c| c.to_lowercase())
        .collect();
    if matches!(
        key.as_str(),
        "5h" | "5hour" | "5hours" | "session" | "5小时"
    ) {
        Some(5.0 * 3600.0)
    } else if matches!(
        key.as_str(),
        "7d" | "7day" | "7days" | "week" | "weekly" | "7天"
    ) {
        Some(7.0 * 86400.0)
    } else if matches!(
        key.as_str(),
        "1m" | "30d" | "month" | "monthly" | "每月" | "1个月"
    ) {
        Some(30.0 * 86400.0)
    } else {
        None
    }
}

pub(super) fn is_gift(text: &str) -> bool {
    let key = text.to_ascii_lowercase();
    key.contains("gift")
        || key.contains("reward")
        || key.contains("activity")
        || key.contains("incentive")
        || text.contains("赠送")
        || text.contains("活动")
}
