//! Points spent over the last days, from TeleAgent's usage records.
use super::{
    label_text, number, BASE, HISTORY_DAYS, OVERVIEW, PAGE_LIMIT, PAGE_SIZE, RECORDS, SPEND_DAYS,
};
use crate::account::{Host, HttpFailure, HttpRequest};
use serde_json::{json, Value};
use std::time::Duration;

pub(super) struct Spend {
    pub(super) per_day: Option<f64>,
    pub(super) days: Vec<Value>,
}

/// Usage records summed by day. The 7-day pace prefers overview `pointsUsed`.
pub(super) fn recent_spend(host: &dyn Host, token: &str, os: &str) -> Spend {
    let today = chrono::Local::now().date_naive();
    let history_start = today - chrono::Duration::days(HISTORY_DAYS - 1);
    let pace_start = today - chrono::Duration::days(SPEND_DAYS - 1);
    let mut by_day = std::collections::BTreeMap::<chrono::NaiveDate, f64>::new();
    let mut fetched = 0i64;
    for page in 1..=PAGE_LIMIT {
        let url = format!(
            "{BASE}{RECORDS}?startDate={}&endDate={}&page={page}&pageSize={PAGE_SIZE}",
            history_start.format("%Y-%m-%d"),
            today.format("%Y-%m-%d")
        );
        let Ok(body) = ask(host, token, os, &url) else {
            break;
        };
        let Some(payload) = spend_payload(&body) else {
            break;
        };
        let Some(list) = payload["list"]
            .as_array()
            .or_else(|| payload["rows"].as_array())
        else {
            break;
        };
        if list.is_empty() {
            break;
        }
        for item in list {
            let Some(day) = record_day(&item["statTime"]) else {
                continue;
            };
            let points = number(&item["totalUsage"]).unwrap_or_else(|| {
                number(&item["modelUsage"]).unwrap_or(0.0)
                    + number(&item["toolUsage"]).unwrap_or(0.0)
            });
            *by_day.entry(day).or_default() += points;
        }
        fetched += list.len() as i64;
        let total = payload["total"].as_i64().or_else(|| {
            payload["total"]
                .as_f64()
                .filter(|n| n.is_finite())
                .map(|n| n as i64)
        });
        if list.len() < PAGE_SIZE as usize || total.is_some_and(|total| fetched >= total) {
            break;
        }
    }
    let days = if by_day.is_empty() {
        Vec::new()
    } else {
        (0..HISTORY_DAYS)
            .map(|offset| {
                let day = history_start + chrono::Duration::days(offset);
                json!({
                    "date": day.format("%Y-%m-%d").to_string(),
                    "points": by_day.get(&day).copied().unwrap_or(0.0),
                })
            })
            .collect()
    };
    let per_day = overview_per_day(host, token, os, pace_start, today).or_else(|| {
        if by_day.is_empty() {
            return None;
        }
        let mut sum = 0.0;
        let mut day = pace_start;
        while day <= today {
            sum += by_day.get(&day).copied().unwrap_or(0.0);
            day += chrono::Duration::days(1);
        }
        Some(sum / SPEND_DAYS as f64)
    });
    Spend { per_day, days }
}

fn overview_per_day(
    host: &dyn Host,
    token: &str,
    os: &str,
    start: chrono::NaiveDate,
    today: chrono::NaiveDate,
) -> Option<f64> {
    let url = format!(
        "{BASE}{OVERVIEW}?startDate={}&endDate={}",
        start.format("%Y-%m-%d"),
        today.format("%Y-%m-%d")
    );
    let body = ask(host, token, os, &url).ok()?;
    let payload = spend_payload(&body)?;
    let used =
        number(&payload["pointsUsed"]).or_else(|| number(&payload["summary"]["pointsUsed"]))?;
    Some(used / SPEND_DAYS as f64)
}

fn ask(host: &dyn Host, token: &str, os: &str, url: &str) -> Result<Value, HttpFailure> {
    host.http_json(&HttpRequest {
        url,
        secrets: &[("X-Token", token)],
        headers: &[("X-OS-Type", os)],
        body: None,
        timeout: Duration::from_secs(12),
    })
}

fn spend_payload(body: &Value) -> Option<&Value> {
    if body["code"] == 0 || body["code"] == 200 || body["success"] == true {
        return Some(&body["data"]);
    }
    if body["list"].is_array()
        || body["rows"].is_array()
        || body["pointsUsed"].is_number()
        || body["summary"]["pointsUsed"].is_number()
    {
        return Some(body);
    }
    None
}

fn record_day(value: &Value) -> Option<chrono::NaiveDate> {
    let text = label_text(value)?;
    if text.len() >= 10 {
        if let Ok(day) = chrono::NaiveDate::parse_from_str(&text[..10], "%Y-%m-%d") {
            return Some(day);
        }
    }
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.with_timezone(&chrono::Local).date_naive())
}
