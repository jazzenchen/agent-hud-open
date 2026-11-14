//! The DeepSeek Harness Platform account's wallets, read with the sign-in the
//! Harness keeps.
use crate::account::{account_id, legacy_unresolved, Host, HttpFailure, HttpRequest, Reading};
use crate::l10n::tr;
use chrono::Utc;
use serde_json::{json, Value};
use std::{path::Path, time::Duration};

const PLATFORM_ORIGIN: &str = "https://platform.deepseek.com";
pub(super) const PLATFORM_BALANCE: &str =
    "https://platform.deepseek.com/api/v0/users/get_user_summary";
const PLATFORM_RECORD: &str = "deepseek-account-platform/default";

/// The Harness account plugin accepts decimal or scientific notation for its
/// wallet balances. Keep the API-key reader's stricter contract unchanged.
pub(super) fn platform_amount(value: &Value) -> Option<f64> {
    let raw = value.as_str()?;
    if raw.is_empty() || raw.starts_with('+') || raw.bytes().any(|c| c.is_ascii_whitespace()) {
        return None;
    }
    let amount = raw.parse::<f64>().ok()?;
    amount.is_finite().then_some(amount)
}

pub(super) fn platform_balance(
    host: &dyn Host,
    credentials: &Value,
    directory: &Path,
) -> Result<Option<Reading>, String> {
    let record = &credentials["records"][PLATFORM_RECORD];
    if record.is_null() {
        return Ok(None);
    }
    let payload = &record["payload"];
    let token = payload["token"].as_str().filter(|token| {
        !token.is_empty() && token.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    });
    if record["kind"] != "grant"
        || payload["version"] != 1
        || payload["issuer"] != PLATFORM_ORIGIN
        || token.is_none()
    {
        return Err(tr!(
            "DeepSeek Platform sign-in has an unsupported format or issuer."
        ));
    }
    let platform = match host.os() {
        crate::Os::Windows => "desktop-win",
        crate::Os::MacOs => "desktop-mac",
        crate::Os::Linux => "web",
    };
    let body = host
        .http_json(&HttpRequest {
            url: PLATFORM_BALANCE,
            secrets: &[("x-dsh-auth-token", token.unwrap())],
            body: None,
            headers: &[("x-client-platform", platform)],
            timeout: Duration::from_secs(15),
        })
        .map_err(|failure| match failure {
            HttpFailure::Unreachable | HttpFailure::BadHeader => {
                tr!("DeepSeek balance could not be reached; check the connection.")
            }
            other => other.message(),
        })?;
    if body["code"] == 40003 {
        return Err(HttpFailure::Status(401).message());
    }
    let summary = &body["data"]["biz_data"];
    if body["code"] != 0 || body["data"]["biz_code"] != 0 {
        return Err(tr!("DeepSeek Platform balance could not be read."));
    }
    let scope = host
        .canonical(directory)
        .unwrap_or_else(|| directory.to_path_buf());
    let id = account_id("deepseek", &[scope.to_string_lossy().as_ref()]);
    let identity = legacy_unresolved("DeepSeek", &scope.to_string_lossy());
    let mut billing = Vec::new();
    for (kind, field) in [("normal", "normal_wallets"), ("bonus", "bonus_wallets")] {
        let wallets = summary[field]
            .as_array()
            .ok_or_else(|| tr!("DeepSeek Platform balance has an unsupported format."))?;
        let mut currencies = std::collections::HashSet::new();
        for wallet in wallets {
            let currency = wallet["currency"]
                .as_str()
                .filter(|c| matches!(*c, "CNY" | "USD"))
                .ok_or_else(|| tr!("DeepSeek balance response has an unsupported currency."))?;
            let amount = platform_amount(&wallet["balance"])
                .ok_or_else(|| tr!("DeepSeek balance response has an invalid amount."))?;
            if !currencies.insert(currency) {
                return Err(tr!("DeepSeek Platform balance has an unsupported format."));
            }
            // Harness displays granted wallets only while positive.
            if kind == "bonus" && amount <= 0.0 {
                continue;
            }
            let label = if kind == "normal" {
                tr!("Topped-up balance")
            } else {
                tr!("Granted balance")
            };
            // Platform login wallets are not proven to be an API-key billing
            // route, so do not attach a consumer route or estimate its costs.
            billing.push(json!({"id":format!("{id}:platform:{kind}:{currency}"),"vendor":"deepseek","accountId":identity,"label":label,"amount":amount,"currency":currency}));
        }
    }
    Ok(Some(Reading {
        provider: "deepseek",
        failure: None,
        quotas: Vec::new(),
        billing,
        account: Some(
            json!({"id":identity,"provider":"deepseek","label":"DeepSeek Platform","plan":null,"identity":"local-home","checkedAt":Utc::now().to_rfc3339()}),
        ),
        additional_accounts: Vec::new(),
        notice: None,
        session_records: Vec::new(),
        services: Vec::new(),
        unreviewed_hooks: vec![],
    }))
}
