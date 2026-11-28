//! Shared Antigravity quota bucket classification, window duration resolution,
//! and untouched rolling-reset filtering across CLI `/usage`, status-line
//! callbacks, and local-service `RetrieveUserQuotaSummary`.
use crate::account::status;
use crate::naming::{vendor_windows, window_names::Period};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

pub(super) const FIVE_HOURS_SECONDS: i64 = 18_000;
pub(super) const WEEKLY_SECONDS: i64 = 604_800;
const ROLLING_RESET_TOLERANCE_SECONDS: i64 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BucketSpec {
    /// Logical family key used to tell whether a reading covers one family or
    /// several (`"carbon"`, `"argon"`, `"gemini"`, `"3p"`, or raw key/group).
    pub family_key: String,
    /// Display group name when no outer group title was supplied.
    pub group_label: String,
    /// Compact prefix word (`"Carbon"`, `"Argon"`, `"Gemini"`, `"3rd-party"`).
    pub short_word: String,
    /// Window length in seconds (`18000` for 5h, `604800` for weekly).
    pub duration_seconds: Option<i64>,
    /// Model families this bucket meters (`["gemini"]` or `["claude", "gpt"]`).
    pub model_families: Option<Value>,
}

/// Classifies an Antigravity quota bucket from its `key` (e.g.
/// `gemini-carbon-5h`, `gemini-argon-weekly`, `gemini-5h`, `3p-weekly`),
/// optional outer `group_name`, and optional `window` tag (`"5h"`, `"weekly"`).
pub(super) fn classify(key: &str, group_name: Option<&str>, window: Option<&str>) -> BucketSpec {
    let lower_key = key.trim().to_ascii_lowercase();
    let group_raw = group_name.map(str::trim).unwrap_or("");
    let lower_group = group_raw.to_ascii_lowercase();

    let duration_seconds = match window.map(str::trim) {
        Some("5h") => Some(FIVE_HOURS_SECONDS),
        Some("weekly") => Some(WEEKLY_SECONDS),
        _ if lower_key.ends_with("-5h") || lower_key.ends_with("_5h") || lower_key == "5h" => {
            Some(FIVE_HOURS_SECONDS)
        }
        _ if lower_key.ends_with("-weekly")
            || lower_key.ends_with("_weekly")
            || lower_key == "weekly" =>
        {
            Some(WEEKLY_SECONDS)
        }
        _ => None,
    };

    if lower_key.starts_with("gemini-carbon") || lower_group.contains("carbon") {
        BucketSpec {
            family_key: "carbon".into(),
            group_label: "Carbon".into(),
            short_word: "Carbon".into(),
            duration_seconds,
            model_families: Some(json!(["gemini"])),
        }
    } else if lower_key.starts_with("gemini-argon") || lower_group.contains("argon") {
        BucketSpec {
            family_key: "argon".into(),
            group_label: "Argon".into(),
            short_word: "Argon".into(),
            duration_seconds,
            model_families: Some(json!(["gemini"])),
        }
    } else if lower_key.starts_with("gemini") || lower_group.contains("gemini") {
        let short_word = if group_raw.is_empty() {
            "Gemini".into()
        } else {
            vendor_windows::antigravity_group_word(group_raw)
        };
        BucketSpec {
            family_key: "gemini".into(),
            group_label: "Gemini".into(),
            short_word,
            duration_seconds,
            model_families: Some(json!(["gemini"])),
        }
    } else if lower_key.starts_with("3p-")
        || lower_key == "3p"
        || lower_group.contains("claude")
        || lower_group.contains("gpt")
        || lower_group.contains("3rd")
    {
        BucketSpec {
            family_key: "3p".into(),
            group_label: "3rd-party".into(),
            short_word: "3rd-party".into(),
            duration_seconds,
            model_families: Some(json!(["claude", "gpt"])),
        }
    } else if !group_raw.is_empty() {
        let short_word = vendor_windows::antigravity_group_word(group_raw);
        BucketSpec {
            family_key: format!("group:{lower_group}"),
            group_label: group_raw.into(),
            short_word,
            duration_seconds,
            model_families: None,
        }
    } else {
        BucketSpec {
            family_key: format!("raw:{lower_key}"),
            group_label: key.into(),
            short_word: key.into(),
            duration_seconds,
            model_families: None,
        }
    }
}

/// Filters out epoch sentinel timestamps (`1970-01-01T00:00:00Z`) and the
/// synthetic rolling `now + duration` reset timestamp reported by Antigravity
/// for untouched (`remaining_fraction == 1.0`) buckets.
pub(super) fn effective_reset_at(
    raw_reset: Option<&str>,
    remaining_fraction: f64,
    duration_seconds: Option<i64>,
    now: DateTime<Utc>,
) -> Option<String> {
    let raw = raw_reset?;
    let parsed = DateTime::parse_from_rfc3339(raw).ok()?;
    let parsed_utc = parsed.with_timezone(&Utc);
    if parsed_utc.timestamp() <= 0 {
        return None;
    }
    if (remaining_fraction - 1.0).abs() <= f64::EPSILON {
        if parsed_utc <= now {
            return None;
        }
        if let Some(duration) = duration_seconds {
            let expected = now + chrono::Duration::seconds(duration);
            let drift = (parsed_utc - expected).num_seconds().abs();
            if drift <= ROLLING_RESET_TOLERANCE_SECONDS {
                return None;
            }
        }
    }
    Some(raw.to_owned())
}

/// Builds a normalized quota JSON row for a bucket.
pub(super) fn build_quota_row(
    id_prefix: &str,
    key: &str,
    account_id: &str,
    account_label: Option<&str>,
    group_name: Option<&str>,
    bucket_name: Option<&str>,
    window: Option<&str>,
    remaining_fraction: f64,
    raw_reset: Option<&str>,
    now: DateTime<Utc>,
) -> (String, Value) {
    let spec = classify(key, group_name, window);
    let used = (1.0 - remaining_fraction) * 100.0;
    let period = Period::from_seconds(spec.duration_seconds.map(|seconds| seconds as f64));
    let name = match (
        group_name.map(str::trim).filter(|s| !s.is_empty()),
        bucket_name.map(str::trim).filter(|s| !s.is_empty()),
    ) {
        (Some(group), Some(bucket)) => format!("{group} · {bucket}"),
        (None, Some(bucket)) => bucket.to_owned(),
        _ => period.as_ref().map_or_else(
            || spec.group_label.clone(),
            |period| format!("{} · {}", spec.group_label, period.short_name()),
        ),
    };
    let short = period.as_ref().map_or_else(
        || spec.short_word.clone(),
        |period| format!("{} {}", spec.short_word, period.after_word()),
    );
    let reset = effective_reset_at(raw_reset, remaining_fraction, spec.duration_seconds, now);
    let mut quota = json!({
        "id": format!("{id_prefix}:{key}"),
        "windowKey": format!("antigravity:{key}"),
        "vendor": "antigravity",
        "accountId": account_id,
        "account": account_label,
        "name": name,
        "shortLabel": short,
        "usedPercent": used,
        "status": status(used),
        "resetAt": reset,
        "windowDurationSeconds": spec.duration_seconds,
    });
    if let Some(families) = spec.model_families {
        quota["modelFamilies"] = families;
    }
    (spec.family_key, quota)
}

/// Finalizes `allModels`, `consumerSource`, and single-family `shortLabel`s.
pub(super) fn finalize_quotas(quotas: &mut [Value], distinct_families: usize) {
    for quota in quotas {
        let all = distinct_families == 1 && !quota["modelFamilies"].is_null();
        quota["allModels"] = json!(all);
        quota["consumerSource"] = json!("antigravity");
        if all {
            if let Some(period) = Period::from_seconds(quota["windowDurationSeconds"].as_f64()) {
                quota["shortLabel"] = json!(period.short_name());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_carbon_argon_gemini_and_third_party_prefixes_and_suffixes() {
        let carbon = classify("gemini-carbon-5h", Some("Gemini Models"), None);
        assert_eq!(carbon.short_word, "Carbon");
        assert_eq!(carbon.duration_seconds, Some(18_000));
        assert_eq!(carbon.model_families, Some(json!(["gemini"])));

        let argon = classify("gemini-argon-weekly", None, None);
        assert_eq!(argon.short_word, "Argon");
        assert_eq!(argon.duration_seconds, Some(604_800));
        assert_eq!(argon.model_families, Some(json!(["gemini"])));

        let third = classify("3p-5h", None, None);
        assert_eq!(third.short_word, "3rd-party");
        assert_eq!(third.duration_seconds, Some(18_000));
        assert_eq!(third.model_families, Some(json!(["claude", "gpt"])));
    }

    #[test]
    fn suppresses_fake_rolling_reset_on_untouched_buckets_only() {
        let now: DateTime<Utc> = "2026-11-27T10:00:00Z".parse().unwrap();
        assert_eq!(
            effective_reset_at(Some("2026-11-27T15:00:25Z"), 1.0, Some(18_000), now),
            None
        );
        assert_eq!(
            effective_reset_at(Some("2026-12-04T09:59:15Z"), 1.0, Some(604_800), now),
            None
        );
        // Epoch sentinel ("1970-01-01T00:00:00Z") reported by agy /usage is suppressed.
        assert_eq!(
            effective_reset_at(Some("1970-01-01T00:00:00Z"), 1.0, Some(604_800), now),
            None
        );
        // Partially used bucket keeps its real reset timestamp.
        assert_eq!(
            effective_reset_at(Some("2026-11-27T15:00:25Z"), 0.99, Some(18_000), now),
            Some("2026-11-27T15:00:25Z".into())
        );
        // Reset earlier than a full cycle on a 1.0 bucket (e.g. fixed reset) stays.
        assert_eq!(
            effective_reset_at(Some("2026-11-27T12:00:00Z"), 1.0, Some(18_000), now),
            Some("2026-11-27T12:00:00Z".into())
        );
    }
}
