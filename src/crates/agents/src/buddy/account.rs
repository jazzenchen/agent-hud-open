//! WorkBuddy's credits, which only its plans page shows: read with the
//! sign-in the user makes in Agent HUD's own window on www.workbuddy.cn
//! (`WebSignIn`). The site binds a session to the browser it was made in, so
//! every request presents the window's browser identity.
//!
//! The request shapes and package codes follow CodexBar's WorkBuddy provider
//! (MIT), which took the codes from the WorkBuddy 5.6.2 client.
use crate::account::{
    account_id, status, Host, HttpFailure, HttpRequest, Provider, Reading, Unread, WebSession,
    WebSignIn, WebSite,
};
use crate::l10n;
use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};
use serde_json::{json, Value};
use std::time::Duration;

const HOST: &str = "www.workbuddy.cn";
const ORIGIN: &str = "https://www.workbuddy.cn";

pub static WEB: WebSignIn = WebSignIn {
    sites: &[WebSite {
        host: HOST,
        name: "WorkBuddy",
        sign_in: "https://www.workbuddy.cn/profile/plans-usage",
    }],
};

/// The credits are spent on every device the account is signed in on.
pub static PROVIDER: Provider = Provider {
    local_work: false,
    web: Some(&WEB),
    ..Provider::new("workbuddy", read)
};

/// The packages the WorkBuddy client lists as paid and as free; the listing
/// requests are refused without them.
const PAID_PACKAGES: &[&str] = &[
    "TCACA_code_002_AkiJS3ZHF5",
    "TCACA_code_005_maRGyrHhw1",
    "TCACA_code_003_FAnt7lcmRT",
    "TCACA_code_023_4xbGhMrE6q",
    "TCACA_code_026_BaESVICNoi",
    "TCACA_code_027_0FCGVA6vSa",
    "TCACA_code_009_0XmEQc2xOf",
    "TCACA_code_038_OhvqZtiPKr",
    "TCACA_code_036_lupO5WgNdG",
];
const FREE_PACKAGES: &[&str] = &[
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
/// The free package reward grants are listed under; they lapse, they don't reset.
const REWARDS_PACKAGE: &str = "TCACA_code_007_nzdH5h4Nl0";

/// The credits of the account the user signed in to; nothing until they have.
pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let mut reading = Reading::empty("workbuddy");
    let Some(session) = host.web_session("workbuddy", HOST) else {
        return Ok(reading);
    };
    let summary = post(
        host,
        &session,
        "/billing/meter/get-user-resource-summary",
        &json!({}),
    )?;
    let packages = summary["Packages"].as_array().ok_or_else(format)?;
    let (mut total, mut left) = (0.0, 0.0);
    for package in packages.iter().filter(|p| p["CapacityUnit"] == "credits") {
        total += amount(&package["CycleTotalCapacity"]).ok_or_else(format)?;
        left += amount(&package["CycleRemainCapacity"]).ok_or_else(format)?;
    }
    let (mut reward_total, mut reward_left) = (0.0, 0.0);
    for group in summary["CreditGroups"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|g| g["Kind"] == "benefit" && g["CapacityUnit"] == "credits")
    {
        if let (Some(t), Some(l)) = (
            amount(&group["CycleTotalCapacity"]),
            amount(&group["CycleRemainCapacity"]),
        ) {
            reward_total += t;
            reward_left += l;
        }
    }
    // The reset is optional: a listing that fails keeps the balance.
    let now = Utc::now();
    let reset = cycle_end(host, &session, PAID_PACKAGES, false, now)
        .or_else(|| cycle_end(host, &session, FREE_PACKAGES, true, now))
        .map(|at| at.to_rfc3339());
    let id = account_id("workbuddy", &[HOST]);
    if total > 0.0 {
        let spent = (total - left).max(0.0);
        let used = (spent / total * 100.0).clamp(0.0, 100.0);
        reading.quotas.push(json!({
            "id": format!("{id}:credits"), "vendor": "workbuddy", "consumerSource": "workbuddy", "accountId": id, "account": "WorkBuddy",
            "name": "Credits", "shortLabel": "Credits", "usedPercent": used, "resetAt": reset,
            "allModels": true, "status": status(used),
            "nativeUsage": {"used": spent, "capacity": total, "unit": "credits"},
        }));
    }
    if reward_total > 0.0 {
        let spent = (reward_total - reward_left).max(0.0);
        let used = (spent / reward_total * 100.0).clamp(0.0, 100.0);
        reading.quotas.push(json!({
            "id": format!("{id}:rewards"), "vendor": "workbuddy", "accountId": id, "account": "WorkBuddy",
            "name": "Reward credits", "shortLabel": "Rewards", "usedPercent": used,
            "allModels": true, "status": status(used),
            "nativeUsage": {"used": spent, "capacity": reward_total, "unit": "credits"},
        }));
    }
    let plan = summary["SubscriptionPackageName"]
        .as_str()
        .map(str::trim)
        .filter(|plan| !plan.is_empty());
    reading.account = Some(json!({
        "id": id, "provider": "workbuddy", "label": "WorkBuddy", "plan": plan,
        "checkedAt": now.to_rfc3339(), "signIn": "web",
    }));
    if reading.quotas.is_empty() {
        reading.notice = Some(l10n::text(
            "WorkBuddy 当前没有可用的积分额度",
            "WorkBuddy reports no credit allowance for this account.",
        ));
    }
    Ok(reading)
}

fn format() -> Unread {
    Unread::from(l10n::text(
        "WorkBuddy 的积分格式无法识别",
        "WorkBuddy's credit usage has an unsupported format.",
    ))
}

/// Billing amounts are decimal strings ("500"); numbers are taken too.
fn amount(value: &Value) -> Option<f64> {
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

/// One billing request: its `data` when `code` is 0.
fn post(host: &dyn Host, session: &WebSession, path: &str, body: &Value) -> Result<Value, Unread> {
    let referer = format!("{ORIGIN}/profile/plans-usage");
    let root = host
        .http_json(&HttpRequest {
            url: &format!("{ORIGIN}{path}"),
            secrets: &[("Cookie", &session.cookie)],
            headers: &[
                ("Accept", "application/json"),
                ("User-Agent", &session.user_agent),
                ("Origin", ORIGIN),
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
        .ok_or_else(format)
}

/// When the earliest of `codes`' current credit cycles ends. A cycle's end
/// carries no zone: it is China Standard Time, and the page refreshes a
/// second after it. Reward grants lapse rather than reset, so they never
/// stand for the reset.
fn cycle_end(
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
    let page = post(
        host,
        session,
        path,
        &json!({"PageNumber": 1, "PageSize": 100, "PackageCodes": codes, "Status": [0, 3]}),
    )
    .ok()?;
    let china = FixedOffset::east_opt(8 * 3600)?;
    page["Accounts"]
        .as_array()?
        .iter()
        .filter(|account| account["CapacityUnit"] == "credits")
        .filter(|account| !free || account["PackageCode"] != REWARDS_PACKAGE)
        .filter_map(|account| {
            let text = account["CycleEndTime"].as_str()?;
            let end = NaiveDateTime::parse_from_str(text.trim(), "%Y-%m-%d %H:%M:%S").ok()?;
            let end = end.and_local_timezone(china).single()?.with_timezone(&Utc)
                + chrono::Duration::seconds(1);
            (end > now).then_some(end)
        })
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    const SUMMARY: &str = "https://www.workbuddy.cn/billing/meter/get-user-resource-summary";
    const PAID: &str = "https://www.workbuddy.cn/billing/meter/get-user-resource-paid-packages";
    const FREE: &str = "https://www.workbuddy.cn/billing/meter/get-user-resource-free-packages";

    fn signed_in() -> FakeHost {
        let mut fake = FakeHost::new("/u");
        fake.web_sessions.insert(
            format!("workbuddy|{HOST}"),
            WebSession {
                host: HOST.into(),
                cookie: "session=1".into(),
                user_agent: crate::account::WEB_USER_AGENT.into(),
            },
        );
        fake
    }

    fn summary() -> Value {
        json!({"code": 0, "data": {
            "SubscriptionPackageName": "Pro",
            "Packages": [
                {"CapacityUnit": "credits", "CycleTotalCapacity": "1000", "CycleRemainCapacity": "250"},
                {"CapacityUnit": "minutes", "CycleTotalCapacity": "60", "CycleRemainCapacity": "60"},
            ],
            "CreditGroups": [
                {"Kind": "benefit", "CapacityUnit": "credits", "CycleTotalCapacity": "200", "CycleRemainCapacity": "200"},
            ],
        }})
    }

    #[test]
    fn nothing_is_read_before_the_user_signs_in() {
        let reading = read(&FakeHost::new("/u")).unwrap();
        assert!(reading.quotas.is_empty() && reading.account.is_none());
    }

    #[test]
    fn the_credits_rewards_and_reset_come_from_the_plans_page() {
        let paid = json!({"code": 0, "data": {"Accounts": [
            {"CapacityUnit": "credits", "CycleEndTime": "2099-12-31 23:59:59"},
            {"CapacityUnit": "credits", "CycleEndTime": "2000-01-01 00:00:00"},
        ]}});
        let host = signed_in()
            .answer(SUMMARY, Ok(summary()))
            .answer(PAID, Ok(paid));
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 2);
        assert_eq!(reading.quotas[0]["usedPercent"], 75.0);
        assert_eq!(reading.quotas[0]["nativeUsage"]["capacity"], 1000.0);
        // 23:59:59 in China, a second later, is 16:00 UTC the same day.
        assert_eq!(reading.quotas[0]["resetAt"], "2099-12-31T16:00:00+00:00");
        assert_eq!(reading.quotas[1]["shortLabel"], "Rewards");
        assert_eq!(reading.quotas[1]["usedPercent"], 0.0);
        assert_eq!(reading.account.unwrap()["plan"], "Pro");
        let headers = host.headers.borrow();
        assert!(headers[0]
            .iter()
            .any(|(name, value)| name == "User-Agent" && value == crate::account::WEB_USER_AGENT));
        let sent = host.sent.borrow();
        assert_eq!(
            sent[1].1.as_ref().unwrap()["PackageCodes"][0],
            PAID_PACKAGES[0]
        );
    }

    #[test]
    fn the_free_list_stands_in_for_the_reset_but_never_a_reward_grant() {
        let free = json!({"code": 0, "data": {"Accounts": [
            {"CapacityUnit": "credits", "PackageCode": REWARDS_PACKAGE, "CycleEndTime": "2098-01-01 00:00:00"},
            {"CapacityUnit": "credits", "PackageCode": "TCACA_code_001_PqouKr6QWV", "CycleEndTime": "2099-01-01 00:00:00"},
        ]}});
        let host = signed_in()
            .answer(SUMMARY, Ok(summary()))
            .answer(PAID, Ok(json!({"code": 0, "data": {"Accounts": []}})))
            .answer(FREE, Ok(free));
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas[0]["resetAt"], "2098-12-31T16:00:01+00:00");
        // Without any listing the balance still reads.
        let host = signed_in().answer(SUMMARY, Ok(summary()));
        assert!(read(&host).unwrap().quotas[0]["resetAt"].is_null());
    }

    #[test]
    fn an_ended_session_is_a_sign_out_and_a_refusal_code_a_failure() {
        let host = signed_in().answer(SUMMARY, Err(HttpFailure::Status(401)));
        assert_eq!(read(&host).err().unwrap().kind(), "signedOut");
        let host = signed_in().answer(SUMMARY, Err(HttpFailure::Status(429)));
        assert_eq!(read(&host).err().unwrap().kind(), "transient");
        let host = signed_in().answer(SUMMARY, Ok(json!({"code": 10001, "data": {}})));
        assert_eq!(read(&host).err().unwrap().kind(), "failed");
    }
}
