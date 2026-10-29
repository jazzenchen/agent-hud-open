//! Grok Bot's weekly quota from Cursor's servers: `GetSandUsageStatus`, the
//! request Grok Bot sends for its own usage meter, with Cursor's sign-in.
//! The Bot's cache of the same reading (`account.rs`) changes only while the
//! Bot is used on this Mac; the servers know it whenever the bots run.
//!
//! The reply also says whether the account was merged into SuperGrok
//! (`isUnified`) and, if so, its plan (`cursorPlanName`), which is the name
//! Grok Bot shows for a merged account.
use crate::account::{Host, HttpFailure};
use crate::cursor::account::{dashboard, sign_in, SignIn};
use crate::l10n;
use serde_json::{json, Map, Value};

/// Grok Bot's value for a spend limit without a ceiling.
const NO_CEILING_CENTS: f64 = 2_147_483_647.0;

fn ms(value: &Value) -> Option<i64> {
    let text = value.as_str()?;
    let at = chrono::DateTime::parse_from_rfc3339(text).ok()?;
    Some(at.timestamp_millis())
}

/// The weekly usage of the Bot signed in as `slot`, shaped as the `usage`
/// the Bot keeps in its cache, so that both are read alike. None when
/// Cursor is not signed in to the same account as the Bot, or the servers
/// keep no personal weekly usage for it (a team's pooled allowance).
pub(super) fn read(host: &dyn Host, slot: &str, now_ms: i64) -> Result<Option<Value>, String> {
    let Some(SignIn { user, token, .. }) = sign_in(host)? else {
        return Ok(None);
    };
    // The Bot's slot is "<provider>|<user>", and Cursor's sign-in names the
    // same user when both are signed in to one account.
    if slot.rsplit('|').next() != Some(user.as_str()) {
        return Ok(None);
    }
    let status =
        dashboard(host, &token, "GetSandUsageStatus").map_err(|failure| match failure {
            HttpFailure::Unreachable => l10n::text(
                "无法读取 Grok Bot 的周额度，请检查网络",
                "Grok Bot's weekly usage could not be reached; check the connection.",
            ),
            other => other.message(),
        })?;
    if !status.is_object() {
        return Err(l10n::text(
            "Grok Bot 的周额度格式无法识别",
            "Grok Bot's weekly usage has an unsupported format.",
        ));
    }
    // The Bot asks for the period's spend alongside and does without it.
    let period = dashboard(host, &token, "GetCurrentPeriodUsage").ok();
    Ok(usage(&status, period.as_ref(), now_ms))
}

/// The Bot's reading of `GetSandUsageStatus` and `GetCurrentPeriodUsage`.
/// The servers leave out fields at their defaults (false, 0, "").
fn usage(status: &Value, period: Option<&Value>, now_ms: i64) -> Option<Value> {
    if status["usesPooledEnterpriseAllowance"] == true {
        return None;
    }
    let percent = status["usagePercent"].as_f64().filter(|n| n.is_finite())?;
    let included = status["hasNonZeroIncludedLimit"] == true;
    let mut usage = Map::new();
    usage.insert("percentUsed".into(), json!(percent.max(0.)));
    usage.insert(
        "nextResetMs".into(),
        json!(ms(&status["nextResetTimestampUtc"]).filter(|at| *at > 0)),
    );
    usage.insert(
        "isSandTrial".into(),
        json!(ms(&status["sandTrialExpiresAt"]).is_some_and(|at| at > now_ms)),
    );
    usage.insert("hasNonZeroIncludedLimit".into(), json!(included));
    usage.insert("isTeamSeat".into(), json!(status["isTeamSeat"] == true));
    let spend = period
        .map(|p| &p["spendLimitUsage"])
        .filter(|s| s.is_object());
    let on_demand = spend.filter(|_| included).and_then(|spend| {
        let limit = spend["individualLimit"]
            .as_f64()
            .filter(|n| n.is_finite() && *n > 0. && *n < NO_CEILING_CENTS)?;
        let used = spend["individualUsed"].as_f64().unwrap_or(0.);
        Some(json!({"usedCents": used, "limitCents": limit}))
    });
    usage.insert("onDemand".into(), json!(on_demand));
    if let Some(label) = status["grokPlanLabel"].as_str().filter(|s| !s.is_empty()) {
        usage.insert("grokPlanLabel".into(), json!(label));
    }
    let merged_plan = (status["isUnified"] == true)
        .then(|| status["cursorPlanName"].as_str().map(str::trim))
        .flatten()
        .filter(|s| !s.is_empty());
    if let Some(plan) = merged_plan {
        usage.insert("unifiedPlanName".into(), json!(plan));
    }
    Some(Value::Object(usage))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_793_200_000_000;

    #[test]
    fn a_merged_accounts_reply_reads_as_the_bot_reads_it() {
        let status = json!({
            "usagePercent": 0.37,
            "nextResetTimestampUtc": "2026-10-31T18:20:37.079Z",
            "hasNonZeroIncludedLimit": true,
            "grokPlanLabel": "Grok Bot Plan",
            "isUnified": true,
            "cursorPlanName": " Super ",
        });
        let period = json!({"spendLimitUsage": {"individualLimit": 2000, "individualUsed": 150}});
        let usage = usage(&status, Some(&period), NOW).unwrap();
        assert_eq!(usage["percentUsed"], 0.37);
        assert_eq!(usage["nextResetMs"], 1_793_470_837_079_i64);
        assert_eq!(usage["isSandTrial"], false);
        assert_eq!(usage["isTeamSeat"], false);
        assert_eq!(usage["unifiedPlanName"], "Super");
        assert_eq!(usage["grokPlanLabel"], "Grok Bot Plan");
        assert_eq!(usage["onDemand"]["usedCents"], 150.0);
        assert_eq!(usage["onDemand"]["limitCents"], 2000.0);
    }

    #[test]
    fn an_account_not_merged_has_no_merged_plan_and_no_ceiling_is_no_extra_limit() {
        let status = json!({
            "usagePercent": 12.0,
            "hasNonZeroIncludedLimit": true,
            "cursorPlanName": "Pro",
            "sandTrialExpiresAt": "2026-11-30T00:00:00Z",
        });
        let period = json!({"spendLimitUsage": {"individualLimit": 2147483647}});
        let read = usage(&status, Some(&period), NOW).unwrap();
        assert!(read.get("unifiedPlanName").is_none());
        assert_eq!(read["isSandTrial"], true);
        assert!(read["nextResetMs"].is_null());
        assert!(read["onDemand"].is_null());
        assert!(usage(&status, None, NOW).unwrap()["onDemand"].is_null());
    }

    #[test]
    fn a_pooled_or_unmetered_reply_has_no_personal_usage() {
        assert!(usage(
            &json!({"usesPooledEnterpriseAllowance": true, "usagePercent": 5.0}),
            None,
            NOW
        )
        .is_none());
        assert!(usage(&json!({"hasNonZeroIncludedLimit": true}), None, NOW).is_none());
    }
}
