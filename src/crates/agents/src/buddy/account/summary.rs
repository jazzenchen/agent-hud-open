//! WorkBuddy's summary/packages meter parser and `www.workbuddy.cn` web fallback.
use crate::account::{account_id, Host, HttpFailure, HttpRequest, Reading, Unread, WebSession};
use crate::l10n;
use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};
use serde_json::{json, Value};
use std::time::Duration;

pub(super) const WEB_HOST: &str = "www.workbuddy.cn";
const WEB_ORIGIN: &str = "https://www.workbuddy.cn";

pub(super) const PAID_PACKAGES: &[&str] = &[
    "TCACA_code_002_AkiJS3ZHF5",
    "TCACA_code_005_maRGyrHhw1",
    "TCACA_code_003_FAnt7lcmRT",
    "TCACA_code_023_4xbGhMrE6q",
    "TCACA_code_026_BaESVICNoi",
    "TCACA_code_027_0FCGVA6vSa",
    "TCACA_code_043_6oIkxT7Z0S",
    "TCACA_code_009_0XmEQc2xOf",
    "TCACA_code_038_OhvqZtiPKr",
    "TCACA_code_036_lupO5WgNdG",
];
pub(super) const FREE_PACKAGES: &[&str] = &[
    "TCACA_code_001_PqouKr6QWV",
    "TCACA_code_008_cfWoLwvjU4",
    "TCACA_code_035_ArVxJcGDsm",
    "TCACA_code_006_DbXS0lrypC",
    "TCACA_code_039_KRcQj7wUat",
    "TCACA_code_040_mi9rCYg46x",
    "TCACA_code_007_nzdH5h4Nl0",
    "TCACA_code_028_NtpWi0jzXs",
    "TCACA_code_037_WxOD3MpI2o",
    "TCACA_code_029_6wCGEWquYy",
    "TCACA_code_030_BjSt89qTvr",
];
pub(super) const REWARD_PACKAGES: &[&str] = &[
    "TCACA_code_007_nzdH5h4Nl0",
    "TCACA_code_029_6wCGEWquYy",
    "TCACA_code_030_BjSt89qTvr",
];

pub(super) fn read_workbuddy_web(host: &dyn Host, session: &WebSession) -> Result<Reading, Unread> {
    let summary = post_web(
        host,
        session,
        "/billing/meter/get-user-resource-summary",
        &json!({}),
    )?;
    let now = Utc::now();
    let reset = web_cycle_end(host, session, PAID_PACKAGES, false, now)
        .or_else(|| web_cycle_end(host, session, FREE_PACKAGES, true, now))
        .map(|at| at.to_rfc3339());
    let id = account_id("workbuddy", &[WEB_HOST]);
    finish_workbuddy_reading(
        "workbuddy",
        &summary,
        &id,
        "WorkBuddy",
        "web",
        &None,
        reset,
        now,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn finish_workbuddy_reading(
    provider: &'static str,
    summary: &Value,
    id: &str,
    label: &str,
    sign_in: &str,
    email: &Option<String>,
    reset: Option<String>,
    now: DateTime<Utc>,
) -> Result<Reading, Unread> {
    let mut reading = Reading::empty(provider);
    let packages = summary["Packages"].as_array().ok_or_else(format_wb)?;
    let (mut pkg_total, mut pkg_left) = (0.0, 0.0);
    for package in packages
        .iter()
        .filter(|p| p["CapacityUnit"] == "credits" && !is_reward(p))
    {
        pkg_total += amount(&package["CycleTotalCapacity"]).ok_or_else(format_wb)?;
        pkg_left += amount(&package["CycleRemainCapacity"]).ok_or_else(format_wb)?;
    }
    let (mut group_total, mut group_left, mut reward_total, mut reward_left) = (0.0, 0.0, 0.0, 0.0);
    for group in summary["CreditGroups"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|g| g["CapacityUnit"] == "credits")
    {
        if let (Some(t), Some(l)) = (
            amount(&group["CycleTotalCapacity"]),
            amount(&group["CycleRemainCapacity"]),
        ) {
            if group["Kind"] == "benefit" {
                reward_total += t;
                reward_left += l;
            } else {
                group_total += t;
                group_left += l;
            }
        }
    }
    let (total, left) = if group_total > 0.0 {
        (round2(group_total), round2(group_left))
    } else {
        (round2(pkg_total), round2(pkg_left))
    };
    let available = amount(&summary["TotalCredits"])
        .map(round2)
        .unwrap_or_else(|| round2(left + reward_left));
    if total > 0.0 || reward_total > 0.0 || available > 0.0 {
        reading.billing.push(json!({
            "id": format!("{id}:credits"),
            "vendor": provider,
            "consumerSource": provider,
            "accountId": id,
            "account": label,
            "label": l10n::text("可用积分", "Available credits"),
            "amount": available,
            "currency": "credits",
            "planCredits": (total > 0.0).then_some(left),
            "rewardCredits": (reward_total > 0.0).then_some(round2(reward_left)),
            "resetAt": reset,
        }));
    }
    let plan = summary["SubscriptionPackageName"]
        .as_str()
        .map(str::trim)
        .filter(|plan| !plan.is_empty());
    let mut acc = json!({
        "id": id,
        "provider": provider,
        "label": label,
        "plan": plan,
        "checkedAt": now.to_rfc3339(),
        "signIn": sign_in,
    });
    if let Some(email) = email {
        acc["email"] = json!(email);
    }
    reading.account = Some(acc);
    if reading.billing.is_empty() {
        reading.notice = Some(l10n::text(
            "WorkBuddy 当前没有可用的积分额度",
            "WorkBuddy reports no credit allowance for this account.",
        ));
    }
    Ok(reading)
}

fn format_wb() -> Unread {
    Unread::from(l10n::text(
        "WorkBuddy 的积分格式无法识别",
        "WorkBuddy's credit usage has an unsupported format.",
    ))
}

pub(super) fn amount(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(text) => {
            let text = text.trim();
            (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
                .then(|| text.parse().ok())
                .flatten()
        }
        _ => None,
    }
    .filter(|n: &f64| n.is_finite() && *n >= 0.0)
}

fn is_reward(item: &Value) -> bool {
    item["PackageCode"]
        .as_str()
        .is_some_and(|code| REWARD_PACKAGES.contains(&code))
}

pub(super) fn round2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

fn post_web(
    host: &dyn Host,
    session: &WebSession,
    path: &str,
    body: &Value,
) -> Result<Value, Unread> {
    let referer = format!("{WEB_ORIGIN}/profile/plans-usage");
    let root = host
        .http_json(&HttpRequest {
            url: &format!("{WEB_ORIGIN}{path}"),
            secrets: &[("Cookie", &session.cookie)],
            headers: &[
                ("Accept", "application/json"),
                ("User-Agent", &session.user_agent),
                ("Origin", WEB_ORIGIN),
                ("Referer", &referer),
            ],
            body: Some(body),
            timeout: Duration::from_secs(15),
        })
        .map_err(|failure| match failure {
            HttpFailure::Status(401) => Unread::SignedOut(l10n::text(
                "WorkBuddy 在 Agent HUD 中的登录已失效，请在它的额度处重新登录",
                "WorkBuddy's sign-in in Agent HUD has ended; sign in again from its quota.",
            )),
            other => other.unread(),
        })?;
    if root["code"] != 0 {
        return Err(Unread::from(l10n::text(
            &format!("WorkBuddy 的账单服务返回了错误 {}", root["code"]),
            &format!(
                "WorkBuddy's billing service answered code {}.",
                root["code"]
            ),
        )));
    }
    root.get("data")
        .filter(|data| data.is_object())
        .cloned()
        .ok_or_else(format_wb)
}

fn web_cycle_end(
    host: &dyn Host,
    session: &WebSession,
    codes: &[&str],
    free: bool,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let path = if free {
        "/billing/meter/get-user-resource-free-packages"
    } else {
        "/billing/meter/get-user-resource-paid-packages"
    };
    let page = post_web(
        host,
        session,
        path,
        &json!({"PageNumber": 1, "PageSize": 100, "PackageCodes": codes, "Status": [0, 3]}),
    )
    .ok()?;
    earliest_cycle_end(&page, free, now)
}

pub(super) fn earliest_cycle_end(
    page: &Value,
    free: bool,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    page["Accounts"]
        .as_array()?
        .iter()
        .filter(|account| account["CapacityUnit"] == "credits")
        .filter(|account| !free || !is_reward(account))
        .filter_map(|account| parse_cst_end(account["CycleEndTime"].as_str()?, now))
        .min()
}

pub(super) fn parse_cst_end(text: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let china = FixedOffset::east_opt(8 * 3600)?;
    let end = NaiveDateTime::parse_from_str(text.trim(), "%Y-%m-%d %H:%M:%S").ok()?;
    let end =
        end.and_local_timezone(china).single()?.with_timezone(&Utc) + chrono::Duration::seconds(1);
    (end > now).then_some(end)
}
