//! The readings record: each enabled agent's accounts with their quota
//! windows, balances and reset credits, as the phone shows them.
use super::milliseconds;
use crate::{keys, SyncRecord};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn readings(device: &str, snapshot: &Value, now: i64) -> SyncRecord {
    let accounts = snapshot["accounts"].as_array().cloned().unwrap_or_default();
    let quotas = snapshot["quotas"].as_array().cloned().unwrap_or_default();
    let billings = snapshot["billing"].as_array().cloned().unwrap_or_default();
    let mut providers = Vec::new();
    let mut names = BTreeMap::new();
    for p in snapshot["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["enabled"] == true)
    {
        let Some(id) = p["id"].as_str() else {
            continue;
        };
        let name = p["name"].as_str().unwrap_or(id);
        names.insert(id.to_owned(), name.to_owned());
        let mut rows = accounts
            .iter()
            .filter(|a| a["provider"] == id)
            .collect::<Vec<_>>();
        if rows.is_empty() {
            rows.push(&Value::Null);
        }
        for account in rows {
            let mut windows = Vec::new();
            for q in quotas.iter().filter(|q| {
                q["vendor"] == id
                    && (account.is_null()
                        || q["accountId"].is_null()
                        || q["accountId"] == account["id"])
            }) {
                let Some(used) = q["usedPercent"].as_f64().filter(|n| n.is_finite()) else {
                    continue;
                };
                let stale = q["status"] == "stale" || account["stale"] == true;
                let observed = milliseconds(&q["observedAt"])
                    .or_else(|| milliseconds(&account["checkedAt"]))
                    .or_else(|| milliseconds(&p["accountStatus"]["checkedAt"]));
                let Some(observed) = observed else {
                    continue;
                };
                let window_name = q["name"].as_str().unwrap_or("");
                let short_name = q["shortLabel"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| window_name.to_owned());
                let remaining = (100.0 - used).clamp(0.0, 100.0);
                let id_str = q["id"].as_str().unwrap_or("");
                let window_id = clean_window_id(id_str, id, account["id"].as_str());
                let all_models = q["allModels"].as_bool().unwrap_or(false);
                let mut quota = json!({"windowID":window_id,"windowName":q["name"],"windowShortName":short_name,"allModels":all_models,"status":if stale{"readFailed"}else{"normal"},"remainingPercent":remaining,"observedAtMs":observed});
                if let Some(seconds) = q["windowDurationSeconds"]
                    .as_f64()
                    .filter(|n| n.is_finite() && *n >= 0.0)
                {
                    quota["windowSeconds"] = json!(seconds.round().min(i32::MAX as f64) as i64);
                }
                if let Some(reset) = milliseconds(&q["resetAt"]) {
                    quota["resetsAtMs"] = json!(reset);
                }
                if !stale {
                    quota["level"] = json!(match q["status"].as_str() {
                        Some("critical") => "critical",
                        Some("warning") => "warning",
                        _ => "ok",
                    });
                    if let Some(rate) = q["metrics"]["burnRatePctPerHour"]
                        .as_f64()
                        .filter(|n| n.is_finite() && *n >= 0.0)
                    {
                        quota["burnRatePercentPerHour"] = json!((rate * 10.0).round() / 10.0);
                    }
                    if let Some(rate) = q["metrics"]["tokensPerHour"]
                        .as_f64()
                        .filter(|n| n.is_finite() && *n >= 0.0)
                    {
                        quota["tokensPerHour"] = json!(rate);
                    }
                    let outlook = &q["metrics"]["outlook"];
                    match outlook["kind"].as_str().or_else(|| q["outlook"].as_str()) {
                        Some("exhausted") => {
                            quota["outlook"] = json!("exhausted");
                        }
                        Some("no-estimate" | "noEstimate") => {
                            quota["outlook"] = json!("noEstimate");
                        }
                        Some("no-usage" | "noUsage") => {
                            quota["outlook"] = json!("noUsage");
                        }
                        Some("insufficient-data" | "insufficientData") => {
                            quota["outlook"] = json!("insufficientData");
                        }
                        Some("exhausts") => {
                            if outlook["beforeReset"] == false {
                                if let Some(used_reset) = outlook["projectedUsedPercent"]
                                    .as_f64()
                                    .filter(|n| n.is_finite())
                                {
                                    let rounded =
                                        ((used_reset * 10.0).round() / 10.0).clamp(0.0, 100.0);
                                    quota["outlook"] = json!("outlastsReset");
                                    quota["usedAtResetPercent"] = json!(rounded);
                                }
                            } else if let Some(at) = milliseconds(&outlook["exhaustsAt"])
                                .or_else(|| milliseconds(&q["exhaustsAtMs"]))
                                .filter(|at| *at > observed)
                            {
                                quota["outlook"] = json!("exhausts");
                                quota["exhaustsAtMs"] = json!(at);
                            }
                        }
                        Some("outlastsReset") => {
                            if let Some(used_reset) =
                                q["usedAtResetPercent"].as_f64().filter(|n| n.is_finite())
                            {
                                let rounded =
                                    ((used_reset * 10.0).round() / 10.0).clamp(0.0, 100.0);
                                quota["outlook"] = json!("outlastsReset");
                                quota["usedAtResetPercent"] = json!(rounded);
                            }
                        }
                        _ => {}
                    }
                }
                let caps = if q["weeklyCaps"].is_object() {
                    &q["weeklyCaps"]
                } else {
                    &q["metrics"]["weeklyCaps"]
                };
                if caps.is_object() {
                    quota["weeklyCaps"] = caps.clone();
                }
                windows.push(quota);
            }
            let mut balances = Vec::new();
            let mut billing_id = None;
            for b in billings.iter().filter(|b| {
                b["vendor"] == id
                    && (account.is_null()
                        || b["accountId"].is_null()
                        || b["accountId"] == account["id"])
            }) {
                let Some(currency) = b["currency"].as_str() else {
                    continue;
                };
                let amount_f64 = b["amount"]
                    .as_f64()
                    .or_else(|| b["amount"].as_str().and_then(|s| s.parse::<f64>().ok()));
                let Some(amount_str) = b["amount"]
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| amount_f64.filter(|n| n.is_finite()).map(|n| n.to_string()))
                else {
                    continue;
                };
                let stale = b["status"] == "stale" || account["stale"] == true;
                let observed = milliseconds(&b["observedAt"])
                    .or_else(|| milliseconds(&account["checkedAt"]))
                    .or_else(|| milliseconds(&p["accountStatus"]["checkedAt"]))
                    .unwrap_or(now);
                let mut balance = json!({
                    "currency": currency,
                    "amount": amount_str,
                    "observedAtMs": observed,
                    "status": if stale { "readFailed" } else { "normal" }
                });
                if !stale {
                    let level = if !b["notice"].is_null() || amount_f64.is_some_and(|a| a <= 0.0) {
                        "critical"
                    } else if amount_f64.is_some_and(|a| {
                        if currency == "CNY" {
                            a <= 20.0
                        } else {
                            a <= 5.0
                        }
                    }) {
                        "warning"
                    } else {
                        "ok"
                    };
                    balance["level"] = json!(level);
                    if let Some(runs_out) = milliseconds(&b["runsOutAt"])
                        .or_else(|| milliseconds(&b["metrics"]["periods"]["day"]["runsOutAt"]))
                    {
                        balance["runsOutAtMs"] = json!(runs_out);
                    }
                }
                if billing_id.is_none() {
                    billing_id = b["billingId"]
                        .as_str()
                        .or_else(|| {
                            b["id"]
                                .as_str()
                                .map(|s| s.rsplit_once(':').map(|(prefix, _)| prefix).unwrap_or(s))
                        })
                        .map(str::to_owned);
                }
                balances.push(balance);
            }
            balances.sort_by(|a, b| a["currency"].as_str().cmp(&b["currency"].as_str()));
            let reset_credits = account["resetCredits"].as_object().and_then(|rc| {
                let available_count = rc.get("availableCount").and_then(Value::as_i64)?.max(0);
                let observed = rc
                    .get("observedAtMs")
                    .and_then(Value::as_i64)
                    .or_else(|| rc.get("observedAt").and_then(milliseconds))
                    .or_else(|| milliseconds(&account["checkedAt"]))
                    .or_else(|| milliseconds(&p["accountStatus"]["checkedAt"]))
                    .unwrap_or(now);
                let stale = account["stale"] == true || p["accountStatus"]["status"] == "error";
                let status = rc
                    .get("status")
                    .and_then(Value::as_str)
                    .filter(|s| matches!(*s, "normal" | "readFailed" | "unverified"))
                    .unwrap_or(if stale { "readFailed" } else { "normal" });
                let mut out = json!({
                    "availableCount": available_count,
                    "observedAtMs": observed,
                    "status": status,
                });
                if let Some(items) = rc.get("credits").and_then(Value::as_array) {
                    let mapped = items
                        .iter()
                        .map(|item| {
                            if let Some(exp) = item
                                .get("expiresAtMs")
                                .and_then(Value::as_i64)
                                .or_else(|| item.get("expiresAt").and_then(milliseconds))
                                .or_else(|| item.get("expirationDate").and_then(milliseconds))
                            {
                                json!({ "expiresAtMs": exp })
                            } else {
                                json!({})
                            }
                        })
                        .collect::<Vec<_>>();
                    out["credits"] = json!(mapped);
                }
                Some(out)
            });
            if !windows.is_empty() || !balances.is_empty() || reset_credits.is_some() {
                let costs_complete = snapshot["indexing"]["complete"] == true;
                let mut value = json!({"provider":id,"vendor":id,"accountLabel":account["label"].as_str().unwrap_or(name),"costsComplete":costs_complete,"quotas":windows,"balances":balances});
                for (from, to) in [
                    ("id", "accountID"),
                    ("accountKey", "accountKey"),
                    ("plan", "plan"),
                ] {
                    if !account[from].is_null() {
                        value[to] = account[from].clone();
                    }
                }
                if value["accountKey"].is_null()
                    && account["identity"] == "account"
                    && !account["id"].is_null()
                {
                    value["accountKey"] = account["id"].clone();
                }
                if let Some(billing_id) = billing_id {
                    value["billingID"] = json!(billing_id);
                }
                if let Some(reset_credits) = reset_credits {
                    value["resetCredits"] = reset_credits;
                }
                providers.push(value);
            }
        }
    }
    SyncRecord::new(
        "readings",
        device,
        keys::SHARED,
        json!({"providers":providers,"names":names}),
        now,
        None,
    )
}
/// Whether an enabled agent's accounts are still being read. A readings record
/// with no providers is held back meanwhile, so it never goes out as none.
pub fn accounts_loading(snapshot: &Value) -> bool {
    snapshot["providers"].as_array().is_some_and(|list| {
        list.iter().any(|p| {
            p["enabled"] == true
                && (p["accountStatus"]["status"] == "loading"
                    || (p["detected"] == true && p["accountStatus"].is_null()))
        })
    })
}
/// What a readings record says was read, without what the clock works out
/// from it each pass: token rates, run-out times and projections, and the
/// pass's own time where a reading names none. Readings go out again only when
/// this changes, as native `ReadingChanges` has them.
pub fn reading_facts(payload: &Value, now: i64) -> Value {
    fn strip(entry: &mut Value, keys: &[&str], now: i64) {
        if let Some(entry) = entry.as_object_mut() {
            for key in keys {
                entry.remove(*key);
            }
            if entry.get("observedAtMs").and_then(Value::as_i64) == Some(now) {
                entry.remove("observedAtMs");
            }
        }
    }
    let mut facts = payload.clone();
    for provider in facts["providers"].as_array_mut().into_iter().flatten() {
        for quota in provider["quotas"].as_array_mut().into_iter().flatten() {
            strip(
                quota,
                &["tokensPerHour", "exhaustsAtMs", "usedAtResetPercent"],
                now,
            );
        }
        for balance in provider["balances"].as_array_mut().into_iter().flatten() {
            strip(balance, &["runsOutAtMs"], now);
        }
        strip(&mut provider["resetCredits"], &[], now);
    }
    facts
}
fn clean_window_id(raw_id: &str, vendor: &str, account_id: Option<&str>) -> String {
    if let Some(acct) = account_id {
        if let Some(rest) = raw_id
            .strip_prefix(acct)
            .and_then(|s| s.strip_prefix('/').or_else(|| s.strip_prefix(':')))
        {
            if !rest.is_empty() {
                return rest.to_owned();
            }
        }
    }
    if let Some(after_vendor) = raw_id
        .strip_prefix(vendor)
        .and_then(|s| s.strip_prefix(':'))
    {
        if let Some((hash, rest)) = after_vendor.split_once(':') {
            if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) && !rest.is_empty() {
                return rest.to_owned();
            }
        }
    }
    raw_id.to_owned()
}
