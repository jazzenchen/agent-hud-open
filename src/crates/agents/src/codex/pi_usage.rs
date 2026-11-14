//! Codex's plan through Pi's ChatGPT sign-in, which spends the same plan.
use super::limits::parse;
use crate::account::{
    client_home_key, read_json, variable, Host, HttpFailure, HttpRequest, Reading, Unread,
};
use crate::l10n::tr;
use serde_json::{json, Value};
use std::time::Duration;

fn pi_signed_out() -> Unread {
    Unread::SignedOut(tr!("Sign in to ChatGPT again in Pi"))
}

/// The route Pi logs its calls through a ChatGPT sign-in under; they spend
/// that account's Codex windows.
pub(super) const PI_ROUTE: &str = "openai-codex";

pub(super) fn read_pi(host: &dyn Host) -> Result<Option<Reading>, Unread> {
    if host.cancelled() {
        return Ok(None);
    }
    let home =
        variable(host, "PI_CODING_AGENT_DIR").unwrap_or_else(|| host.home().join(".pi/agent"));
    let Some(auth) = read_json(host, &home.join("auth.json")) else {
        return Ok(None);
    };
    let auth = &auth["openai-codex"];
    if !auth.is_object() {
        return Ok(None);
    }
    let token = auth["access"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(pi_signed_out)?;
    let account = auth["accountId"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(pi_signed_out)?;
    if auth["type"] != "oauth"
        || auth["expires"]
            .as_f64()
            .is_none_or(|value| value / 1000.0 <= chrono::Utc::now().timestamp() as f64)
    {
        return Err(pi_signed_out());
    }
    let payload = host
        .http_json(&HttpRequest {
            url: "https://chatgpt.com/backend-api/wham/usage",
            secrets: &[("Authorization", &format!("Bearer {token}"))],
            headers: &[("ChatGPT-Account-Id", account)],
            body: None,
            timeout: Duration::from_secs(12),
        })
        .map_err(|failure| match failure {
            HttpFailure::Unreachable | HttpFailure::BadHeader => {
                failure.unread_as(tr!("Pi ChatGPT usage request failed"))
            }
            HttpFailure::Status(401) => pi_signed_out(),
            other => other.unread(),
        })?;
    if payload["account_id"] != account
        || payload["email"]
            .as_str()
            .is_none_or(|value| value.is_empty())
    {
        return Err(tr!("Pi ChatGPT usage returned a different or unknown account").into());
    }
    let window = |value: &Value| json!({"usedPercent":value["used_percent"],"windowDurationMins":value["limit_window_seconds"].as_i64().map(|seconds|seconds/60),"resetsAt":value["reset_at"]});
    let bucket = |value: &Value, id: &str, name: Option<&str>| json!({"limitId":id,"limitName":name,"planType":payload["plan_type"],"primary":window(&value["primary_window"]),"secondary":window(&value["secondary_window"])});
    let mut buckets = serde_json::Map::new();
    if payload["rate_limit"].is_object() {
        buckets.insert(
            "codex".into(),
            bucket(&payload["rate_limit"], "codex", None),
        );
    }
    for additional in payload["additional_rate_limits"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(id) = additional["metered_feature"].as_str() {
            buckets.insert(
                id.into(),
                bucket(
                    &additional["rate_limit"],
                    id,
                    additional["limit_name"].as_str(),
                ),
            );
        }
    }
    Ok(Some(parse(
        host,
        &json!({"accountId":account,"rateLimitsByLimitId":buckets,"rateLimitResetCredits":payload["rate_limit_reset_credits"]}),
        &json!({"type":"chatgpt","email":payload["email"],"planType":payload["plan_type"]}),
        &home.to_string_lossy(),
        &format!(
            "pi:{}",
            client_home_key(host, &home, &host.home().join(".pi/agent"))
        ),
        false,
    )?))
}
