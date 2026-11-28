//! The running agy's read-only slash command, as recorded by the native app's
//! AntigravityCLITests, and isolated home `/usage` polling.
use super::buckets::{build_quota_row, finalize_quotas};
use crate::account::{account_id, legacy_account, legacy_unresolved, EngineStart, Host, Reading};
use crate::l10n::tr;
use chrono::Utc;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};

fn failure() -> String {
    tr!("agy is running, but its quota could not be read")
}

pub(super) fn read(host: &dyn Host, executable: &Path) -> Result<Reading, String> {
    let mut process = host.start_engine(&EngineStart {
        executable,
        args: &["-p", "/usage", "--output-format", "json"],
        folder: "antigravity",
        env: &[("AGY_CLI_DISABLE_AUTO_UPDATE", "true".into())],
    })?;
    let body = process
        .joined(Instant::now() + Duration::from_secs(30))
        .ok_or_else(failure)?;
    if !process.ended() {
        return Err(failure());
    }
    parse(
        &host
            .home()
            .join(".gemini/antigravity-cli")
            .to_string_lossy(),
        &body,
    )
}

pub(super) fn parse(scope: &str, body: &Value) -> Result<Reading, String> {
    parse_usage(scope, None, None, body)
}

pub(super) fn parse_usage(
    scope: &str,
    email: Option<&str>,
    consumer_home: Option<&str>,
    body: &Value,
) -> Result<Reading, String> {
    if body["status"] == "ERROR" {
        let reason = body["error"].as_str().map(str::trim).unwrap_or("");
        return Err(if reason.is_empty() {
            failure()
        } else {
            tr!(
                "Antigravity quota could not be read: {{reason}}",
                reason = reason
            )
        });
    }
    if body["status"] != "SUCCESS" || body["command"]["name"] != "usage" {
        return Err(failure());
    }
    let data = &body["command"]["data"];
    let groups = data["groups"].as_array().ok_or_else(failure)?;
    let trimmed_email = email.map(str::trim).filter(|value| !value.is_empty());
    let lower_email = trimmed_email.map(str::to_lowercase);
    let (account, id, label, identity) = if let Some(ref lower) = lower_email {
        let account = legacy_account("Antigravity", lower, "")
            .unwrap_or_else(|| legacy_unresolved("Antigravity", scope));
        let id = account_id("antigravity", &[lower, ""]);
        (
            account,
            id,
            trimmed_email.unwrap_or("Antigravity CLI"),
            "account",
        )
    } else {
        (
            legacy_unresolved("Antigravity", scope),
            account_id("antigravity", &[scope]),
            "Antigravity CLI",
            "local-home",
        )
    };
    let plan = data["plan_tier"]
        .as_str()
        .or_else(|| data["planTier"].as_str())
        .or_else(|| data["tier"].as_str())
        .or_else(|| data["plan"].as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let now = Utc::now();
    let mut reading = Reading::empty("antigravity");
    let mut account_row = json!({
        "id": account,
        "provider": "antigravity",
        "label": label,
        "identity": identity,
        "checkedAt": now.to_rfc3339(),
    });
    if let Some(plan) = plan {
        account_row["plan"] = json!(plan);
    }
    if let Some(home) = consumer_home {
        account_row["home"] = json!(home);
    }
    reading.account = Some(account_row);
    let mut ids = BTreeSet::new();
    let mut families = BTreeSet::new();
    for group in groups {
        let buckets = group["buckets"].as_array().ok_or_else(failure)?;
        let group_name = group["name"].as_str();
        for bucket in buckets {
            let key = bucket["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(failure)?;
            if !ids.insert(key) {
                return Err(failure());
            }
            let Some(fraction) = bucket["remaining_fraction"]
                .as_f64()
                .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            else {
                continue;
            };
            let bucket_name = Some(bucket["name"].as_str().unwrap_or(key));
            let (family, mut quota) = build_quota_row(
                &id,
                key,
                &account,
                Some(label),
                group_name,
                bucket_name,
                bucket["window"].as_str(),
                fraction,
                bucket["reset_time"].as_str(),
                now,
            );
            if let Some(home) = consumer_home {
                quota["consumerHome"] = json!(home);
            }
            families.insert(family);
            reading.quotas.push(quota);
        }
    }
    finalize_quotas(&mut reading.quotas, families.len());
    if reading.quotas.is_empty() {
        reading.notice = Some(tr!("agy reports no quota percentage."));
    }
    Ok(reading)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    fn usage() -> Value {
        json!({"status":"SUCCESS","num_turns":0,"command":{"name":"usage","data":{"groups":[
            {"name":"Gemini Models","buckets":[
                {"id":"gemini-weekly","name":"Weekly Limit Remaining","window":"weekly","remaining_fraction":0.75,"reset_time":"2026-10-23T00:00:00Z"},
                {"id":"gemini-5h","name":"Five Hour Limit Remaining","window":"5h","remaining_fraction":1.0}]},
            {"name":"Claude and GPT models","buckets":[
                {"id":"3p-weekly","name":"Weekly Limit Remaining","window":"weekly","remaining_fraction":0.5},
                {"id":"3p-5h","name":"Five Hour Limit Remaining","window":"5h","remaining_fraction":0.0}]}]}}})
    }

    #[test]
    fn printed_usage_preserves_groups_windows_and_unknown_identity() {
        let reading = parse("/u/.gemini/antigravity-cli", &usage()).unwrap();
        assert_eq!(reading.quotas.len(), 4);
        assert_eq!(reading.quotas[0]["usedPercent"], 25.0);
        assert_eq!(reading.quotas[0]["shortLabel"], "Gemini 7d");
        assert_eq!(reading.quotas[0]["resetAt"], "2026-10-23T00:00:00Z");
        assert_eq!(reading.quotas[1]["windowDurationSeconds"], 18000);
        assert_eq!(reading.quotas[2]["shortLabel"], "3rd-party 7d");
        assert_eq!(reading.quotas[0]["windowKey"], "antigravity:gemini-weekly");
        assert_eq!(reading.quotas[3]["windowKey"], "antigravity:3p-5h");
        assert_eq!(reading.quotas[3]["usedPercent"], 100.0);
        assert_eq!(reading.account.unwrap()["identity"], "local-home");
    }

    #[test]
    fn one_group_uses_period_names_and_duplicate_buckets_are_rejected() {
        let mut body = usage();
        body["command"]["data"]["groups"]
            .as_array_mut()
            .unwrap()
            .truncate(1);
        let reading = parse("/u", &body).unwrap();
        assert_eq!(reading.quotas[0]["shortLabel"], "Weekly");
        assert_eq!(reading.quotas[0]["allModels"], true);
        body["command"]["data"]["groups"][0]["buckets"][1]["id"] = json!("gemini-weekly");
        assert!(parse("/u", &body).is_err());
    }

    #[test]
    fn reads_only_usage_from_the_running_executable() {
        let mut host = FakeHost::new("/u");
        host.engine_output.insert("-p".into(), vec![usage()]);
        let result = read(&host, Path::new("/installed/agy")).unwrap();
        assert_eq!(result.quotas.len(), 4);
        let started = host.started.borrow();
        assert_eq!(started[0].0, Path::new("/installed/agy"));
        assert_eq!(started[0].1, ["-p", "/usage", "--output-format", "json"]);
        assert_eq!(
            started[0].3,
            [("AGY_CLI_DISABLE_AUTO_UPDATE".into(), "true".into())]
        );
        assert!(host.engine_sent.borrow().is_empty());
    }

    #[test]
    fn failed_run_or_other_command_is_not_quota() {
        let mut body = usage();
        body["status"] = json!("ERROR");
        assert!(parse("/u", &body).is_err());
        body["status"] = json!("SUCCESS");
        body["command"]["name"] = json!("credits");
        assert!(parse("/u", &body).is_err());
    }

    #[test]
    fn a_refusal_keeps_the_clis_own_reason() {
        let refused = json!({"status": "ERROR", "error": " Not eligible in this region. "});
        let why = parse("/u", &refused).err().unwrap();
        assert!(why.ends_with("Not eligible in this region."), "{why}");
        assert_eq!(
            parse("/u", &json!({"status": "ERROR"})).err(),
            Some(failure())
        );
    }

    #[test]
    fn incomplete_cli_output_is_not_a_successful_reading() {
        let mut host = FakeHost::new("/u");
        host.engine_output.insert("-p".into(), vec![usage()]);
        host.engine_ends = false;
        assert!(read(&host, Path::new("/installed/agy")).is_err());
    }
}
