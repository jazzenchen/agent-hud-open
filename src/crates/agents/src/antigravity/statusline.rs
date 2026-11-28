//! Official agy status-line reports identify the signed-in account and its
//! quota buckets. Only these account facts cross the callback storage boundary.
use super::buckets::{build_quota_row, finalize_quotas};
use crate::account::{account_id, legacy_account, status, Reading};
use crate::l10n::tr;
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

pub const STALE_AFTER_SECONDS: i64 = 300;

/// Strip conversation, workspace and transcript data from the official
/// status-line stdin. A missing percentage stays unavailable, never zero.
pub fn sanitize(payload: &Value, observed_at: &str) -> Option<Value> {
    let observed_at = DateTime::parse_from_rfc3339(observed_at).ok()?;
    let email = payload["email"].as_str()?.trim().to_lowercase();
    if email.is_empty() || email.chars().any(char::is_control) {
        return None;
    }
    let mut quota = Map::new();
    for (id, bucket) in payload["quota"].as_object()? {
        if id.trim().is_empty() || id.chars().any(char::is_control) {
            continue;
        }
        let Some(fraction) = bucket["remaining_fraction"]
            .as_f64()
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        else {
            continue;
        };
        let reset = bucket["reset_time"]
            .as_str()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok());
        quota.insert(
            id.clone(),
            json!({"remaining_fraction":fraction,"reset_time":reset.map(|value|value.to_rfc3339())}),
        );
    }
    Some(json!({
        "observedAt":observed_at.to_rfc3339(),
        "email":email,
        "plan_tier":payload["plan_tier"].as_str().map(str::trim).filter(|value|!value.is_empty()),
        "quota":quota,
    }))
}

pub(super) fn parse(report: &Value) -> Option<Reading> {
    parse_at(report, Utc::now())
}

fn parse_at(report: &Value, now: DateTime<Utc>) -> Option<Reading> {
    let report = sanitize(report, report["observedAt"].as_str()?)?;
    let email = report["email"].as_str()?;
    let observed_at = report["observedAt"].as_str()?;
    let observed = DateTime::parse_from_rfc3339(observed_at).ok()?;
    let stale = (now - observed.with_timezone(&Utc)).num_seconds() > STALE_AFTER_SECONDS;
    let account = legacy_account("Antigravity", email, "")?;
    let id = account_id("antigravity", &[email, ""]);
    let mut reading = Reading::empty("antigravity");
    reading.account = Some(json!({
        "id":account,"provider":"antigravity","source":"antigravity_cli","label":email,"identity":"account",
        "plan":report["plan_tier"],"checkedAt":observed_at,"stale":stale,
    }));
    let mut groups = BTreeSet::new();
    for (key, bucket) in report["quota"].as_object()? {
        let fraction = bucket["remaining_fraction"].as_f64()?;
        let raw_reset = bucket["reset_time"].as_str();
        let expired = raw_reset
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .is_some_and(|reset| reset <= now);
        let (family, mut quota) = build_quota_row(
            &id,
            key,
            &account,
            Some(email),
            None,
            None,
            None,
            fraction,
            raw_reset,
            now,
        );
        groups.insert(family);
        let used = quota["usedPercent"].as_f64().unwrap_or(0.0);
        quota["source"] = json!("antigravity_cli");
        quota["observedAt"] = json!(observed_at);
        quota["status"] = json!(if stale || expired {
            "stale"
        } else {
            status(used)
        });
        reading.quotas.push(quota);
    }
    finalize_quotas(&mut reading.quotas, groups.len());
    if reading.quotas.is_empty() {
        reading.notice = Some(tr!(
            "Antigravity CLI identified the account but reports no quota percentage."
        ));
    }
    Some(reading)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> Value {
        json!({
            "email":" Me@Example.com ","plan_tier":"Pro","cwd":"/private/project",
            "transcript_path":"/private/conversation","conversation_id":"private-session",
            "quota":{
                "gemini-weekly":{"remaining_fraction":0.9378,"reset_time":"2026-10-30T07:50:32Z","reset_in_seconds":560580},
                "3p-5h":{"remaining_fraction":0.25,"reset_time":"2026-10-23T13:00:00Z"},
            },
        })
    }

    fn now() -> DateTime<Utc> {
        "2026-10-23T08:00:00Z".parse().unwrap()
    }

    #[test]
    fn callback_storage_contains_only_account_facts() {
        let report = sanitize(&payload(), "2026-10-23T08:00:00Z").unwrap();
        let keys: Vec<_> = report
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["email", "observedAt", "plan_tier", "quota"]);
        assert_eq!(report["email"], "me@example.com");
        assert_eq!(
            report["quota"]["gemini-weekly"].as_object().unwrap().len(),
            2
        );
        let encoded = report.to_string();
        assert!(!encoded.contains("private"));
        assert!(!encoded.contains("conversation"));
    }

    #[test]
    fn official_buckets_preserve_identity_plan_windows_and_sample_time() {
        let report = sanitize(&payload(), "2026-10-23T08:00:00Z").unwrap();
        let reading = parse_at(&report, now()).unwrap();
        assert_eq!(
            reading.account.as_ref().unwrap()["id"],
            legacy_account("Antigravity", "me@example.com", "").unwrap()
        );
        assert_eq!(reading.account.as_ref().unwrap()["plan"], "Pro");
        assert_eq!(
            reading.account.as_ref().unwrap()["source"],
            "antigravity_cli"
        );
        assert_eq!(reading.quotas[0]["source"], "antigravity_cli");
        assert_eq!(reading.quotas[0]["shortLabel"], "3rd-party 5h");
        assert_eq!(reading.quotas[0]["usedPercent"], 75.0);
        assert_eq!(reading.quotas[1]["windowDurationSeconds"], 604800);
        assert!((reading.quotas[1]["usedPercent"].as_f64().unwrap() - 6.22).abs() < 0.001);
        assert_eq!(reading.quotas[1]["observedAt"], "2026-10-23T08:00:00+00:00");
        assert_eq!(reading.quotas[0]["windowKey"], "antigravity:3p-5h");
        assert_eq!(reading.quotas[1]["windowKey"], "antigravity:gemini-weekly");
        assert_eq!(reading.quotas[1]["allModels"], false);
    }

    #[test]
    fn carbon_and_argon_statusline_buckets_get_exact_durations_and_filter_rolling_resets() {
        let report = sanitize(
            &json!({
                "email": "pro@example.com",
                "plan_tier": "Google AI Pro",
                "quota": {
                    "gemini-carbon-5h": {"remaining_fraction": 1.0, "reset_time": "2026-10-23T13:00:10Z"},
                    "gemini-argon-weekly": {"remaining_fraction": 0.6, "reset_time": "2026-10-30T00:00:00Z"}
                }
            }),
            "2026-10-23T08:00:00Z",
        )
        .unwrap();
        let reading = parse_at(&report, now()).unwrap();
        assert_eq!(reading.quotas.len(), 2);
        assert_eq!(reading.quotas[0]["shortLabel"], "Argon 7d");
        assert_eq!(reading.quotas[0]["windowDurationSeconds"], 604_800);
        assert_eq!(reading.quotas[1]["shortLabel"], "Carbon 5h");
        assert_eq!(reading.quotas[1]["windowDurationSeconds"], 18_000);
        assert!(reading.quotas[1]["resetAt"].is_null());
    }

    #[test]
    fn unavailable_or_invalid_quota_does_not_become_zero_percent() {
        let report = json!({"email":"ldap","observedAt":"2026-10-23T08:00:00Z","quota":{
            "empty":{},"negative":{"remaining_fraction":-1},"high":{"remaining_fraction":2},
            "text":{"remaining_fraction":"0.5"},"new-bucket":{"remaining_fraction":0.5}
        }});
        let reading = parse_at(&report, now()).unwrap();
        assert_eq!(reading.quotas.len(), 1);
        assert_eq!(reading.quotas[0]["name"], "new-bucket");
        assert_eq!(reading.quotas[0]["usedPercent"], 50.0);
        assert!(reading.quotas[0]["windowDurationSeconds"].is_null());
        assert!(reading.quotas[0]["modelFamilies"].is_null());
        assert_eq!(reading.quotas[0]["allModels"], false);
        assert!(sanitize(&json!({"email":"a"}), "2026-10-23T08:00:00Z").is_none());
        assert!(sanitize(&json!({"email":" ","quota":{}}), "2026-10-23T08:00:00Z").is_none());
        assert!(sanitize(&payload(), "invalid").is_none());
        let empty = parse_at(
            &json!({"email":"a","quota":{},"observedAt":"2026-10-23T08:00:00Z"}),
            now(),
        )
        .unwrap();
        assert!(empty.quotas.is_empty());
        assert!(empty.notice.is_some());
    }

    #[test]
    fn old_samples_and_expired_cycles_are_marked_stale_without_changing_sample_time() {
        let report = sanitize(&payload(), "2026-10-23T07:54:59Z").unwrap();
        let reading = parse_at(&report, now()).unwrap();
        assert_eq!(reading.account.unwrap()["stale"], true);
        assert!(reading
            .quotas
            .iter()
            .all(|quota| quota["status"] == "stale"));
        assert_eq!(reading.quotas[0]["observedAt"], "2026-10-23T07:54:59+00:00");
        let mut payload = payload();
        payload["quota"]["3p-5h"]["reset_time"] = json!("2026-10-23T07:59:00Z");
        let report = sanitize(&payload, "2026-10-23T08:00:00Z").unwrap();
        let reading = parse_at(&report, now()).unwrap();
        assert_eq!(reading.quotas[0]["status"], "stale");
        assert_eq!(reading.quotas[1]["status"], "normal");
    }
}
