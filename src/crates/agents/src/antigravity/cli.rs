//! The running agy's read-only slash command, as recorded by the native app's
//! AntigravityCLITests. It reports quota groups but no signed-in identity.
use crate::account::{account_id, legacy_unresolved, status, EngineStart, Host, Reading};
use crate::l10n;
use crate::naming::{vendor_windows, window_names::Period};
use chrono::Utc;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};

fn failure() -> String {
    l10n::text(
        "agy 正在运行，但读取不到它的额度",
        "agy is running, but its quota could not be read",
    )
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

fn parse(scope: &str, body: &Value) -> Result<Reading, String> {
    // The CLI can refuse to report usage at all, for example when the
    // account's region is not eligible for Antigravity. Its own words say
    // why, so keep them in the failure instead of the generic text.
    if body["status"] == "ERROR" {
        let reason = body["error"].as_str().map(str::trim).unwrap_or("");
        return Err(if reason.is_empty() {
            failure()
        } else {
            l10n::text(
                &format!("无法读取 Antigravity 额度：{reason}"),
                &format!("Antigravity quota could not be read: {reason}"),
            )
        });
    }
    if body["status"] != "SUCCESS" || body["command"]["name"] != "usage" {
        return Err(failure());
    }
    let groups = body["command"]["data"]["groups"]
        .as_array()
        .ok_or_else(failure)?;
    let account = legacy_unresolved("Antigravity", scope);
    let id = account_id("antigravity", &[scope]);
    let mut reading = Reading::empty("antigravity");
    reading.account = Some(
        json!({"id":account,"provider":"antigravity","label":"Antigravity CLI",
        "identity":"local-home","checkedAt":Utc::now().to_rfc3339()}),
    );
    let mut ids = BTreeSet::new();
    let mut active_groups = BTreeSet::new();
    for (index, group) in groups.iter().enumerate() {
        let buckets = group["buckets"].as_array().ok_or_else(failure)?;
        for bucket in buckets {
            let key = bucket["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(failure)?;
            if !ids.insert(key) {
                return Err(failure());
            }
            active_groups.insert(index);
            let Some(fraction) = bucket["remaining_fraction"]
                .as_f64()
                .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            else {
                continue;
            };
            let used = (1.0 - fraction) * 100.0;
            let group_name = group["name"].as_str().unwrap_or("");
            let bucket_name = bucket["name"].as_str().unwrap_or(key);
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
            let reset = bucket["reset_time"]
                .as_str()
                .filter(|value| chrono::DateTime::parse_from_rfc3339(value).is_ok());
            let lower = group_name.to_lowercase();
            let mut quota = json!({"id":format!("{id}:{key}"),"vendor":"antigravity","accountId":account,
                "account":"Antigravity CLI","name":name,"shortLabel":short,"usedPercent":used,
                "status":status(used),"resetAt":reset,"windowDurationSeconds":duration});
            if lower.contains("gemini") {
                quota["modelFamilies"] = json!(["gemini"]);
            } else if lower.contains("claude") || lower.contains("gpt") {
                quota["modelFamilies"] = json!(["claude", "gpt"]);
            }
            reading.quotas.push(quota);
        }
    }
    for quota in &mut reading.quotas {
        let all = active_groups.len() == 1;
        quota["allModels"] = json!(all);
        // Its calls are logged by model, which picks the pool.
        quota["consumerSource"] = json!("antigravity");
        if all {
            if let Some(period) = Period::from_seconds(quota["windowDurationSeconds"].as_f64()) {
                quota["shortLabel"] = json!(period.short_name());
            }
        }
    }
    if reading.quotas.is_empty() {
        reading.notice = Some(l10n::text(
            "agy 未提供额度比例",
            "agy reports no quota percentage.",
        ));
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
