//! An Antigravity account's quota windows, from its user status and the
//! model quota summary.
use crate::account::{account_id, legacy_account, legacy_unresolved, status, Reading};
use crate::l10n::tr;
use crate::naming::{
    vendor_windows,
    window_names::{self, Period},
};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

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
        // Its calls are logged by model, which picks the pool.
        quota["consumerSource"] = json!("antigravity");
        if all {
            if let Some(period) = Period::from_seconds(quota["windowDurationSeconds"].as_f64()) {
                quota["shortLabel"] = json!(period.short_name());
            }
        }
    }
    Some(quotas)
}

pub(super) fn parse(scope: &str, body: &Value, summary: &Value) -> Result<Reading, String> {
    let user = &body["userStatus"];
    let configs = user["cascadeModelConfigData"]["clientModelConfigs"]
        .as_array()
        .ok_or_else(|| tr!("Antigravity local service returned an unsupported quota format."))?;
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
                // Its calls are logged by model, which picks the pool.
                quota["consumerSource"] = json!("antigravity");
                quota
            })
            .collect::<Vec<_>>()
    };
    let notice = quotas
        .is_empty()
        .then(|| tr!("Antigravity is connected; its local service reports no quota percentage."));
    Ok(Reading {
        provider: "antigravity",
        failure: None,
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
        unreviewed_hooks: vec![],
    })
}
