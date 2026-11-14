//! CodeBuddy & CodeBuddy CN resource/enterprise meter parsing.
use super::copilot::{account_json, post_token, CopilotSpec};
use super::summary::{amount, parse_cst_end, round2, REWARD_PACKAGES};
use super::vscdb::BuddyAuth;
use crate::account::{status, Host, Reading, Unread};
use crate::l10n::tr;
use chrono::{DateTime, FixedOffset, Utc};
use serde_json::{json, Value};

#[allow(clippy::too_many_arguments)]
pub(super) fn read_enterprise(
    host: &dyn Host,
    spec: &CopilotSpec,
    base: &str,
    bearer: &str,
    auth: &BuddyAuth,
    id: &str,
    account_label: &str,
) -> Result<Option<Reading>, Unread> {
    let data = post_token(
        host,
        spec,
        &format!("{base}/v2/billing/meter/get-enterprise-user-usage"),
        bearer,
        auth,
        &json!({}),
    )?;
    let inner = data
        .get("Response")
        .and_then(|r| r.get("Data"))
        .or_else(|| data.get("data"))
        .unwrap_or(&data);
    let Some(limit) = signed_amount(inner.get("limit_num").or_else(|| inner.get("limitNum")))
    else {
        return Ok(None);
    };
    let used = signed_amount(
        inner
            .get("used_num")
            .or_else(|| inner.get("usedNum"))
            .or_else(|| inner.get("credit")),
    )
    .unwrap_or(0.0)
    .max(0.0);
    let now = Utc::now();
    let reset = inner
        .get("cycle_end_time")
        .or_else(|| inner.get("cycleEndTime"))
        .or_else(|| inner.get("cycle_reset_time"))
        .or_else(|| inner.get("cycleResetTime"))
        .and_then(|v| v.as_str())
        .and_then(|s| parse_cst_end(s, now))
        .map(|at| at.to_rfc3339());
    let mut reading = Reading::empty(spec.provider);
    if limit > 0.0 {
        let share = (used / limit * 100.0).clamp(0.0, 100.0);
        reading.quotas.push(json!({
            "id": format!("{id}:credits"),
            "vendor": spec.provider,
            "consumerSource": spec.provider,
            "accountId": id,
            "account": account_label,
            "name": "Enterprise credits",
            "shortLabel": "Credits",
            "usedPercent": share,
            "resetAt": reset,
            "allModels": true,
            "status": status(share),
            "nativeUsage": {"used": round2(used), "capacity": round2(limit), "unit": "credits"},
        }));
    }
    reading.account = Some(account_json(
        spec,
        id,
        account_label,
        Some("Enterprise"),
        &auth.email,
        now,
    ));
    Ok(Some(reading))
}

pub(super) fn read_codebuddy_resources(
    host: &dyn Host,
    spec: &CopilotSpec,
    base: &str,
    bearer: &str,
    auth: &BuddyAuth,
    id: &str,
    account_label: &str,
) -> Result<Reading, Unread> {
    let now = Utc::now();
    let china = FixedOffset::east_opt(8 * 3600).unwrap();
    let cst_now = now.with_timezone(&china);
    let end_range = (cst_now + chrono::Duration::days(3650))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let data = post_token(
        host,
        spec,
        &format!("{base}/v2/billing/meter/get-user-resource"),
        bearer,
        auth,
        &json!({
            "PageNumber": 1,
            "PageSize": 100,
            "ProductCode": "p_tcaca",
            "Status": [0, 3],
            "PackageEndTimeRangeBegin": cst_now.format("%Y-%m-%d %H:%M:%S").to_string(),
            "PackageEndTimeRangeEnd": end_range,
        }),
    )?;
    let accounts = data
        .pointer("/Response/Data/Accounts")
        .or_else(|| data.get("Accounts"))
        .or_else(|| data.get("list"))
        .and_then(Value::as_array)
        .ok_or_else(|| format_err(spec.label))?;

    let (mut total, mut left, mut plan_name, mut earliest) =
        (0.0, 0.0, None, None::<DateTime<Utc>>);
    for item in accounts {
        let code = item["PackageCode"].as_str().unwrap_or_default();
        if code == "TCACA_code_009_0XmEQc2xOf" {
            continue;
        }
        let cap = item_amount(
            item,
            &[
                "CycleCapacitySizePrecise",
                "CycleCapacitySize",
                "CapacitySize",
                "CycleTotalCapacity",
            ],
        );
        let rem = item_amount(
            item,
            &[
                "CycleCapacityRemainPrecise",
                "CycleCapacityRemain",
                "CapacityRemain",
                "CycleRemainCapacity",
            ],
        );
        let Some(cap) = cap else { continue };
        let rem = rem.unwrap_or_else(|| {
            let used = item_amount(
                item,
                &[
                    "CycleCapacityUsedPrecise",
                    "CycleCapacityUsed",
                    "CapacityUsed",
                ],
            )
            .unwrap_or(0.0);
            (cap - used).max(0.0)
        });
        total += cap;
        left += rem.min(cap);
        if plan_name.is_none() && !REWARD_PACKAGES.contains(&code) {
            plan_name = item["PackageName"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
        }
        if !REWARD_PACKAGES.contains(&code) {
            if let Some(end_str) = item["CycleEndTime"]
                .as_str()
                .or_else(|| item["PackageEndTime"].as_str())
            {
                if let Some(end) = parse_cst_end(end_str, now) {
                    earliest = Some(earliest.map_or(end, |prev| prev.min(end)));
                }
            }
        }
    }
    let mut reading = Reading::empty(spec.provider);
    if total > 0.0 {
        let used = round2((total - left).max(0.0));
        let cap = round2(total);
        let share = (used / cap * 100.0).clamp(0.0, 100.0);
        reading.quotas.push(json!({
            "id": format!("{id}:credits"),
            "vendor": spec.provider,
            "consumerSource": spec.provider,
            "accountId": id,
            "account": account_label,
            "name": "Credits",
            "shortLabel": "Credits",
            "usedPercent": share,
            "resetAt": earliest.map(|at| at.to_rfc3339()),
            "allModels": true,
            "status": status(share),
            "nativeUsage": {"used": used, "capacity": cap, "unit": "credits"},
        }));
    } else {
        reading.notice = Some(tr!(
            "{{name}} reports no credit allowance for this account.",
            name = spec.label
        ));
    }
    reading.account = Some(account_json(
        spec,
        id,
        account_label,
        plan_name.as_deref(),
        &auth.email,
        now,
    ));
    Ok(reading)
}

fn item_amount(item: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|k| amount(&item[*k]))
}

fn signed_amount(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|n| n.is_finite())
}

fn format_err(label: &str) -> Unread {
    Unread::from(tr!(
        "{{name}}'s credit usage has an unsupported format.",
        name = label
    ))
}
