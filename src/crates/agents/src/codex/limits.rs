//! Codex's plan windows and credits as its app-server reports them, and the
//! sign-in identities Agent HUD remembers for them.
use crate::account::{account_id, legacy_account, legacy_unresolved, status, Host, Reading};
use crate::l10n;
use crate::naming::{vendor_windows, window_names};
use serde_json::{json, Value};

/// Where Agent HUD remembers which email signed in to which workspace.
pub(super) const IDENTITIES: &str = "codex-identities.json";

/// The workspace and email of a sign-in, completed from what Agent HUD
/// remembers when Codex reports only one of them.
fn remember_identity(
    host: &dyn Host,
    home: &str,
    workspace: &str,
    email: &str,
) -> (String, String) {
    const LIMIT: u64 = 16 * 1024 * 1024;
    let mut map = host
        .read_own(IDENTITIES, LIMIT)
        .found()
        .filter(|bytes| bytes.len() as u64 <= LIMIT)
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    let mut ws = workspace.trim().to_owned();
    let mut em = email.trim().to_owned();
    let prefix = format!("{home}/");
    if !ws.is_empty() && !em.is_empty() {
        let key = format!("{prefix}{ws}");
        if map.get(&key).and_then(Value::as_str) != Some(em.as_str()) {
            map.insert(key, json!(em));
            let _ = host.write_own(IDENTITIES, Value::Object(map).to_string().as_bytes());
        }
    } else if !ws.is_empty() && em.is_empty() {
        if let Some(saved) = map
            .get(&format!("{prefix}{ws}"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            em = saved.to_owned();
        }
    } else if ws.is_empty() && !em.is_empty() {
        let matches = map
            .iter()
            .filter_map(|(k, v)| {
                let rest = k.strip_prefix(&prefix)?;
                v.as_str()
                    .filter(|s| s.eq_ignore_ascii_case(&em))
                    .map(|_| rest.to_owned())
            })
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            ws = matches[0].clone();
        }
    }
    (ws, em)
}
fn parse_reset_credits(limits: &Value) -> Option<Value> {
    let raw = limits
        .get("rateLimitResetCredits")
        .or_else(|| limits.get("rate_limit_reset_credits"))?;
    let count = raw
        .get("availableCount")
        .or_else(|| raw.get("available_count"))
        .and_then(Value::as_u64)?;
    let credits = raw.get("credits").and_then(Value::as_array).map(|items| {
        let mut parsed = items
            .iter()
            .filter_map(|item| {
                let id = item.get("id")?.as_str()?.to_owned();
                let raw_exp = item.get("expiresAt").or_else(|| item.get("expires_at"));
                let (sort_ts, exp_iso) =
                    if let Some(secs) = raw_exp.and_then(Value::as_f64).filter(|n| n.is_finite()) {
                        let iso = chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0)
                            .map(|d| d.to_rfc3339());
                        (Some((secs * 1000.0) as i64), iso)
                    } else if let Some(s) = raw_exp.and_then(Value::as_str) {
                        let ts = chrono::DateTime::parse_from_rfc3339(s)
                            .ok()
                            .map(|d| d.timestamp_millis());
                        (ts, Some(s.to_owned()))
                    } else {
                        (None, None)
                    };
                Some((
                    sort_ts.unwrap_or(i64::MAX),
                    id.clone(),
                    json!({"id": id, "expiresAt": exp_iso}),
                ))
            })
            .collect::<Vec<_>>();
        parsed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        parsed.into_iter().map(|(_, _, v)| v).collect::<Vec<_>>()
    });
    let mut out = json!({"availableCount": count});
    if let Some(credits) = credits {
        out["credits"] = json!(credits);
    }
    Some(out)
}
pub(super) fn parse(
    host: &dyn Host,
    limits: &Value,
    account: &Value,
    home: &str,
    home_key: &str,
    native: bool,
) -> Result<Reading, String> {
    let mut reading = Reading::empty("codex");
    let raw_workspace = limits["accountId"].as_str().unwrap_or_default();
    let raw_email = account["email"].as_str().unwrap_or_default();
    let (workspace_str, email_str) = remember_identity(host, home, raw_workspace, raw_email);
    let workspace = workspace_str.as_str();
    let email = email_str.as_str();
    let identity = if workspace.is_empty() && email.is_empty() {
        account_id("codex", &["unresolved", home])
    } else {
        account_id("codex", &[&email.to_lowercase(), workspace])
    };
    let legacy = legacy_account("Codex", &email.to_lowercase(), workspace);
    let account_identity = legacy.unwrap_or_else(|| legacy_unresolved("Codex", home_key));
    // A present multi-bucket map is authoritative, an empty one included;
    // only without it does the single legacy bucket count (agent-hud-native-open
    // CodexRateLimits.buckets).
    let mut buckets = match limits["rateLimitsByLimitId"].as_object() {
        Some(map) => map
            .iter()
            .map(|(id, value)| (id.clone(), value))
            .collect::<Vec<_>>(),
        None => limits["rateLimits"]
            .is_object()
            .then(|| &limits["rateLimits"])
            .map(|value| {
                (
                    value["limitId"].as_str().unwrap_or("codex").to_owned(),
                    value,
                )
            })
            .into_iter()
            .collect(),
    };
    buckets.sort_by(|a, b| match (a.0.as_str(), b.0.as_str()) {
        ("codex", "codex") => std::cmp::Ordering::Equal,
        ("codex", _) => std::cmp::Ordering::Less,
        (_, "codex") => std::cmp::Ordering::Greater,
        _ => a.0.cmp(&b.0),
    });
    // The plan the first bucket names, else the signed-in account's.
    let plan = buckets
        .iter()
        .find_map(|(_, bucket)| bucket["planType"].as_str())
        .or_else(|| account["planType"].as_str());
    if account.is_object() || !workspace.is_empty() || !email.is_empty() {
        let mut acct = json!({"id":account_identity,"provider":"codex","label":if email.is_empty(){None}else{Some(email)},"plan":plan});
        if let Some(credits) = parse_reset_credits(limits) {
            acct["resetCredits"] = credits;
        }
        reading.account = Some(acct);
    }
    for (bucket_id, bucket) in buckets {
        // Named as Codex's own client names them; another bucket than `codex`
        // with its name first. Short names give the `codex` bucket's windows
        // their period alone and another bucket's its word, with the period
        // when the bucket has two windows (Spark 5h, Spark 7d, Reserve).
        let service = bucket["limitName"].as_str().unwrap_or(&bucket_id);
        let windows = ["primary", "secondary"]
            .iter()
            .filter(|slot| {
                bucket[**slot]["usedPercent"]
                    .as_f64()
                    .is_some_and(f64::is_finite)
            })
            .count();
        for slot in ["primary", "secondary"] {
            let window = &bucket[slot];
            let Some(percent) = window["usedPercent"]
                .as_f64()
                .filter(|percent| percent.is_finite())
            else {
                continue;
            };
            let secondary = slot == "secondary";
            let period = window_names::Period::from_seconds(
                window["windowDurationMins"]
                    .as_f64()
                    .map(|minutes| minutes * 60.0),
            );
            let limit = vendor_windows::codex_limit_name(period, secondary);
            let alone = period.map_or_else(
                || if secondary { "Secondary" } else { "Usage" }.to_owned(),
                |period| period.short_name(),
            );
            let (name, short) = if bucket_id == "codex" {
                (limit, alone)
            } else {
                let word = window_names::catalog_window_word("codex", service);
                let short = if windows > 1 {
                    format!(
                        "{word} {}",
                        period.map_or(alone, |period| period.after_word())
                    )
                } else {
                    word
                };
                (
                    format!(
                        "{} · {limit}",
                        window_names::catalog_window("codex", service)
                    ),
                    short,
                )
            };
            let reset = window["resetsAt"]
                .as_f64()
                .filter(|value| value.is_finite())
                .and_then(|value| chrono::DateTime::<chrono::Utc>::from_timestamp(value as i64, 0))
                .map(|date| date.to_rfc3339());
            let key = if bucket_id == "codex" && slot == "primary" {
                "codex".into()
            } else {
                format!("codex:{bucket_id}:{slot}")
            };
            reading.quotas.push(json!({"id":format!("{identity}:{key}"),"vendor":"codex","name":name,"shortLabel":short,"usedPercent":percent,"resetAt":reset,"status":status(percent),"account":if email.is_empty(){None}else{Some(email)},"accountId":account_identity,"legacyWindowId":format!("{account_identity}/{key}"),"consumerSource":if native{Some("codex")}else{None},"consumerRoutes":if native{None}else{Some([super::pi_usage::PI_ROUTE])},"allModels":bucket_id=="codex","windowDurationSeconds":window["windowDurationMins"].as_f64().filter(|value|*value>0.0).map(|value|value*60.0)}));
        }
    }
    if reading.quotas.is_empty() {
        reading.notice = Some(l10n::text(
            "Codex 当前登录没有返回额度窗口",
            "Codex returned no quota windows for its current login",
        ));
    }
    Ok(reading)
}
