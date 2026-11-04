//! Cursor's account (Open's CursorClient): the access token Cursor keeps in
//! its own database and the dashboard's usage summary as quota windows.
//! An account merged into SuperGrok is metered weekly instead, and its
//! meters come from the period Cursor itself shows (`GetCurrentPeriodUsage`).
//! Sessions and their tokens are read on this Mac (`sessions.rs`, the turn
//! log), never from the dashboard.
use super::periods::{current_period, summary_period};
use crate::account::{
    account_id, legacy_account, status, Host, HttpFailure, HttpRequest, Provider, Reading, Unread,
};
use crate::{l10n, Location, PerOs, Support};
use chrono::Utc;
use serde_json::{json, Value};
use std::time::Duration;

pub(crate) use super::sign_in::{sign_in, SignIn};

/// Cursor's state database, where it keeps its sign-in and chat headers.
pub(crate) const STATE: Location =
    Location::app_data(&["Cursor"], "User/globalStorage/state.vscdb");
pub const DATABASE: PerOs<Location> = PerOs {
    macos: Support::Verified(STATE),
    windows: Support::Unverified(STATE),
    linux: Support::Unverified(STATE),
};

pub(super) fn auth_format() -> String {
    l10n::text(
        "Cursor 账户登录信息格式无法识别",
        "Cursor account authentication has an unsupported format.",
    )
}
pub(super) fn database_format() -> String {
    l10n::text(
        "Cursor 账户数据库格式无法识别",
        "Cursor account database has an unsupported format.",
    )
}

pub(super) fn percentage(pool: &Value) -> Option<f64> {
    if pool["enabled"] == false {
        return None;
    }
    let used = pool["used"].as_f64()?;
    let limit = pool["limit"].as_f64()?;
    (used.is_finite() && used >= 0.0 && limit.is_finite() && limit > 0.0)
        .then_some(used / limit * 100.0)
}

/// Cursor's DashboardService, where the Cursor and Grok Bot apps ask for
/// their usage meters with Cursor's sign-in.
const DASHBOARD: &str = "https://api2.cursor.sh/aiserver.v1.DashboardService/";

pub(crate) fn dashboard(host: &dyn Host, token: &str, method: &str) -> Result<Value, HttpFailure> {
    host.http_json(&HttpRequest {
        url: &format!("{DASHBOARD}{method}"),
        secrets: &[("Authorization", &format!("Bearer {token}"))],
        headers: &[("Connect-Protocol-Version", "1")],
        body: Some(&json!({})),
        timeout: Duration::from_secs(12),
    })
}

/// The plan of an account merged into SuperGrok. It is metered weekly, on
/// the pool Grok Bot shares, and the dashboard's monthly summary stays at 0.
const MERGED_PLAN: &str = "super";

/// Cursor's plan, read with the sign-in Cursor keeps. It is spent on every
/// device the account signs in on, so it is read on the account interval.
pub static PROVIDER: Provider = Provider {
    local_work: false,
    ..Provider::new("cursor", read)
};

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let Some(SignIn { user, token, email }) = sign_in(host)? else {
        return Ok(Reading::empty("cursor"));
    };
    let cookie_str = format!("WorkosCursorSessionToken={user}%3A%3A{token}");
    // The web summary rides on a browser session cookie, which Cursor's
    // servers refuse (401/403/browser check) for some accounts while the
    // app's own bearer sign-in is still good. Then the period Cursor shows
    // (`GetCurrentPeriodUsage`, the call Grok Bot's reading already makes)
    // stands in for the summary, for every plan, merged or not.
    let summary = host
        .http_json(&HttpRequest {
            url: "https://cursor.com/api/usage-summary",
            secrets: &[("Cookie", &cookie_str)],
            headers: &[],
            body: None,
            timeout: Duration::from_secs(15),
        })
        .map_err(|failure| match failure {
            HttpFailure::BadHeader => auth_format().into(),
            HttpFailure::Unreachable => failure.unread_as(l10n::text(
                "无法读取 Cursor 账户用量，请检查网络",
                "Cursor account usage could not be reached; check the connection.",
            )),
            other => other.unread(),
        })
        .and_then(|body| {
            if !body["individualUsage"].is_object()
                && !body["teamUsage"].is_object()
                && !body["membershipType"].is_string()
            {
                Err(l10n::text(
                    "Cursor 账户返回的数据格式无法识别",
                    "Cursor account response has an unsupported format.",
                )
                .into())
            } else {
                Ok(body)
            }
        });
    let (body, fallback) = match summary {
        Ok(body) => (body, None),
        Err(why) => match dashboard(host, &token, "GetCurrentPeriodUsage")
            .ok()
            .and_then(|reply| current_period(&reply))
        {
            Some(period) => (json!({}), Some(period)),
            None => return Err(why),
        },
    };
    let id = account_id("cursor", &[&user]);
    let account_identity = legacy_account("Cursor", &user, "").unwrap();
    let period = if let Some(period) = fallback {
        period
    } else if body["membershipType"] == MERGED_PLAN {
        let reply =
            dashboard(host, &token, "GetCurrentPeriodUsage").map_err(|failure| match failure {
                HttpFailure::BadHeader => auth_format().into(),
                HttpFailure::Unreachable => failure.unread_as(l10n::text(
                    "无法读取 Cursor 的计划用量，请检查网络",
                    "Cursor's plan usage could not be reached; check the connection.",
                )),
                other => other.unread(),
            })?;
        current_period(&reply).ok_or_else(|| {
            Unread::from(l10n::text(
                "Cursor 的计划用量格式无法识别",
                "Cursor's plan usage has an unsupported format.",
            ))
        })?
    } else {
        summary_period(&body)
    };
    let reset = period.reset.as_deref();
    let duration = period.seconds;
    let [main, models, third_party] = period.used;
    let rows = [
        ("included", "Included usage", main),
        ("models", "Cursor Models", models),
        ("third-party", "Other Models", third_party),
        (
            "personal",
            "Individual spending limit",
            percentage(&body["individualUsage"]["overall"]),
        ),
        (
            "team",
            "Pooled usage",
            percentage(&body["teamUsage"]["pooled"]),
        ),
        (
            "extra",
            "On-demand usage",
            percentage(&body["individualUsage"]["onDemand"]),
        ),
    ];
    let quotas = rows
        .into_iter()
        .filter_map(|(key, name, used)| {
            used.map(|used| {
                // Cursor's first pool is its own models, which it calls
                // first-party, where "Cursor Models" would read as the vendor.
                let short = match key {
                    "included" => "Included",
                    "models" => "First-party",
                    "third-party" => "Third-party",
                    "personal" => "Spend limit",
                    "team" => "Pooled",
                    _ => "On-demand",
                };
                // The Swift app's window: `cursor` for the included pool, `cursor:<key>` for the rest.
                let window = match key {
                    "included" => "cursor".to_owned(),
                    _ => format!("cursor:{key}"),
                };
                let mut quota = json!({
                    "id":format!("{id}:{key}"),"vendor":"cursor","name":name,"shortLabel":short,"usedPercent":used,
                    "resetAt":reset,"status":status(used),
                    "account":email,"accountId":account_identity,"legacyWindowId":format!("{account_identity}/{window}"),
                    "allModels":!matches!(key,"models"|"third-party"),
                });
                if let Some(duration) = duration {
                    quota["windowDurationSeconds"] = json!(duration);
                }
                // The dashboard's used/limit fields price spending pools. The
                // source does not expose a token/credit capacity for these rows.
                quota
            })
        })
        .collect::<Vec<_>>();
    let notice = quotas.is_empty().then(|| {
        l10n::text(
            "Cursor 已连接，当前计划未提供额度比例",
            "Cursor is connected; this plan reports no quota percentage.",
        )
    });
    Ok(Reading {
        provider: "cursor",
        failure: None,
        quotas,
        billing: Vec::new(),
        notice,
        additional_accounts: Vec::new(),
        session_records: Vec::new(),
        services: Vec::new(),
        account: Some(
            json!({"id":account_identity,"provider":"cursor","label":email,"plan":body["membershipType"],"checkedAt":Utc::now().to_rfc3339()}),
        ),
    })
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
