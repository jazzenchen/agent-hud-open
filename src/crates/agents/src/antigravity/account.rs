//! Antigravity's quota (Open's AntigravityService and AntigravityClient):
//! CLI status-line callbacks identify each account; running language servers
//! supply accounts without callbacks. Anonymous `/usage` is the last fallback.
use super::{ACCOUNT_SERVICE, AGENT};
use crate::account::{account_id, legacy_account, legacy_unresolved, status, Host, Reading};
use crate::l10n;
use crate::naming::{
    vendor_windows,
    window_names::{self, Period},
};
use chrono::Utc;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

fn parse_summary_quotas(
    id: &str,
    account_identity: &str,
    email: Option<&str>,
    summary: &Value,
) -> Option<Vec<Value>> {
    let groups = summary["response"]["groups"]
        .as_array()
        .or_else(|| summary["groups"].as_array())?;
    let mut ids = BTreeSet::new();
    let mut active_groups = BTreeSet::new();
    let mut quotas = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        let buckets = group["buckets"].as_array()?;
        let group_name = group["displayName"]
            .as_str()
            .or_else(|| group["name"].as_str())
            .unwrap_or("");
        for bucket in buckets {
            let key = bucket["bucketId"]
                .as_str()
                .or_else(|| bucket["id"].as_str())
                .filter(|id| !id.is_empty())?;
            if !ids.insert(key) {
                return None;
            }
            let Some(fraction) = bucket["remainingFraction"]
                .as_f64()
                .or_else(|| bucket["remaining_fraction"].as_f64())
                .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            else {
                continue;
            };
            active_groups.insert(index);
            let used = (1.0 - fraction) * 100.0;
            let bucket_name = bucket["displayName"]
                .as_str()
                .or_else(|| bucket["name"].as_str())
                .unwrap_or(key);
            let name = if group_name.is_empty() {
                bucket_name.to_owned()
            } else {
                format!("{group_name} · {bucket_name}")
            };
            let duration = match bucket["window"].as_str() {
                Some("weekly") => Some(604800),
                Some("5h") => Some(18000),
                _ => None,
            };
            let word = vendor_windows::antigravity_group_word(group_name);
            let short = Period::from_seconds(duration.map(f64::from)).map_or_else(
                || word.clone(),
                |period| format!("{word} {}", period.after_word()),
            );
            let reset = bucket["resetTime"]
                .as_str()
                .or_else(|| bucket["reset_time"].as_str())
                .filter(|value| chrono::DateTime::parse_from_rfc3339(value).is_ok());
            let lower = group_name.to_lowercase();
            let mut quota = json!({
                "id": format!("{id}:{key}"),
                "vendor": "antigravity",
                "accountId": account_identity,
                "account": email,
                "name": name,
                "shortLabel": short,
                "usedPercent": used,
                "status": status(used),
                "resetAt": reset,
                "windowDurationSeconds": duration,
            });
            if lower.contains("gemini") {
                quota["modelFamilies"] = json!(["gemini"]);
            } else if lower.contains("claude") || lower.contains("gpt") {
                quota["modelFamilies"] = json!(["claude", "gpt"]);
            }
            quotas.push(quota);
        }
    }
    if quotas.is_empty() {
        return None;
    }
    let all = active_groups.len() == 1;
    for quota in &mut quotas {
        quota["allModels"] = json!(all);
        if all {
            if let Some(period) = Period::from_seconds(quota["windowDurationSeconds"].as_f64()) {
                quota["shortLabel"] = json!(period.short_name());
            }
        }
    }
    Some(quotas)
}

fn parse(scope: &str, body: &Value, summary: &Value) -> Result<Reading, String> {
    let user = &body["userStatus"];
    let configs = user["cascadeModelConfigData"]["clientModelConfigs"]
        .as_array()
        .ok_or_else(|| {
            l10n::text(
                "Antigravity 本地服务返回的额度格式无法识别",
                "Antigravity local service returned an unsupported quota format.",
            )
        })?;
    let email = user["email"]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let lower_email = email.map(|email| email.to_lowercase());
    let id = if let Some(email) = lower_email.as_deref() {
        account_id(
            "antigravity",
            &[email, user["teamId"].as_str().unwrap_or("")],
        )
    } else {
        account_id("antigravity", &[scope])
    };
    let identified = legacy_account(
        "Antigravity",
        lower_email.as_deref().unwrap_or(""),
        user["teamId"].as_str().unwrap_or(""),
    );
    let account_identity = identified
        .clone()
        .unwrap_or_else(|| legacy_unresolved("Antigravity", scope));
    let quotas = if let Some(quotas) = parse_summary_quotas(&id, &account_identity, email, summary)
    {
        quotas
    } else {
        let mut pools = BTreeMap::<String, Value>::new();
        for config in configs {
            let Some(fraction) = config["quotaInfo"]["remainingFraction"]
                .as_f64()
                .filter(|fraction| fraction.is_finite() && (0.0..=1.0).contains(fraction))
            else {
                continue;
            };
            let model = config["modelOrAlias"]["model"].as_str().unwrap_or("");
            let label = config["label"].as_str().unwrap_or(model);
            let lower = format!("{model} {label}").to_lowercase();
            if ["lite", "autocomplete", "image"]
                .iter()
                .any(|word| lower.contains(word))
            {
                continue;
            }
            let family = if lower.contains("gemini") {
                "gemini"
            } else if lower.contains("claude") || lower.contains("gpt") {
                "claude-gpt"
            } else {
                model
            };
            if family.is_empty() {
                continue;
            }
            let name = match family {
                "gemini" => "Gemini Models",
                "claude-gpt" => "Claude and GPT models",
                _ => label,
            };
            let used = (1.0 - fraction) * 100.0;
            if pools
                .get(family)
                .is_some_and(|row| row["usedPercent"].as_f64().unwrap_or(0.0) >= used)
            {
                continue;
            }
            let reset = config["quotaInfo"]["resetTime"]
                .as_str()
                .filter(|value| chrono::DateTime::parse_from_rfc3339(value).is_ok());
            // Gemini, third-party for Claude and GPT as Antigravity's plans call
            // them, and any other model by its word.
            let short = match family {
                "gemini" => "Gemini".to_owned(),
                "claude-gpt" => vendor_windows::antigravity_group_word(name),
                _ => window_names::word(label),
            };
            let mut quota = json!({"id":format!("{id}:legacy:{family}"),"vendor":"antigravity","name":name,"shortLabel":short,
                "usedPercent":used,"resetAt":reset,"status":status(used),"account":email,"accountId":account_identity});
            // The Swift app kept these GetUserStatus pools as `antigravity:legacy:<family>`;
            // an account it did not name had another home key there.
            if let Some(account) = &identified {
                quota["legacyWindowId"] = json!(format!("{account}/antigravity:legacy:{family}"));
            }
            match family {
                "gemini" => quota["modelFamilies"] = json!(["gemini"]),
                "claude-gpt" => quota["modelFamilies"] = json!(["claude", "gpt"]),
                _ => {}
            }
            // GetUserStatus supplies a reset, but no window length or native token
            // capacity. Keep those unknown rather than inferring a five-hour cycle.
            pools.insert(family.into(), quota);
        }
        let all_models = pools.len() == 1;
        pools
            .into_values()
            .map(|mut quota| {
                quota["allModels"] = json!(all_models);
                quota
            })
            .collect::<Vec<_>>()
    };
    let notice = quotas.is_empty().then(|| {
        l10n::text(
            "Antigravity 已连接，但本地服务未提供额度比例",
            "Antigravity is connected; its local service reports no quota percentage.",
        )
    });
    Ok(Reading {
        provider: "antigravity",
        quotas,
        billing: Vec::new(),
        notice,
        additional_accounts: Vec::new(),
        session_records: Vec::new(),
        services: Vec::new(),
        account: Some(
            json!({"id":account_identity,"provider":"antigravity","label":email,
            "identity":if email.is_some(){"account"}else{"local-home"},
            "plan":user["userTier"]["name"].as_str().or_else(||user["planStatus"]["planInfo"]["planName"].as_str()),
            "checkedAt":Utc::now().to_rfc3339()}),
        ),
    })
}

pub fn read(host: &dyn Host) -> Result<Reading, String> {
    let mut reading = Reading::empty("antigravity");
    let mut partial = false;
    for report in host.account_reports("antigravity") {
        match super::statusline::parse(&report) {
            Some(next) => merge(&mut reading, next),
            None => partial = true,
        }
    }
    if !ACCOUNT_SERVICE.runs_on(host.os()) && reading.account.is_none() {
        reading.notice = Some(format!(
            "{} ({})",
            l10n::text(
                "此系统暂不支持查找 Antigravity 本地服务",
                "Antigravity local-service discovery is not implemented here"
            ),
            host.os().name()
        ));
        return Ok(reading);
    }
    // The CLI supplies exact windows; GetUserStatus only supplies model-family
    // totals with unknown duration. Do not show both as separate subscriptions
    // or replace a sampled CLI window with an incomparable family total.
    let reported: BTreeSet<String> = reading
        .quotas
        .iter()
        .filter_map(|quota| quota["accountId"].as_str().map(str::to_owned))
        .collect();
    let native = ACCOUNT_SERVICE.runs_on(host.os()).then(||host.native(
        AGENT.id,
        "ReadAccounts",
        &json!({"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en","ideVersion":"unknown"}}),
    ));
    let mut executable = None;
    if let Some(native) = native {
        match native {
            Ok(body) => {
                executable = body["cliExecutable"].as_str().map(str::to_owned);
                partial |= body["partial"].as_bool().unwrap_or(false);
                if let Some(accounts) = body["accounts"].as_array() {
                    for account in accounts {
                        let Some(scope) =
                            account["source"].as_str().filter(|value| !value.is_empty())
                        else {
                            partial = true;
                            continue;
                        };
                        match parse(scope, &account["status"], &account["summary"]) {
                            Ok(mut next) => {
                                next.quotas.retain(|quota| {
                                    !quota["accountId"]
                                        .as_str()
                                        .is_some_and(|id| reported.contains(id))
                                });
                                if next
                                    .account
                                    .as_ref()
                                    .and_then(|row| row["id"].as_str())
                                    .is_some_and(|id| reported.contains(id))
                                {
                                    next.account = None;
                                }
                                merge(&mut reading, next);
                            }
                            Err(_) => partial = true,
                        }
                    }
                } else if reading.account.is_none() {
                    return Err(l10n::text(
                        "Antigravity 本地服务返回的账户格式无法识别",
                        "Antigravity local service returned an unsupported account format.",
                    ));
                } else {
                    partial = true;
                }
            }
            Err(error) if reading.account.is_none() => return Err(error),
            Err(_) => partial = true,
        }
    }
    if reading.quotas.is_empty() && reading.account.is_none() {
        if let Some(executable) = executable {
            return super::cli::read(host, Path::new(&executable));
        }
        return Err(l10n::text(
            "没有已登录且正在运行的 Antigravity 服务",
            "No authenticated already-running Antigravity service is available",
        ));
    }
    reading.notice = if partial {
        Some(l10n::text(
            "部分 Antigravity 账户额度无法读取",
            "Some Antigravity accounts could not be read.",
        ))
    } else if reading.quotas.is_empty() {
        Some(l10n::text(
            "Antigravity 已连接，但未提供额度比例",
            "Antigravity is connected but reports no quota percentage.",
        ))
    } else {
        None
    };
    Ok(reading)
}

fn merge(reading: &mut Reading, next: Reading) {
    for quota in next.quotas {
        if let Some(existing) = reading
            .quotas
            .iter_mut()
            .find(|row| row["id"] == quota["id"])
        {
            let resets = (timestamp(existing, "resetAt"), timestamp(&quota, "resetAt"));
            let observations = (
                timestamp(existing, "observedAt"),
                timestamp(&quota, "observedAt"),
            );
            let replace = match (resets, observations) {
                ((Some(old), Some(new)), _) if old != new => new > old,
                (_, (Some(old), Some(new))) if old != new => new > old,
                ((Some(old), Some(new)), _) if old == new => {
                    quota["usedPercent"].as_f64() > existing["usedPercent"].as_f64()
                }
                _ => false,
            };
            if replace {
                *existing = quota;
            }
        } else {
            reading.quotas.push(quota);
        }
    }
    for account in next.account.into_iter().chain(next.additional_accounts) {
        if reading.account.is_none() {
            reading.account = Some(account);
        } else if let Some(existing) = reading
            .account
            .iter_mut()
            .chain(reading.additional_accounts.iter_mut())
            .find(|row| row["id"] == account["id"])
        {
            if timestamp(&account, "checkedAt") > timestamp(existing, "checkedAt") {
                *existing = account;
            }
        } else {
            reading.additional_accounts.push(account);
        }
    }
}

fn timestamp(row: &Value, field: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    row[field]
        .as_str()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;
    use crate::Os;

    fn status_body() -> Value {
        json!({"userStatus": {
            "email": "Me@Example.com",
            "userTier": {"name": "Pro"},
            "cascadeModelConfigData": {"clientModelConfigs": [
                {"label": "Gemini 3 Pro", "modelOrAlias": {"model": "gemini-3-pro"},
                 "quotaInfo": {"remainingFraction": 0.25, "resetTime": "2026-10-23T00:00:00Z"}},
                {"label": "Gemini 3 Flash", "modelOrAlias": {"model": "gemini-3-flash"},
                 "quotaInfo": {"remainingFraction": 0.5}},
                {"label": "Claude Sonnet", "modelOrAlias": {"model": "claude-sonnet"},
                 "quotaInfo": {"remainingFraction": 1.0}},
                {"label": "Gemini Lite", "modelOrAlias": {"model": "gemini-lite"},
                 "quotaInfo": {"remainingFraction": 0.0}},
            ]},
        }})
    }

    fn answering(body: Result<Value, String>) -> FakeHost {
        let mut host = FakeHost::new("/u");
        host.natives.insert(
            ("antigravity".into(), "ReadAccounts".into()),
            body.map(|status| json!({"accounts":[{"source":"/u","status":status}]})),
        );
        host
    }

    #[test]
    fn families_keep_their_most_used_model() {
        let reading = read(&answering(Ok(status_body()))).unwrap();
        let identity = legacy_account("Antigravity", "me@example.com", "").unwrap();
        assert_eq!(reading.quotas.len(), 2);
        let gemini = &reading.quotas[1];
        assert_eq!(gemini["name"], "Gemini Models");
        assert_eq!(gemini["usedPercent"], 75.0);
        assert_eq!(gemini["status"], "warning");
        assert_eq!(gemini["resetAt"], "2026-10-23T00:00:00Z");
        assert_eq!(gemini["accountId"], identity.as_str());
        assert_eq!(gemini["allModels"], false);
        let id = account_id("antigravity", &["me@example.com", ""]);
        assert_eq!(gemini["id"], format!("{id}:legacy:gemini"));
        assert_eq!(reading.quotas[0]["modelFamilies"], json!(["claude", "gpt"]));
        // The Swift app's account for me@example.com, as its quota history keys it.
        let open = "account:53881ee06d7a6f9562ecf52163857631d9ffdf31717da2d0a7b37b430ff6d1e2";
        assert_eq!(identity, open);
        assert_eq!(
            reading.quotas[0]["legacyWindowId"],
            format!("{open}/antigravity:legacy:claude-gpt")
        );
        assert_eq!(
            gemini["legacyWindowId"],
            format!("{open}/antigravity:legacy:gemini")
        );
        let account = reading.account.unwrap();
        assert_eq!(account["plan"], "Pro");
        assert_eq!(account["identity"], "account");
    }

    #[test]
    fn without_an_email_the_account_is_the_home() {
        let mut body = status_body();
        body["userStatus"]["email"] = json!(" ");
        let reading = read(&answering(Ok(body))).unwrap();
        let account = reading.account.unwrap();
        assert_eq!(account["identity"], "local-home");
        assert_eq!(
            account["id"],
            legacy_unresolved("Antigravity", "/u").as_str()
        );
        assert!(reading.quotas[0]["legacyWindowId"].is_null());
    }

    #[test]
    fn windows_reads_the_account_service() {
        let mut host = answering(Ok(status_body()));
        host.os = Os::Windows;
        let reading = read(&host).unwrap();
        assert!(!reading.quotas.is_empty());
    }

    #[test]
    fn a_service_error_or_odd_answer_is_an_error() {
        assert_eq!(
            read(&answering(Err("no service".into()))).err().unwrap(),
            "no service"
        );
        assert!(read(&answering(Ok(json!({"userStatus": {}})))).is_err());
    }

    #[test]
    fn every_account_is_kept_and_the_same_sign_in_is_not_duplicated() {
        let first = status_body();
        let mut second = status_body();
        second["userStatus"]["email"] = json!("work@example.com");
        let mut duplicate = first.clone();
        duplicate["userStatus"]["cascadeModelConfigData"]["clientModelConfigs"][0]["quotaInfo"]
            ["remainingFraction"] = json!(0.1);
        let mut host = FakeHost::new("/u");
        host.natives.insert(
            ("antigravity".into(), "ReadAccounts".into()),
            Ok(json!({"accounts":[
                {"source":"service-1","status":first},
                {"source":"service-2","status":second},
                {"source":"service-3","status":duplicate}
            ]})),
        );
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 4);
        assert_eq!(reading.additional_accounts.len(), 1);
        assert_eq!(reading.additional_accounts[0]["label"], "work@example.com");
        assert_eq!(reading.quotas[1]["usedPercent"], 90.0);
        assert!(reading.notice.is_none());
    }

    #[test]
    fn anonymous_services_stay_separate_and_partial_failure_keeps_good_quota() {
        let mut anonymous = status_body();
        anonymous["userStatus"]["email"] = Value::Null;
        let mut host = FakeHost::new("/u");
        host.natives.insert(
            ("antigravity".into(), "ReadAccounts".into()),
            Ok(json!({"accounts":[
                {"source":"service-1","status":anonymous},
                {"source":"service-2","status":anonymous},
                {"source":"service-3","status":{}}
            ]})),
        );
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 4);
        assert_eq!(reading.additional_accounts.len(), 1);
        assert_ne!(
            reading.quotas[0]["accountId"],
            reading.quotas[2]["accountId"]
        );
        assert_eq!(
            reading.notice.as_deref(),
            Some("Some Antigravity accounts could not be read.")
        );
    }

    #[test]
    fn a_readable_native_account_does_not_launch_the_cli() {
        let mut host = FakeHost::new("/u");
        host.natives.insert(
            ("antigravity".into(), "ReadAccounts".into()),
            Ok(json!({
                "accounts":[{"source":"service-1","status":status_body()}],
                "cliExecutable":"/installed/agy"
            })),
        );
        assert!(!read(&host).unwrap().quotas.is_empty());
        assert!(host.started.borrow().is_empty());
    }

    #[test]
    fn a_tokenless_running_cli_is_read_when_no_native_quota_is_available() {
        let mut host = FakeHost::new("/u");
        host.natives.insert(
            ("antigravity".into(), "ReadAccounts".into()),
            Ok(json!({"accounts":[],"cliExecutable":"/installed/agy"})),
        );
        host.engine_output.insert("-p".into(), vec![json!({
            "status":"SUCCESS","command":{"name":"usage","data":{"groups":[
                {"name":"Gemini Models","buckets":[{"id":"gemini-5h","window":"5h","remaining_fraction":0.25}]}
            ]}}
        })]);
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas[0]["usedPercent"], 75.0);
        assert_eq!(reading.account.unwrap()["label"], "Antigravity CLI");
        assert_eq!(host.started.borrow().len(), 1);
    }

    #[test]
    fn duplicate_services_prefer_the_new_cycle_over_exhausted_old_quota() {
        let mut old = status_body();
        old["userStatus"]["cascadeModelConfigData"]["clientModelConfigs"][0]["quotaInfo"] = json!({
            "remainingFraction":0.05,"resetTime":"2026-10-22T00:00:00Z"
        });
        let mut new = status_body();
        new["userStatus"]["cascadeModelConfigData"]["clientModelConfigs"]
            .as_array_mut()
            .unwrap()
            .truncate(1);
        new["userStatus"]["cascadeModelConfigData"]["clientModelConfigs"][0]["quotaInfo"] = json!({
            "remainingFraction":0.95,"resetTime":"2026-10-23T00:00:00Z"
        });
        for statuses in [[old.clone(), new.clone()], [new, old]] {
            let mut reading = Reading::empty("antigravity");
            for body in statuses {
                merge(&mut reading, parse("/u", &body, &Value::Null).unwrap());
            }
            let gemini = reading
                .quotas
                .iter()
                .find(|row| row["name"] == "Gemini Models")
                .unwrap();
            assert!((gemini["usedPercent"].as_f64().unwrap() - 5.0).abs() < 0.001);
            assert_eq!(gemini["resetAt"], "2026-10-23T00:00:00Z");
        }
    }

    #[test]
    fn native_quota_summary_supplies_exact_windows_with_account_identity() {
        let mut host = FakeHost::new("/u");
        host.natives.insert(
            ("antigravity".into(), "ReadAccounts".into()),
            Ok(json!({
                "accounts":[{
                    "source":"service-1",
                    "status":status_body(),
                    "summary":{
                        "response":{
                            "groups":[
                                {
                                    "displayName":"Gemini Models",
                                    "buckets":[
                                        {"bucketId":"gemini-weekly","displayName":"Weekly Limit Remaining","window":"weekly","remainingFraction":0.75,"resetTime":"2026-11-02T00:00:00Z"},
                                        {"bucketId":"gemini-5h","displayName":"Five Hour Limit Remaining","window":"5h","remainingFraction":1.0,"resetTime":"2026-10-26T12:00:00Z"}
                                    ]
                                },
                                {
                                    "displayName":"Claude and GPT models",
                                    "buckets":[
                                        {"bucketId":"3p-weekly","displayName":"Weekly Limit Remaining","window":"weekly","remainingFraction":0.2,"resetTime":"2026-10-26T15:00:00Z"},
                                        {"bucketId":"3p-5h","displayName":"Five Hour Limit Remaining","window":"5h","remainingFraction":0.5,"resetTime":"2026-10-26T12:00:00Z"}
                                    ]
                                }
                            ]
                        }
                    }
                }]
            })),
        );
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 4);
        assert_eq!(reading.quotas[0]["shortLabel"], "Gemini 7d");
        assert_eq!(reading.quotas[0]["windowDurationSeconds"], 604800);
        assert_eq!(reading.quotas[0]["usedPercent"], 25.0);
        assert_eq!(reading.quotas[1]["shortLabel"], "Gemini 5h");
        assert_eq!(reading.quotas[1]["windowDurationSeconds"], 18000);
        assert_eq!(reading.quotas[2]["shortLabel"], "3rd-party 7d");
        assert!((reading.quotas[2]["usedPercent"].as_f64().unwrap() - 80.0).abs() < 0.001);
        assert_eq!(reading.quotas[2]["modelFamilies"], json!(["claude", "gpt"]));
        assert_eq!(reading.quotas[3]["shortLabel"], "3rd-party 5h");
        assert_eq!(reading.quotas[3]["account"], "Me@Example.com");
    }

    fn report(email: &str, observed_at: &str, remaining: f64, reset: &str) -> Value {
        super::super::statusline::sanitize(
            &json!({"email":email,"plan_tier":"Pro",
                "quota":{"gemini-weekly":{"remaining_fraction":remaining,"reset_time":reset}}
            }),
            observed_at,
        )
        .unwrap()
    }

    #[test]
    fn cli_reports_track_multiple_accounts_without_a_running_service() {
        let mut host = FakeHost::new("/u");
        host.os = Os::Windows;
        host.account_reports.insert(
            "antigravity".into(),
            vec![
                report(
                    "Me@Example.com",
                    "2026-10-23T08:00:00Z",
                    0.75,
                    "2026-10-30T00:00:00Z",
                ),
                report(
                    "work@example.com",
                    "2026-10-23T08:00:00Z",
                    0.5,
                    "2026-10-30T00:00:00Z",
                ),
            ],
        );
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 2);
        assert_eq!(reading.additional_accounts.len(), 1);
        assert_ne!(
            reading.quotas[0]["accountId"],
            reading.quotas[1]["accountId"]
        );
        assert_eq!(reading.quotas[0]["observedAt"], "2026-10-23T08:00:00+00:00");
        assert!(host.started.borrow().is_empty());
    }

    #[test]
    fn cli_summary_supersedes_native_same_account_and_keeps_other_accounts() {
        let mut other = status_body();
        other["userStatus"]["email"] = json!("other@example.com");
        let mut host = FakeHost::new("/u");
        host.natives.insert(("antigravity".into(), "ReadAccounts".into()), Ok(json!({
            "accounts":[{"source":"ide-1","status":status_body()},{"source":"ide-2","status":other}],
            "cliExecutable":"/installed/agy"
        })));
        host.account_reports.insert(
            "antigravity".into(),
            vec![report(
                "me@example.com",
                "2026-10-23T08:00:00Z",
                0.75,
                "2026-10-30T00:00:00Z",
            )],
        );
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 3);
        assert_eq!(reading.additional_accounts.len(), 1);
        let account = legacy_account("Antigravity", "me@example.com", "").unwrap();
        let cli_rows: Vec<_> = reading
            .quotas
            .iter()
            .filter(|row| row["accountId"] == account)
            .collect();
        assert_eq!(cli_rows.len(), 1);
        assert_eq!(cli_rows[0]["windowDurationSeconds"], 604800);
        assert_eq!(
            reading.account.as_ref().unwrap()["checkedAt"],
            "2026-10-23T08:00:00+00:00"
        );
        assert!(host.started.borrow().is_empty());
    }

    #[test]
    fn unknown_cli_quota_preserves_identity_and_readable_native_windows() {
        let mut host = answering(Ok(status_body()));
        host.account_reports.insert(
            "antigravity".into(),
            vec![json!({
                "email":"me@example.com","observedAt":"2026-10-23T08:00:00Z","quota":{}
            })],
        );
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 2);
        assert!(reading.additional_accounts.is_empty());
    }

    #[test]
    fn a_native_failure_does_not_discard_identified_cli_accounts() {
        let mut host = answering(Err("unavailable".into()));
        host.account_reports.insert(
            "antigravity".into(),
            vec![report(
                "me@example.com",
                "2026-10-23T08:00:00Z",
                0.75,
                "2026-10-30T00:00:00Z",
            )],
        );
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 1);
        assert_eq!(
            reading.notice.as_deref(),
            Some("Some Antigravity accounts could not be read.")
        );
    }

    #[test]
    fn duplicate_cli_reports_use_newer_cycle_then_newer_sample_in_any_order() {
        let old = report(
            "me@example.com",
            "2026-10-23T08:00:00Z",
            0.05,
            "2026-10-23T09:00:00Z",
        );
        let new = report(
            "ME@example.com",
            "2026-10-23T09:01:00Z",
            0.95,
            "2026-10-30T09:00:00Z",
        );
        for reports in [[old.clone(), new.clone()], [new.clone(), old]] {
            let mut reading = Reading::empty("antigravity");
            for report in reports {
                merge(
                    &mut reading,
                    super::super::statusline::parse(&report).unwrap(),
                );
            }
            assert_eq!(reading.quotas.len(), 1);
            assert!((reading.quotas[0]["usedPercent"].as_f64().unwrap() - 5.0).abs() < 0.001);
            assert_eq!(reading.quotas[0]["resetAt"], "2026-10-30T09:00:00+00:00");
            assert!(reading.additional_accounts.is_empty());
        }
        let newer = report(
            "me@example.com",
            "2026-10-23T09:02:00Z",
            0.97,
            "2026-10-30T09:00:00Z",
        );
        for reports in [[new.clone(), newer.clone()], [newer, new]] {
            let mut reading = Reading::empty("antigravity");
            for report in reports {
                merge(
                    &mut reading,
                    super::super::statusline::parse(&report).unwrap(),
                );
            }
            assert!((reading.quotas[0]["usedPercent"].as_f64().unwrap() - 3.0).abs() < 0.001);
            assert_eq!(reading.quotas[0]["observedAt"], "2026-10-23T09:02:00+00:00");
            assert_eq!(
                reading.account.unwrap()["checkedAt"],
                "2026-10-23T09:02:00+00:00"
            );
        }
    }
}
