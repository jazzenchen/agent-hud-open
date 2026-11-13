//! Tencent Coding Copilot API client shared across WorkBuddy, CodeBuddy, and CodeBuddy CN.
use super::resources::{read_codebuddy_resources, read_enterprise};
use super::summary::{earliest_cycle_end, finish_workbuddy_reading, FREE_PACKAGES, PAID_PACKAGES};
use super::vscdb::BuddyAuth;
use crate::account::{account_id, Host, HttpFailure, HttpRequest, Reading, Unread, WEB_USER_AGENT};
use crate::l10n;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::time::Duration;

pub(super) struct CopilotSpec {
    pub provider: &'static str,
    pub label: &'static str,
    pub base: &'static str,
    pub summary_billing: bool,
}

pub(super) fn read_copilot(
    host: &dyn Host,
    spec: &CopilotSpec,
    mut auth: BuddyAuth,
) -> Result<Reading, Unread> {
    let base = normalized_base(spec, auth.domain.as_deref());
    let bearer = format!("Bearer {}", auth.access_token);
    if let Ok(profile) = get_accounts(host, &base, &bearer, &auth) {
        if auth.email.is_none() {
            auth.email = str_field(&profile["email"]);
        }
        if auth.nickname.is_none() {
            auth.nickname = str_field(&profile["nickname"]).or_else(|| str_field(&profile["name"]));
        }
        if auth.enterprise_id.is_none() {
            auth.enterprise_id = str_field(&profile["enterpriseId"]);
        }
    }
    let key = auth
        .uid
        .as_deref()
        .or(auth.email.as_deref())
        .unwrap_or(spec.provider);
    // WorkBuddy keeps the id its web sign-in has always had, so an upgrade or
    // a local sign-in coming and going never splits its history.
    let id = if spec.summary_billing {
        account_id("workbuddy", &[super::summary::WEB_HOST])
    } else {
        account_id(spec.provider, &[key])
    };
    let account_label = auth
        .email
        .as_deref()
        .or(auth.nickname.as_deref())
        .unwrap_or(spec.label)
        .to_owned();

    if auth.enterprise_id.as_deref().is_some_and(|s| !s.is_empty()) {
        // An enterprise answer that fails is that failure, not a personal
        // account with no allowance; only "not enterprise" falls through.
        if let Some(reading) =
            read_enterprise(host, spec, &base, &bearer, &auth, &id, &account_label)?
        {
            return Ok(reading);
        }
    }

    if spec.summary_billing {
        read_workbuddy_token_summary(host, spec, &base, &bearer, &auth, &id, &account_label)
    } else {
        read_codebuddy_resources(host, spec, &base, &bearer, &auth, &id, &account_label)
    }
}

fn normalized_base(spec: &CopilotSpec, domain: Option<&str>) -> String {
    if let Some(raw) = domain {
        let host = raw
            .trim()
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/');
        let ok = match spec.provider {
            "codebuddy" => host == "www.codebuddy.ai" || host == "copilot.codebuddy.ai",
            "codebuddy-cn" => host == "www.codebuddy.cn" || host == "copilot.tencent.com",
            _ => host == "copilot.tencent.com" || host == "www.workbuddy.cn",
        };
        if ok {
            return format!("https://{host}");
        }
    }
    spec.base.to_owned()
}

fn headers_for(auth: &BuddyAuth) -> Vec<(&'static str, &str)> {
    let mut headers = vec![
        ("Accept", "application/json, text/plain, */*"),
        ("User-Agent", WEB_USER_AGENT),
    ];
    if let Some(uid) = auth.uid.as_deref() {
        headers.push(("X-User-Id", uid));
    }
    if let Some(eid) = auth.enterprise_id.as_deref() {
        headers.push(("X-Enterprise-Id", eid));
        headers.push(("X-Tenant-Id", eid));
    }
    if let Some(domain) = auth.domain.as_deref() {
        headers.push(("X-Domain", domain));
    }
    headers
}

fn get_accounts(
    host: &dyn Host,
    base: &str,
    bearer: &str,
    auth: &BuddyAuth,
) -> Result<Value, HttpFailure> {
    let headers = headers_for(auth);
    let root = host.http_json(&HttpRequest {
        url: &format!("{base}/v2/plugin/accounts"),
        secrets: &[("Authorization", bearer)],
        headers: &headers,
        body: None,
        timeout: Duration::from_secs(10),
    })?;
    Ok(root["data"]
        .as_array()
        .and_then(|arr| arr.first())
        .cloned()
        .unwrap_or_else(|| root["data"].clone()))
}

fn read_workbuddy_token_summary(
    host: &dyn Host,
    spec: &CopilotSpec,
    base: &str,
    bearer: &str,
    auth: &BuddyAuth,
    id: &str,
    account_label: &str,
) -> Result<Reading, Unread> {
    let summary = post_token(
        host,
        spec,
        &format!("{base}/billing/meter/get-user-resource-summary"),
        bearer,
        auth,
        &json!({}),
    )?;
    let now = Utc::now();
    let reset = token_cycle_end(host, spec, base, bearer, auth, PAID_PACKAGES, false, now)
        .or_else(|| token_cycle_end(host, spec, base, bearer, auth, FREE_PACKAGES, true, now))
        .map(|at| at.to_rfc3339());
    finish_workbuddy_reading(
        spec.provider,
        &summary,
        id,
        account_label,
        "local",
        &auth.email,
        reset,
        now,
    )
}

#[allow(clippy::too_many_arguments)]
fn token_cycle_end(
    host: &dyn Host,
    spec: &CopilotSpec,
    base: &str,
    bearer: &str,
    auth: &BuddyAuth,
    codes: &[&str],
    free: bool,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let route = if free {
        "get-user-resource-free-packages"
    } else {
        "get-user-resource-paid-packages"
    };
    let page = post_token(
        host,
        spec,
        &format!("{base}/billing/meter/{route}"),
        bearer,
        auth,
        &json!({"PageNumber": 1, "PageSize": 100, "PackageCodes": codes, "Status": [0, 3]}),
    )
    .ok()?;
    earliest_cycle_end(&page, free, now)
}

pub(super) fn post_token(
    host: &dyn Host,
    spec: &CopilotSpec,
    url: &str,
    bearer: &str,
    auth: &BuddyAuth,
    body: &Value,
) -> Result<Value, Unread> {
    let headers = headers_for(auth);
    let root = host
        .http_json(&HttpRequest {
            url,
            secrets: &[("Authorization", bearer)],
            headers: &headers,
            body: Some(body),
            timeout: Duration::from_secs(15),
        })
        .map_err(|failure| match failure {
            HttpFailure::Status(401 | 403) => Unread::SignedOut(l10n::text(
                &format!(
                    "{} 保存的登录已失效，请打开 {} 重新登录",
                    spec.label, spec.label
                ),
                &format!(
                    "{}'s saved sign-in has ended; sign in again in {}.",
                    spec.label, spec.label
                ),
            )),
            other => other.unread(),
        })?;
    if let Some(code) = root["code"].as_i64() {
        if code != 0 && code != 200 {
            return Err(Unread::from(l10n::text(
                &format!("{} 的账单服务返回了错误 {code}", spec.label),
                &format!("{}'s billing service answered code {code}.", spec.label),
            )));
        }
    }
    root.get("data")
        .filter(|d| d.is_object())
        .cloned()
        .ok_or_else(|| {
            Unread::from(l10n::text(
                &format!("{} 的积分格式无法识别", spec.label),
                &format!("{}'s credit usage has an unsupported format.", spec.label),
            ))
        })
}

pub(super) fn account_json(
    spec: &CopilotSpec,
    id: &str,
    label: &str,
    plan: Option<&str>,
    email: &Option<String>,
    now: DateTime<Utc>,
) -> Value {
    let mut acc = json!({
        "id": id,
        "provider": spec.provider,
        "label": label,
        "plan": plan,
        "checkedAt": now.to_rfc3339(),
        "signIn": "local",
    });
    if let Some(email) = email {
        acc["email"] = json!(email);
    }
    acc
}

fn str_field(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
