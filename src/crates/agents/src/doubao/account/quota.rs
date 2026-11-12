//! The personal subscription and the activity gift, as two quota rows.
use super::fields::{first_text, payload};
use super::window::{collect, is_gift, period_seconds, period_short, window_of, Window};
use crate::account::{account_id, status, Reading, Unread};
use crate::l10n;
use chrono::Utc;
use serde_json::{json, Value};

pub(super) fn parse(summary: &Value, reward: &Value) -> Result<Reading, Unread> {
    let summary_body = payload(summary).unwrap_or(summary);
    let reward_body = payload(reward).unwrap_or(reward);
    // The live payload names two buckets: `window_limit_section` is the
    // personal subscription, and reward `entitlements` are the activity gift.
    let mut subscription = section_windows(&summary_body["window_limit_section"]);
    let mut gift = entitlement_windows(reward_body.get("entitlements"));
    if gift.is_empty() {
        gift = section_windows(&summary_body["incentive_quota_section"]);
    }
    if subscription.is_empty() && gift.is_empty() {
        let mut found = Vec::new();
        collect(summary_body, &mut found);
        let (legacy, summary_gift): (Vec<_>, Vec<_>) = found
            .into_iter()
            .partition(|window| !window.period.as_deref().is_some_and(is_gift));
        subscription = legacy;
        gift = Vec::new();
        collect(reward_body, &mut gift);
        if gift.is_empty() {
            gift = summary_gift;
        }
    }
    gift = one_gift_pool(gift);
    if subscription.is_empty() && gift.is_empty() {
        return Err(Unread::Failed(l10n::text(
            "豆包返回的额度格式无法识别",
            "Doubao's quota has an unsupported format",
        )));
    }
    let id = account_id("doubao", &["doubao.com"]);
    let observed = Utc::now().to_rfc3339();
    let mut reading = Reading::empty("doubao");
    let plan = plan_name(summary_body).or_else(|| plan_name(reward_body));
    push_windows(
        &mut reading,
        &id,
        &observed,
        &subscription,
        true,
        plan.as_deref(),
    );
    push_windows(&mut reading, &id, &observed, &gift, false, plan.as_deref());
    reading.account = Some(json!({
        "id": id,
        "provider": "doubao",
        "label": null,
        "plan": plan,
        "checkedAt": observed,
        "signIn": "app",
    }));
    Ok(reading)
}

fn push_windows(
    reading: &mut Reading,
    id: &str,
    observed: &str,
    windows: &[Window],
    subscription: bool,
    plan: Option<&str>,
) {
    for (index, window) in windows.iter().enumerate() {
        let (name, short) = if subscription {
            subscription_names(window.period.as_deref(), windows.len())
        } else if windows.len() == 1 {
            (
                l10n::text("活动赠送额度", "Activity gift"),
                l10n::text("赠送", "Gift"),
            )
        } else {
            (
                l10n::text(
                    &format!("活动赠送额度 {}", index + 1),
                    &format!("Activity gift {}", index + 1),
                ),
                l10n::text(
                    &format!("赠送{}", index + 1),
                    &format!("Gift {}", index + 1),
                ),
            )
        };
        let key = if subscription { "subscription" } else { "gift" };
        let mut row = json!({
            "id": format!("{id}:{key}:{index}"),
            "vendor": "doubao",
            "consumerSource": "doubao",
            "accountId": id,
            "account": plan,
            "name": name,
            "shortLabel": short,
            "usedPercent": window.percent,
            "resetAt": window.reset,
            "windowDurationSeconds": window.duration,
            "allModels": true,
            "observedAt": observed,
            "status": status(window.percent),
        });
        if window.has_capacity {
            row["nativeUsage"] = json!({
                "used": window.used,
                "capacity": window.total,
                "unit": "credits",
            });
        }
        reading.quotas.push(row);
    }
}

/// Several activity grants are one gift pool: their amounts add, and the
/// soonest expiry is the reset.
fn one_gift_pool(windows: Vec<Window>) -> Vec<Window> {
    if windows.len() <= 1 || windows.iter().any(|window| !window.has_capacity) {
        return windows;
    }
    let used = windows.iter().map(|window| window.used).sum();
    let total = windows.iter().map(|window| window.total).sum();
    let reset = windows.into_iter().filter_map(|window| window.reset).min();
    vec![Window {
        used,
        total,
        has_capacity: total > 0.0,
        percent: if total > 0.0 {
            (used / total * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        },
        period: None,
        reset,
        duration: None,
    }]
}

fn section_windows(section: &Value) -> Vec<Window> {
    let mut out = Vec::new();
    let Some(groups) = section["window_limit_groups"].as_array() else {
        return out;
    };
    for group in groups {
        let title = group["feature_group_name"]
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty());
        let Some(limits) = group["window_limits"].as_array() else {
            continue;
        };
        for limit in limits {
            let Some(object) = limit.as_object() else {
                continue;
            };
            let Some(mut window) = window_of(object) else {
                continue;
            };
            if window.duration.is_none() {
                if let Some(title) = title {
                    if is_gift(title) {
                        window.period = Some(title.to_owned());
                    } else if let Some(seconds) = period_seconds(title) {
                        window.period = Some(title.to_owned());
                        window.duration = Some(seconds);
                    }
                }
            }
            out.push(window);
        }
    }
    out
}

fn entitlement_windows(list: Option<&Value>) -> Vec<Window> {
    let mut out = Vec::new();
    let Some(items) = list.and_then(Value::as_array) else {
        return out;
    };
    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
        let Some(mut window) = window_of(object) else {
            continue;
        };
        if let Some(title) = item["display"]["title"].as_str() {
            if is_gift(title) {
                window.period = Some(title.to_owned());
            }
        }
        out.push(window);
    }
    out
}

fn subscription_names(period: Option<&str>, count: usize) -> (String, String) {
    let short = period.and_then(period_short).filter(|_| count > 1);
    if let Some(short) = short {
        (
            l10n::text(
                &format!("个人订阅 · {short}"),
                &format!("Personal subscription · {short}"),
            ),
            short,
        )
    } else {
        (
            l10n::text("个人订阅", "Personal subscription"),
            l10n::text("订阅", "Plan"),
        )
    }
}

fn plan_name(summary: &Value) -> Option<String> {
    let mut found = None;
    find_plan(summary, &mut found);
    found
}

fn find_plan(value: &Value, found: &mut Option<String>) {
    if found.is_some() {
        return;
    }
    let Some(object) = value.as_object() else {
        if let Some(items) = value.as_array() {
            for item in items {
                find_plan(item, found);
            }
        }
        return;
    };
    if let Some(name) = first_text(
        object,
        &[
            "plan_display_name",
            "plan_name",
            "short_name",
            "package_name",
            "sku_name",
            "display_name",
        ],
    ) {
        if !name.chars().all(|c| c.is_ascii_digit()) {
            *found = Some(name);
            return;
        }
    }
    for child in object.values() {
        find_plan(child, found);
    }
}
