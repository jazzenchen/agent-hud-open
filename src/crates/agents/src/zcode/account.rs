//! ZCode's (`id: "zcode"`) read-only local account reader.
//! Opens `~/.zcode/v2/credentials.json` (`enc:v1:` AES-256-GCM credentials)
//! in memory and queries `https://zcode.z.ai/api/v1/zcode-plan/billing/balance`.
use crate::account::{
    account_id, read_json_bounded, status, Answer, Host, HttpFailure, HttpRequest, Provider,
    Reading, Unread,
};
use crate::l10n::tr;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::time::Duration;

mod credentials;
use credentials::{
    credential_key, detect_version, extract_profile, open_value, platform_name, str_field, v2_dir,
    MAX_JSON,
};

const BALANCE_URL: &str = "https://zcode.z.ai/api/v1/zcode-plan/billing/balance";

pub static PROVIDER: Provider = Provider {
    local_work: false,
    work_sources: &["zcode"],
    ..Provider::new("zcode", read)
};

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let creds_path = v2_dir(host).join("credentials.json");
    let creds = match read_json_bounded(host, &creds_path, MAX_JSON) {
        Answer::Found(v) => v,
        Answer::Missing => return Ok(Reading::empty("zcode")),
        Answer::Denied => {
            return Err(tr!("Permission denied reading ZCode's local credentials.").into())
        }
        _ => return Err(auth_format().into()),
    };
    let key = credential_key(host);
    let Some(raw_token) = creds.get("zcodejwttoken").and_then(Value::as_str) else {
        return Ok(Reading::empty("zcode"));
    };
    if raw_token.trim().is_empty() {
        return Ok(Reading::empty("zcode"));
    }
    let jwt = open_value(raw_token, &key).ok_or_else(|| {
        tr!(
            "{{name}}'s encrypted sign-in could not be unlocked.",
            name = "ZCode"
        )
    })?;
    if jwt.trim().is_empty() {
        return Ok(Reading::empty("zcode"));
    }

    let (uid, email, name) = extract_profile(&creds, &key, &jwt);
    let version = detect_version(host);
    let platform = platform_name(host.os());
    let ua = format!("Mozilla/5.0 ZCode/{version}");
    let url = format!("{BALANCE_URL}?app_version={version}");
    let bearer = format!("Bearer {}", jwt.trim());
    let body = host
        .http_json(&HttpRequest {
            url: &url,
            secrets: &[("Authorization", &bearer)],
            headers: &[
                ("Accept", "application/json"),
                ("Origin", "https://zcode.z.ai"),
                ("Referer", "https://zcode.z.ai/"),
                ("User-Agent", &ua),
                ("X-ZCode-Version", &version),
                ("X-ZCode-Platform", platform),
            ],
            body: None,
            timeout: Duration::from_secs(15),
        })
        .map_err(|failure| match failure {
            HttpFailure::Status(401 | 403) => Unread::SignedOut(tr!(
                "{{name}}'s saved sign-in has ended; sign in again in {{name}}.",
                name = "ZCode"
            )),
            other => other.unread(),
        })?;
    parse_balance(&body, uid.as_deref(), email, name)
}

fn parse_balance(
    root: &Value,
    uid: Option<&str>,
    email: Option<String>,
    name: Option<String>,
) -> Result<Reading, Unread> {
    if root["success"] == false {
        return Err(format_err());
    }
    if let Some(code) = root["code"].as_i64() {
        if code != 0 && code != 200 {
            return Err(format_err());
        }
    }
    let data = root.get("data").filter(|d| d.is_object()).unwrap_or(root);
    let plans = data["plans"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let plan = plans
        .iter()
        .find(|p| p["status"] == "active")
        .or_else(|| plans.first())
        .and_then(|p| {
            str_field(&p["name"])
                .or_else(|| str_field(&p["plan_name"]))
                .or_else(|| str_field(&p["plan_id"]))
        });

    let id = account_id("zcode", &[uid.or(email.as_deref()).unwrap_or("zcode")]);
    let label = email
        .as_deref()
        .or(name.as_deref())
        .unwrap_or("ZCode")
        .to_owned();

    let mut reading = Reading::empty("zcode");
    let balances = data["balances"].as_array().ok_or_else(format_err)?;
    for (idx, item) in balances.iter().enumerate() {
        let total = num(item
            .get("total_units")
            .or_else(|| item.get("total_balance"))
            .or_else(|| item.get("total")));
        let Some(total) = total.filter(|&t| t > 0.0) else {
            continue;
        };
        let used = num(item
            .get("used_units")
            .or_else(|| item.get("used_balance"))
            .or_else(|| item.get("used")))
        .or_else(|| {
            let rem = num(item
                .get("remaining_units")
                .or_else(|| item.get("available_units"))
                .or_else(|| item.get("remaining_balance")))?;
            Some((total - rem).max(0.0))
        })
        .unwrap_or(0.0);
        let share = (used / total * 100.0).clamp(0.0, 100.0);
        let title = str_field(&item["name"])
            .or_else(|| str_field(&item["cycle_type"]))
            .unwrap_or_else(|| "Plan balance".into());
        let reset = parse_reset(
            item.get("period_end")
                .or_else(|| item.get("expires_at"))
                .or_else(|| item.get("next_reset_time")),
        );
        let suffix = if balances.len() == 1 {
            "balance".into()
        } else {
            format!("balance-{idx}")
        };
        reading.quotas.push(json!({
            "id": format!("{id}:{suffix}"),
            "vendor": "zcode",
            "consumerSource": "zcode",
            "accountId": id,
            "account": label,
            "name": title,
            "shortLabel": title,
            "usedPercent": share,
            "resetAt": reset,
            "allModels": true,
            "status": status(share),
            "nativeUsage": {"used": used, "capacity": total, "unit": "units"},
        }));
    }
    if reading.quotas.is_empty() {
        reading.notice = Some(tr!(
            "ZCode reports no active plan allowance for this account."
        ));
    }
    let mut acc = json!({
        "id": id,
        "provider": "zcode",
        "label": label,
        "plan": plan,
        "checkedAt": Utc::now().to_rfc3339(),
        "signIn": "local",
    });
    if let Some(email) = email {
        acc["email"] = json!(email);
    }
    reading.account = Some(acc);
    Ok(reading)
}

fn num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|n| n.is_finite() && *n >= 0.0)
}

fn parse_reset(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::Number(n) => n.as_i64().filter(|&at| at > 0).and_then(|at| {
            let ms = if at > 10_000_000_000 { at } else { at * 1000 };
            DateTime::<Utc>::from_timestamp_millis(ms)
        }),
        Value::String(s) => s
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|&at| at > 0)
            .and_then(|at| {
                let ms = if at > 10_000_000_000 { at } else { at * 1000 };
                DateTime::<Utc>::from_timestamp_millis(ms)
            })
            .or_else(|| {
                DateTime::parse_from_rfc3339(s.trim())
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
            }),
        _ => None,
    }
    .map(|at| at.to_rfc3339())
}

fn auth_format() -> String {
    tr!("ZCode credentials have an unsupported format.")
}

fn format_err() -> Unread {
    Unread::from(tr!("ZCode's balance response has an unsupported format."))
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
