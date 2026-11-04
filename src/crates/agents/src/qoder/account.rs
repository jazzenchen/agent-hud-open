//! Qoder's model credits, which only its account page shows: read with the
//! sign-in the user makes in Agent HUD's own window, on qoder.com or, for
//! the China builds, qoder.com.cn (`WebSignIn`). Qoder, Qoder CN and QoderWork
//! spend the same credits as the account they are signed in to.
use crate::account::{
    account_id, status, Host, HttpFailure, HttpRequest, Provider, Reading, Unread, WebSession,
    WebSignIn, WebSite,
};
use crate::l10n;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::time::Duration;

/// The sites a Qoder account is on. Each page asks the user to sign in when
/// they aren't, and shows the credits after.
pub static WEB: WebSignIn = WebSignIn {
    sites: &[
        WebSite {
            host: "qoder.com",
            name: "Qoder",
            sign_in: "https://qoder.com/account/usage",
        },
        WebSite {
            host: "qoder.com.cn",
            name: "Qoder CN",
            sign_in: "https://qoder.com.cn/account/usage",
        },
    ],
};

/// The credits are spent on every device the account is signed in on.
pub static PROVIDER: Provider = Provider {
    local_work: false,
    work_sources: &["qoder", "qoder-cn", "qoder-work"],
    web: Some(&WEB),
    ..Provider::new("qoder", read)
};

/// The credits of every site the user signed in to; nothing until they have.
pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let mut reading = Reading::empty("qoder");
    let mut failures = Vec::new();
    for site in WEB.sites {
        let Some(session) = host.web_session("qoder", site.host) else {
            continue;
        };
        match credits(host, site, &session) {
            Ok((quota, account)) => {
                reading.quotas.push(quota);
                if reading.account.is_none() {
                    reading.account = Some(account);
                } else {
                    reading.additional_accounts.push(account);
                }
            }
            Err(why) => failures.push(why),
        }
    }
    match Unread::combined(failures) {
        // One site read and another failed: the reading stands, with why.
        Some(why) if reading.account.is_some() => {
            reading.notice = Some(why.reason().to_owned());
            Ok(reading)
        }
        Some(why) => Err(why),
        None => Ok(reading),
    }
}

/// The account page's credit summary for `site`.
fn credits(
    host: &dyn Host,
    site: &WebSite,
    session: &WebSession,
) -> Result<(Value, Value), Unread> {
    let origin = format!("https://{}", site.host);
    let referer = format!("{origin}/account/usage");
    let body = host
        .http_json(&HttpRequest {
            url: &format!("{origin}/api/v2/me/usages/big_model_credits"),
            secrets: &[("Cookie", &session.cookie)],
            headers: &[
                ("Accept", "application/json, text/plain, */*"),
                ("User-Agent", &session.user_agent),
                ("Origin", &origin),
                ("Referer", &referer),
                ("X-Requested-With", "XMLHttpRequest"),
            ],
            body: None,
            timeout: Duration::from_secs(15),
        })
        .map_err(|failure| match failure {
            HttpFailure::Status(401 | 403) => Unread::SignedOut(l10n::text(
                &format!(
                    "{} 在 Agent HUD 中的登录已失效，请在它的额度处重新登录",
                    site.name
                ),
                &format!(
                    "{}'s sign-in in Agent HUD has ended; sign in again from its quota.",
                    site.name
                ),
            )),
            other => other.unread(),
        })?;
    let summary = parse(&body).ok_or_else(|| {
        Unread::from(l10n::text(
            "Qoder 的额度格式无法识别",
            "Qoder's credit usage has an unsupported format.",
        ))
    })?;
    let id = account_id("qoder", &[site.host]);
    let mut quota = json!({
        "id": format!("{id}:credits"), "vendor": "qoder", "accountId": id, "account": site.name,
        "name": "Model credits", "shortLabel": "Credits", "usedPercent": summary.percent,
        "resetAt": summary.reset, "allModels": true, "status": status(summary.percent),
        "nativeUsage": {"used": summary.used, "capacity": summary.total, "unit": "credits"},
    });
    if summary.total <= 0.0 {
        quota["nativeUsage"] = Value::Null;
    }
    let account = json!({
        "id": id, "provider": "qoder", "label": site.name,
        "checkedAt": Utc::now().to_rfc3339(), "signIn": "web",
    });
    Ok((quota, account))
}

struct Summary {
    used: f64,
    total: f64,
    percent: f64,
    reset: Option<String>,
}

fn field<'a>(object: &'a Value, camel: &str, snake: &str) -> &'a Value {
    match &object[camel] {
        Value::Null => &object[snake],
        value => value,
    }
}

fn amount(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite() && *n >= 0.0)
}

/// Used and total credits of one quota, from its `quotaSummary`.
fn quota(container: &Value) -> Option<(f64, f64, Option<f64>)> {
    let summary = field(container, "quotaSummary", "quota_summary");
    if !summary.is_object() {
        return None;
    }
    let used = amount(field(summary, "usedValue", "used_value"))?;
    let total = amount(field(summary, "limitValue", "limit_value"))?;
    let given = match field(summary, "usagePercentage", "usage_percentage") {
        Value::Null => None,
        value => Some(amount(value)?),
    };
    Some((used, total, given))
}

/// The answer of `big_model_credits`: the plan's credits, with the shared
/// ones added where the account has both, and when they reset.
fn parse(body: &Value) -> Option<Summary> {
    let (used, total, given) = quota(field(body, "totalQuota", "total_quota"))?;
    let shared = match field(body, "sharedQuota", "shared_quota") {
        Value::Null => None,
        container => Some(quota(container)?),
    };
    let (used, total, given) = match shared {
        Some((more_used, more_total, _)) => (used + more_used, total + more_total, None),
        None => (used, total, given),
    };
    if total == 0.0 && used > 0.0 {
        return None;
    }
    let percent = given
        .unwrap_or(if total > 0.0 {
            used / total * 100.0
        } else {
            100.0
        })
        .clamp(0.0, 100.0);
    let reset = match field(body, "nextResetAt", "next_reset_at") {
        Value::Number(n) => n.as_i64().and_then(|at| {
            let ms = if at > 10_000_000_000 { at } else { at * 1000 };
            DateTime::<Utc>::from_timestamp_millis(ms)
        }),
        Value::String(text) => DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|at| at.with_timezone(&Utc)),
        _ => None,
    }
    .map(|at| at.to_rfc3339());
    Some(Summary {
        used,
        total,
        percent,
        reset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    const GLOBAL: &str = "https://qoder.com/api/v2/me/usages/big_model_credits";
    const CHINA: &str = "https://qoder.com.cn/api/v2/me/usages/big_model_credits";

    fn session(host: &str) -> WebSession {
        WebSession {
            host: host.into(),
            cookie: "sid=abc".into(),
            user_agent: crate::account::WEB_USER_AGENT.into(),
        }
    }

    fn signed_in(hosts: &[&str]) -> FakeHost {
        let mut fake = FakeHost::new("/u");
        for host in hosts {
            fake.web_sessions
                .insert(format!("qoder|{host}"), session(host));
        }
        fake
    }

    #[test]
    fn nothing_is_read_before_the_user_signs_in() {
        let reading = read(&FakeHost::new("/u")).unwrap();
        assert!(reading.quotas.is_empty() && reading.account.is_none());
        assert!(reading.notice.is_none());
    }

    #[test]
    fn the_credits_are_read_with_the_session_the_window_kept() {
        let body = json!({
            "totalQuota": {"quotaSummary": {"usedValue": 300, "limitValue": 2000, "unit": "credits"}},
            "nextResetAt": 1_794_000_000_000_i64,
        });
        let host = signed_in(&["qoder.com"]).answer(GLOBAL, Ok(body));
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 1);
        let quota = &reading.quotas[0];
        assert_eq!(quota["usedPercent"], 15.0);
        assert_eq!(quota["nativeUsage"]["capacity"], 2000.0);
        assert!(quota["resetAt"].as_str().unwrap().starts_with("2026-11-06"));
        assert_eq!(reading.account.unwrap()["label"], "Qoder");
        let sent = host.sent.borrow();
        assert_eq!(sent[0].0, [("Cookie".to_owned(), "sid=abc".to_owned())]);
        let headers = host.headers.borrow();
        assert!(headers[0]
            .iter()
            .any(|(name, value)| name == "User-Agent" && value == crate::account::WEB_USER_AGENT));
    }

    #[test]
    fn shared_credits_add_to_the_plans_and_each_site_is_its_own_account() {
        let global = json!({
            "total_quota": {"quota_summary": {"used_value": 100, "limit_value": 1000, "usage_percentage": 10}},
            "shared_quota": {"quota_summary": {"used_value": 100, "limit_value": 1000}},
        });
        let china = json!({"totalQuota": {"quotaSummary": {"usedValue": 0, "limitValue": 500}}});
        let host = signed_in(&["qoder.com", "qoder.com.cn"])
            .answer(GLOBAL, Ok(global))
            .answer(CHINA, Ok(china));
        let reading = read(&host).unwrap();
        let used: Vec<_> = reading
            .quotas
            .iter()
            .map(|q| q["usedPercent"].as_f64().unwrap())
            .collect();
        assert_eq!(used, [10.0, 0.0]);
        assert_eq!(reading.additional_accounts[0]["label"], "Qoder CN");
        assert_ne!(
            reading.quotas[0]["accountId"],
            reading.quotas[1]["accountId"]
        );
    }

    #[test]
    fn a_refused_session_is_a_sign_out_and_a_missed_answer_is_not() {
        let host = signed_in(&["qoder.com"]).answer(GLOBAL, Err(HttpFailure::Status(401)));
        let why = read(&host).err().unwrap();
        assert_eq!(why.kind(), "signedOut");
        assert!(why.reason().contains("sign in again"));
        let host = signed_in(&["qoder.com"]).answer(GLOBAL, Err(HttpFailure::Status(503)));
        assert_eq!(read(&host).err().unwrap().kind(), "transient");
        let host = signed_in(&["qoder.com"]).answer(GLOBAL, Ok(json!({"totalQuota": 3})));
        assert_eq!(read(&host).err().unwrap().kind(), "failed");
    }
}
