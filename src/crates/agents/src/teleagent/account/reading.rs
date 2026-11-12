//! The points balance and the activity groups TeleAgent reports.
use super::spend::Spend;
use super::{label_text, number, SPEND_DAYS};
use crate::account::{account_id, Reading, Unread};
use crate::l10n;
use serde_json::{json, Value};

pub(super) fn parse(owner: &str, body: &Value, spend: Spend) -> Result<Reading, Unread> {
    let balance = number(&body["current_points_balance"]);
    let activities = body["activities"].as_array();
    if balance.is_none() && activities.map(|rows| rows.is_empty()).unwrap_or(true) {
        return Err(Unread::Failed(l10n::text(
            "TeleAgent 返回的积分格式无法识别",
            "TeleAgent's points have an unsupported format",
        )));
    }
    let id = account_id("teleagent", &[owner]);
    // The plan name is payment info only. Without a separate account name it
    // is not also used as the heading, or the plan is written twice.
    let plan = label_text(&body["plan_display_name"]);
    let label = label_text(&body["account_name"]).or_else(|| label_text(&body["user_name"]));
    let mut reading = Reading::empty("teleagent");
    let groups = activities
        .filter(|rows| !rows.is_empty())
        .map(|rows| {
            let only = rows.len() == 1;
            rows.iter()
                .filter_map(|activity| activity_group(activity, balance.filter(|_| only)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !groups.is_empty() {
        let amount = groups
            .iter()
            .find_map(|group| group["remaining"].as_f64())
            .or(balance)
            .unwrap_or(0.0);
        reading
            .billing
            .push(points_row(&id, label, amount, groups, &spend));
    } else if let Some(balance) = balance {
        reading
            .billing
            .push(points_row(&id, label, balance, Vec::new(), &spend));
    }
    reading.account = Some(json!({
        "id": id,
        "provider": "teleagent",
        "label": label,
        "plan": plan,
    }));
    Ok(reading)
}

/// One activity: its total, what remains, and points about to expire.
/// The percent is the remaining share of the total.
fn activity_group(activity: &Value, fallback_remaining: Option<f64>) -> Option<Value> {
    let total = number(&activity["total_activity_points"]);
    let remaining = number(&activity["remaining_activity_points"]).or(fallback_remaining);
    let percent = percent_number(&activity["usage_percent"]).or_else(|| {
        let (remaining, total) = (remaining?, total?);
        (total > 0.0).then_some(remaining / total * 100.0)
    });
    let grant = grant(activity);
    if total.is_none() && remaining.is_none() && percent.is_none() && grant.is_none() {
        return None;
    }
    Some(json!({
        "total": total,
        "remaining": remaining,
        "percent": percent,
        "grant": grant,
    }))
}

/// `balance` is the points that expire; `expire_days` is how many days remain.
fn grant(activity: &Value) -> Option<Value> {
    let points = number(&activity["balance"]).filter(|points| *points > 0.0);
    let days = number(&activity["expire_days"]).map(|days| days.round() as i64);
    let name = label_text(&activity["name"]).unwrap_or("");
    if points.is_none() && days.is_none() && name.is_empty() {
        return None;
    }
    Some(json!({
        "name": name,
        "points": points,
        "expireDays": days,
    }))
}

fn percent_number(value: &Value) -> Option<f64> {
    label_text(value)
        .and_then(|text| text.trim_end_matches('%').trim().parse::<f64>().ok())
        .or_else(|| number(value))
        .filter(|n| n.is_finite() && *n >= 0.0)
}

fn points_row(
    id: &str,
    account: Option<&str>,
    amount: f64,
    groups: Vec<Value>,
    spend: &Spend,
) -> Value {
    let mut row = json!({
        "id": format!("{id}:points"),
        "vendor": "teleagent",
        "consumerSource": "teleagent",
        "accountId": id,
        "account": account,
        "label": l10n::text("余额", "Balance"),
        "amount": amount,
        "currency": "credits",
    });
    if !groups.is_empty() || spend.per_day.is_some() || !spend.days.is_empty() {
        row["pointsDetail"] = json!({
            "groups": groups,
            "spent": spend.per_day.map(|per_day| json!({"perDay": per_day, "days": SPEND_DAYS})),
            "days": spend.days,
        });
    }
    row
}
