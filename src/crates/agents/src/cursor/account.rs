//! Cursor's account (Open's CursorClient): the access token Cursor keeps in
//! its own database and the dashboard's usage summary as quota windows.
//! An account merged into SuperGrok is metered weekly instead, and its
//! meters come from the period Cursor itself shows (`GetCurrentPeriodUsage`).
//! Sessions and their tokens are read on this Mac (`sessions.rs`, the turn
//! log), never from the dashboard.
use crate::account::{
    account_id, legacy_account, locate, DatabaseFailure, Host, HttpFailure, HttpRequest, Reading,
};
use crate::{l10n, Location, PerOs, Support};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, FixedOffset, Utc};
use serde_json::{json, Value};
use std::time::Duration;

/// Cursor's state database, where it keeps its sign-in and chat headers.
pub(crate) const STATE: Location =
    Location::app_data(&["Cursor"], "User/globalStorage/state.vscdb");
pub const DATABASE: PerOs<Location> = PerOs {
    macos: Support::Verified(STATE),
    windows: Support::Unverified(STATE),
    linux: Support::Unverified(STATE),
};

fn auth_format() -> String {
    l10n::text(
        "Cursor 账户登录信息格式无法识别",
        "Cursor account authentication has an unsupported format.",
    )
}
fn database_format() -> String {
    l10n::text(
        "Cursor 账户数据库格式无法识别",
        "Cursor account database has an unsupported format.",
    )
}

fn percentage(pool: &Value) -> Option<f64> {
    if pool["enabled"] == false {
        return None;
    }
    let used = pool["used"].as_f64()?;
    let limit = pool["limit"].as_f64()?;
    (used.is_finite() && used >= 0.0 && limit.is_finite() && limit > 0.0)
        .then_some(used / limit * 100.0)
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .filter(|number| number.is_finite() && *number >= 0.0)
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

/// The plan's meters for one billing period: included, first-party and
/// third-party usage, the period's end and its length in seconds.
struct Period {
    used: [Option<f64>; 3],
    reset: Option<String>,
    seconds: Option<f64>,
}

fn seconds(
    start: Option<DateTime<FixedOffset>>,
    end: Option<DateTime<FixedOffset>>,
) -> Option<f64> {
    let (start, end) = start.zip(end).filter(|(start, end)| end > start)?;
    Some((end - start).num_milliseconds() as f64 / 1000.0)
}

/// The period in the dashboard's usage summary.
fn summary_period(body: &Value) -> Period {
    let at = |key: &str| {
        let text = body[key].as_str()?;
        Some((text, DateTime::parse_from_rfc3339(text).ok()?))
    };
    let (start, end) = (at("billingCycleStart"), at("billingCycleEnd"));
    let plan = &body["individualUsage"]["plan"];
    let main = (plan["enabled"] != false)
        .then(|| number(&plan["totalPercentUsed"]).or_else(|| percentage(plan)))
        .flatten();
    Period {
        used: [
            main,
            number(&plan["autoPercentUsed"]),
            number(&plan["apiPercentUsed"]),
        ],
        reset: end.map(|(text, _)| text.to_owned()),
        seconds: seconds(start.map(|(_, at)| at), end.map(|(_, at)| at)),
    }
}

/// The period Cursor shows (`GetCurrentPeriodUsage`), whose bounds are
/// milliseconds. The servers leave out a meter at 0.
fn current_period(reply: &Value) -> Option<Period> {
    let plan = reply.get("planUsage").filter(|plan| plan.is_object())?;
    let at = |key: &str| {
        let ms = reply[key]
            .as_str()
            .and_then(|text| text.parse().ok())
            .or_else(|| reply[key].as_i64())?;
        Some(DateTime::from_timestamp_millis(ms)?.fixed_offset())
    };
    let (start, end) = (at("billingCycleStart"), at("billingCycleEnd"));
    let used = |key: &str| match &plan[key] {
        Value::Null => Some(0.0),
        value => number(value),
    };
    Some(Period {
        used: [
            used("totalPercentUsed"),
            used("autoPercentUsed"),
            used("apiPercentUsed"),
        ],
        reset: end.map(|end| end.to_rfc3339()),
        seconds: seconds(start, end),
    })
}

/// Cursor's saved sign-in: the account's user id, its access token and the
/// email Cursor shows. Grok Bot's server usage is read with it as well.
pub(crate) struct SignIn {
    pub user: String,
    pub token: String,
    pub email: Option<String>,
}

/// The sign-in Cursor keeps in its database; None where Cursor is not here.
pub(crate) fn sign_in(host: &dyn Host) -> Result<Option<SignIn>, String> {
    let Ok(database) = locate(host, &DATABASE) else {
        return Ok(None);
    };
    let rows = match host.sqlite_rows(
        &database,
        "SELECT key, value FROM ItemTable WHERE key IN (?1, ?2)",
        &["cursorAuth/accessToken", "cursorAuth/cachedEmail"],
        Duration::from_secs(2),
    ) {
        Ok(rows) => rows,
        Err(DatabaseFailure::Missing) => return Ok(None),
        Err(DatabaseFailure::Unopenable) => {
            return Err(l10n::text(
                "无法打开 Cursor 账户数据库",
                "Cursor account database could not be opened.",
            ))
        }
        Err(DatabaseFailure::Unreadable) => {
            return Err(l10n::text(
                "无法读取 Cursor 账户数据库",
                "Cursor account database could not be read.",
            ))
        }
        Err(DatabaseFailure::Unrecognized) => return Err(database_format()),
    };
    // A stored value that is not text does not fit the database Cursor writes.
    let lookup = |key: &str| -> Result<Option<String>, String> {
        rows.iter()
            .find(|row| row[0] == key)
            .map(|row| {
                row[1]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(database_format)
            })
            .transpose()
    };
    let Some(token) = lookup("cursorAuth/accessToken")?.filter(|token| !token.is_empty()) else {
        return Err(l10n::text(
            "登录 Cursor 后读取账户用量",
            "Sign in to Cursor to read its account usage.",
        ));
    };
    let email = lookup("cursorAuth/cachedEmail")?.filter(|email| !email.trim().is_empty());
    if token.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n')) {
        return Err(auth_format());
    }
    let parts = token.split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(auth_format());
    }
    let claims: Value = URL_SAFE_NO_PAD
        .decode(parts[1].trim_end_matches('='))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(auth_format)?;
    if !claims["exp"]
        .as_f64()
        .is_some_and(|expiry| expiry > Utc::now().timestamp() as f64 + 60.0)
    {
        return Err(l10n::text(
            "Cursor 保存的登录已过期，请在 Cursor 中重新登录",
            "Cursor's saved access token expired; sign in in Cursor to renew it.",
        ));
    }
    let user = claims["sub"]
        .as_str()
        .and_then(|sub| sub.rsplit('|').next())
        .filter(|user| {
            !user.is_empty()
                && user
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        })
        .ok_or_else(auth_format)?;
    Ok(Some(SignIn {
        user: user.to_owned(),
        token,
        email,
    }))
}

pub fn read(host: &dyn Host) -> Result<Reading, String> {
    let Some(SignIn { user, token, email }) = sign_in(host)? else {
        return Ok(Reading::empty("cursor"));
    };
    let cookie_str = format!("WorkosCursorSessionToken={user}%3A%3A{token}");
    let body = host
        .http_json(&HttpRequest {
            url: "https://cursor.com/api/usage-summary",
            secrets: &[("Cookie", &cookie_str)],
            headers: &[],
            body: None,
            timeout: Duration::from_secs(15),
        })
        .map_err(|failure| match failure {
            HttpFailure::BadHeader => auth_format(),
            HttpFailure::Unreachable => l10n::text(
                "无法读取 Cursor 账户用量，请检查网络",
                "Cursor account usage could not be reached; check the connection.",
            ),
            other => other.message(),
        })?;
    if !body["individualUsage"].is_object()
        && !body["teamUsage"].is_object()
        && !body["membershipType"].is_string()
    {
        return Err(l10n::text(
            "Cursor 账户返回的数据格式无法识别",
            "Cursor account response has an unsupported format.",
        ));
    }
    let id = account_id("cursor", &[&user]);
    let account_identity = legacy_account("Cursor", &user, "").unwrap();
    let period = if body["membershipType"] == MERGED_PLAN {
        let reply =
            dashboard(host, &token, "GetCurrentPeriodUsage").map_err(|failure| match failure {
                HttpFailure::BadHeader => auth_format(),
                HttpFailure::Unreachable => l10n::text(
                    "无法读取 Cursor 的计划用量，请检查网络",
                    "Cursor's plan usage could not be reached; check the connection.",
                ),
                other => other.message(),
            })?;
        current_period(&reply).ok_or_else(|| {
            l10n::text(
                "Cursor 的计划用量格式无法识别",
                "Cursor's plan usage has an unsupported format.",
            )
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
                    "resetAt":reset,"status":if used>=90.0 {"critical"}else if used>=70.0 {"warning"}else{"normal"},
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
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;
    use std::path::PathBuf;

    const SUMMARY: &str = "https://cursor.com/api/usage-summary";
    const PERIOD: &str =
        "https://api2.cursor.sh/aiserver.v1.DashboardService/GetCurrentPeriodUsage";
    const DB: &str = "/u/AppData/Cursor/User/globalStorage/state.vscdb";

    fn token(expires_in: i64) -> String {
        let claims = json!({"exp": Utc::now().timestamp() + expires_in, "sub": "auth0|user_123"});
        format!(
            "h.{}.s",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
        )
    }

    /// A Cursor state database with these ItemTable values.
    fn database(dir: &tempfile::TempDir, items: &[(&str, &str)]) -> PathBuf {
        let path = dir.path().join("state.vscdb");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value BLOB);")
            .unwrap();
        for (key, value) in items {
            db.execute("INSERT INTO ItemTable VALUES (?1, ?2)", [key, value])
                .unwrap();
        }
        path
    }

    fn signed_in(dir: &tempfile::TempDir, token: &str) -> FakeHost {
        let mut host = FakeHost::new("/u");
        host.databases.insert(
            DB.into(),
            database(
                dir,
                &[
                    ("cursorAuth/accessToken", token),
                    ("cursorAuth/cachedEmail", "me@example.com"),
                ],
            ),
        );
        host
    }

    fn summary() -> Value {
        json!({
            "membershipType": "pro",
            "billingCycleStart": "2026-10-01T00:00:00Z",
            "billingCycleEnd": "2026-11-01T00:00:00Z",
            "individualUsage": {"plan": {"totalPercentUsed": 42.0, "autoPercentUsed": 10.0, "apiPercentUsed": 80.0}},
        })
    }

    #[test]
    fn without_cursor_there_is_nothing_to_say() {
        let reading = read(&FakeHost::new("/u")).unwrap();
        assert!(reading.account.is_none() && reading.notice.is_none());
    }

    #[test]
    fn the_saved_sign_in_reads_the_usage_summary() {
        let dir = tempfile::tempdir().unwrap();
        let token = token(3600);
        let host = signed_in(&dir, &token).answer(SUMMARY, Ok(summary()));
        let reading = read(&host).unwrap();
        let identity = legacy_account("Cursor", "user_123", "").unwrap();
        let account = reading.account.unwrap();
        assert_eq!(account["id"], identity.as_str());
        assert_eq!(account["label"], "me@example.com");
        assert_eq!(account["plan"], "pro");
        let names: Vec<_> = reading
            .quotas
            .iter()
            .map(|q| q["shortLabel"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Included", "First-party", "Third-party"]);
        let id = account_id("cursor", &["user_123"]);
        assert_eq!(reading.quotas[0]["id"], format!("{id}:included"));
        assert_eq!(reading.quotas[2]["status"], "warning");
        assert_eq!(reading.quotas[1]["allModels"], false);
        assert_eq!(reading.quotas[0]["windowDurationSeconds"], 2_678_400.0);
        let sent = host.sent.borrow();
        assert_eq!(
            sent[0].0,
            [(
                "Cookie".to_owned(),
                format!("WorkosCursorSessionToken=user_123%3A%3A{token}")
            )]
        );
        // Sessions come from Cursor's own files: the summary is the one request.
        assert_eq!(sent.len(), 1);
        assert!(reading.session_records.is_empty());
    }

    #[test]
    fn each_window_keeps_its_open_window_id() {
        let dir = tempfile::tempdir().unwrap();
        let mut body = summary();
        body["individualUsage"]["overall"] = json!({"used": 5.0, "limit": 10.0});
        body["individualUsage"]["onDemand"] = json!({"used": 1.0, "limit": 10.0});
        body["teamUsage"] = json!({"pooled": {"used": 2.0, "limit": 10.0}});
        let host = signed_in(&dir, &token(3600)).answer(SUMMARY, Ok(body));
        let reading = read(&host).unwrap();
        // The Swift app's account for user_123, as its quota history keys it.
        let account = "account:34591b03e518547be61123ba4f04d8cac609afdd291ac7fcce82555fe7d45729";
        assert_eq!(reading.account.unwrap()["id"], account);
        let windows: Vec<_> = reading
            .quotas
            .iter()
            .map(|q| q["legacyWindowId"].as_str().unwrap())
            .collect();
        assert_eq!(
            windows,
            [
                "cursor",
                "cursor:models",
                "cursor:third-party",
                "cursor:personal",
                "cursor:team",
                "cursor:extra"
            ]
            .map(|window| format!("{account}/{window}"))
        );
    }

    #[test]
    fn a_merged_account_reads_the_weekly_period_cursor_shows() {
        let dir = tempfile::tempdir().unwrap();
        let token = token(3600);
        let mut body = summary();
        body["membershipType"] = json!("super");
        body["individualUsage"]["plan"] =
            json!({"totalPercentUsed": 0, "autoPercentUsed": 0, "apiPercentUsed": 0});
        // As the servers sent it on 2026-10-29, without the third-party meter at 0.
        let period = json!({
            "billingCycleStart": "1792866037079",
            "billingCycleEnd": "1793470837079",
            "planUsage": {"totalPercentUsed": 0.49, "autoPercentUsed": 0.49},
        });
        let host = signed_in(&dir, &token)
            .answer(SUMMARY, Ok(body))
            .answer(PERIOD, Ok(period));
        let reading = read(&host).unwrap();
        let used: Vec<_> = reading
            .quotas
            .iter()
            .map(|q| q["usedPercent"].as_f64().unwrap())
            .collect();
        assert_eq!(used, [0.49, 0.49, 0.0]);
        assert_eq!(
            reading.quotas[0]["resetAt"],
            "2026-10-31T18:20:37.079+00:00"
        );
        assert_eq!(reading.quotas[0]["windowDurationSeconds"], 604_800.0);
        assert_eq!(reading.account.unwrap()["plan"], "super");
        let sent = host.sent.borrow();
        assert_eq!(
            sent[1].0,
            [("Authorization".to_owned(), format!("Bearer {token}"))]
        );
    }

    #[test]
    fn a_merged_account_without_its_period_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let mut body = summary();
        body["membershipType"] = json!("super");
        let host = signed_in(&dir, &token(3600)).answer(SUMMARY, Ok(body));
        assert_eq!(
            read(&host).err().unwrap(),
            "Cursor's plan usage could not be reached; check the connection."
        );
        let dir = tempfile::tempdir().unwrap();
        let mut body = summary();
        body["membershipType"] = json!("super");
        let host = signed_in(&dir, &token(3600))
            .answer(SUMMARY, Ok(body))
            .answer(PERIOD, Ok(json!({"enabled": true})));
        assert_eq!(
            read(&host).err().unwrap(),
            "Cursor's plan usage has an unsupported format."
        );
    }

    #[test]
    fn sign_in_problems_are_named() {
        let dir = tempfile::tempdir().unwrap();
        let expired = signed_in(&dir, &token(-10));
        assert_eq!(
            read(&expired).err().unwrap(),
            "Cursor's saved access token expired; sign in in Cursor to renew it."
        );
        let dir = tempfile::tempdir().unwrap();
        let mut signed_out = FakeHost::new("/u");
        signed_out.databases.insert(DB.into(), database(&dir, &[]));
        assert_eq!(
            read(&signed_out).err().unwrap(),
            "Sign in to Cursor to read its account usage."
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("other.db");
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE t(a)")
            .unwrap();
        let mut odd = FakeHost::new("/u");
        odd.databases.insert(DB.into(), path);
        assert_eq!(read(&odd).err().unwrap(), database_format());
    }
}
