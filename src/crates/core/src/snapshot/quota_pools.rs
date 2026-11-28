//! Pooled multi-account quota windows and recommended account selection.
use super::*;
use crate::metering::date_ms;
use std::collections::{BTreeMap, HashMap, HashSet};

impl crate::AgentHudCore {
    /// Aggregates quota windows across accounts when a vendor has `>= 2` distinct
    /// signed-in accounts, computing combined utilization, earliest recovery gain,
    /// and the optimal account to use next.
    pub(super) fn quota_pool_rows(
        &self,
        quotas: &[Value],
        accounts: &[Value],
        now: i64,
    ) -> Vec<Value> {
        build_quota_pools(quotas, accounts, now)
    }
}

pub(crate) fn build_quota_pools(quotas: &[Value], accounts: &[Value], now: i64) -> Vec<Value> {
    let account_by_id: HashMap<&str, &Value> = accounts
        .iter()
        .filter_map(|acct| Some((acct["id"].as_str()?, acct)))
        .collect();

    let mut vendor_accounts: HashMap<&str, HashSet<&str>> = HashMap::new();
    for row in quotas {
        let Some(vendor) = row["vendor"].as_str().filter(|v| !v.is_empty()) else {
            continue;
        };
        let Some(acct_id) = row["accountId"].as_str().filter(|id| !id.is_empty()) else {
            continue;
        };
        if row["usedPercent"].as_f64().is_some_and(f64::is_finite) {
            vendor_accounts.entry(vendor).or_default().insert(acct_id);
        }
    }

    let mut group_order: Vec<(String, String)> = Vec::new();
    let mut groups: BTreeMap<(String, String), Vec<&Value>> = BTreeMap::new();
    for row in quotas {
        let Some(vendor) = row["vendor"].as_str() else {
            continue;
        };
        if vendor_accounts.get(vendor).map_or(0, HashSet::len) < 2 {
            continue;
        }
        if !row["usedPercent"].as_f64().is_some_and(f64::is_finite) {
            continue;
        }
        let Some(acct_id) = row["accountId"].as_str().filter(|id| !id.is_empty()) else {
            continue;
        };
        let key = pool_window_key(row, acct_id);
        let pair = (vendor.to_owned(), key);
        let entry = groups.entry(pair.clone()).or_default();
        if entry
            .iter()
            .any(|existing| existing["accountId"] == row["accountId"])
        {
            continue;
        }
        if entry.is_empty() {
            group_order.push(pair);
        }
        entry.push(row);
    }

    let mut out = Vec::new();
    for pair in group_order {
        let Some(rows) = groups.get(&pair) else {
            continue;
        };
        if rows.len() < 2 {
            continue;
        }
        let (vendor, window_key) = &pair;
        let first = rows[0];
        let recommended_id = pick_recommended(rows, now);

        let mut member_tuples = rows
            .iter()
            .map(|row| {
                let acct_id = row["accountId"].as_str().unwrap_or_default();
                let acct = account_by_id.get(acct_id).copied();
                let label = acct
                    .and_then(|a| a["name"].as_str().or_else(|| a["label"].as_str()))
                    .or_else(|| row["account"].as_str())
                    .unwrap_or(acct_id);
                let plan_label =
                    acct.and_then(|a| a["planLabel"].as_str().or_else(|| a["plan"].as_str()));
                let used = row["usedPercent"].as_f64().unwrap_or(0.0).clamp(0.0, 100.0);
                let reset_str = row["resetAt"].as_str();
                let reset_ms = reset_str.and_then(date_ms).filter(|ms| *ms > now);
                let status = row["status"].as_str().unwrap_or("normal");
                let burn = row["metrics"]["burnRatePctPerHour"]
                    .as_f64()
                    .filter(|v| v.is_finite() && *v >= 0.0);
                (
                    reset_ms.unwrap_or(i64::MAX),
                    used,
                    acct_id.to_owned(),
                    json!({
                        "accountId": acct_id,
                        "quotaId": row["id"],
                        "accountLabel": label,
                        "planLabel": plan_label,
                        "usedPercent": used,
                        "burnRatePctPerHour": burn,
                        "resetAt": reset_str,
                        "status": status,
                        "recommended": recommended_id.as_deref() == Some(acct_id),
                    }),
                    status == "stale",
                )
            })
            .collect::<Vec<_>>();

        member_tuples.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.total_cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });

        let live_members: Vec<_> = member_tuples.iter().filter(|m| !m.4).collect();
        let denom_slice = if live_members.is_empty() {
            member_tuples.iter().collect::<Vec<_>>()
        } else {
            live_members
        };
        let count = denom_slice.len() as f64;
        let mean_used = (denom_slice.iter().map(|m| m.1).sum::<f64>() / count).clamp(0.0, 100.0);

        let earliest_reset_ms = denom_slice
            .iter()
            .filter(|m| m.1 > 0.0 && m.0 < i64::MAX)
            .map(|m| m.0)
            .min();
        let (next_reset_at, next_reset_gain) = match earliest_reset_ms {
            Some(target_ms) => {
                let iso_str = denom_slice
                    .iter()
                    .find(|m| m.0 == target_ms)
                    .and_then(|m| m.3["resetAt"].as_str())
                    .map(str::to_owned);
                let gain = denom_slice
                    .iter()
                    .filter(|m| m.0 < i64::MAX && (m.0 - target_ms).abs() <= 60_000)
                    .map(|m| m.1 / count)
                    .sum::<f64>()
                    .clamp(0.0, 100.0);
                (iso_str, Some(gain))
            }
            None => (None, None),
        };

        let pool_status = if member_tuples.iter().all(|m| m.4) {
            "stale"
        } else if mean_used >= 90.0 {
            "critical"
        } else if mean_used >= 75.0 {
            "warning"
        } else {
            "normal"
        };

        let members: Vec<Value> = member_tuples.into_iter().map(|m| m.3).collect();
        out.push(json!({
            "id": format!("pool:{vendor}:{window_key}"),
            "vendor": vendor,
            "windowKey": window_key,
            "name": first["name"],
            "shortLabel": first["shortLabel"],
            "allModels": first["allModels"],
            "windowDurationSeconds": first["windowDurationSeconds"],
            "usedPercent": mean_used,
            "status": pool_status,
            "nextResetAt": next_reset_at,
            "nextResetGainPercent": next_reset_gain,
            "recommendedAccountId": recommended_id,
            "members": members,
        }));
    }
    out
}

fn pool_window_key(row: &Value, account_id: &str) -> String {
    if let Some(window_key) = row["windowKey"].as_str().filter(|s| !s.is_empty()) {
        return window_key.to_owned();
    }
    if let Some(legacy) = row["legacyWindowId"].as_str() {
        if let Some(rest) = legacy.strip_prefix(&format!("{account_id}/")) {
            if !rest.is_empty() {
                return rest.to_owned();
            }
        }
    }
    row["name"]
        .as_str()
        .unwrap_or("window")
        .to_lowercase()
        .replace(' ', "-")
}

/// Selects the best account to spend next: excludes stale and exhausted (`>= 99.5%`)
/// accounts, prefers comfortable headroom (`< 90%`), then prioritizes an active
/// rolling window resetting soonest (burn-before-reset), else lowest utilization.
fn pick_recommended(rows: &[&Value], now: i64) -> Option<String> {
    let mut candidates = rows
        .iter()
        .filter_map(|row| {
            let id = row["accountId"].as_str()?;
            let status = row["status"].as_str().unwrap_or("normal");
            if status == "stale" {
                return None;
            }
            let used = row["usedPercent"].as_f64()?.clamp(0.0, 100.0);
            if used >= 99.5 {
                return None;
            }
            let tight = u8::from(used >= 90.0);
            let active_reset = row["resetAt"]
                .as_str()
                .and_then(date_ms)
                .filter(|ms| *ms > now && used > 0.0)
                .unwrap_or(i64::MAX);
            Some((tight, active_reset, used, id.to_owned()))
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.total_cmp(&b.2))
            .then_with(|| a.3.cmp(&b.3))
    });
    candidates.into_iter().next().map(|c| c.3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pools_compute_mean_usage_recovery_gain_and_shift_recommendation_when_exhausted() {
        let now = 1_776_000_000_000i64;
        let accounts = vec![
            json!({"id": "acct-1", "provider": "antigravity", "name": "a@example.com", "planLabel": "Google AI Pro"}),
            json!({"id": "acct-2", "provider": "antigravity", "name": "b@example.com", "planLabel": "Google AI Pro"}),
        ];
        let quotas = vec![
            json!({
                "id": "q1",
                "vendor": "antigravity",
                "accountId": "acct-1",
                "windowKey": "antigravity:carbon-5h",
                "name": "Gemini Carbon · 5h",
                "shortLabel": "Carbon 5h",
                "usedPercent": 100.0,
                "resetAt": "2026-04-12T15:00:00Z",
                "status": "critical",
                "windowDurationSeconds": 18000.0,
            }),
            json!({
                "id": "q2",
                "vendor": "antigravity",
                "accountId": "acct-2",
                "windowKey": "antigravity:carbon-5h",
                "name": "Gemini Carbon · 5h",
                "shortLabel": "Carbon 5h",
                "usedPercent": 20.0,
                "resetAt": "2026-04-12T16:00:00Z",
                "status": "normal",
                "windowDurationSeconds": 18000.0,
            }),
        ];
        let pools = build_quota_pools(&quotas, &accounts, now);
        assert_eq!(pools.len(), 1);
        let pool = &pools[0];
        assert_eq!(pool["usedPercent"], 60.0);
        assert_eq!(pool["recommendedAccountId"], "acct-2");
        assert_eq!(pool["nextResetAt"], "2026-04-12T15:00:00Z");
        assert_eq!(pool["nextResetGainPercent"], 50.0);
        assert_eq!(pool["members"][0]["accountId"], "acct-1");
        assert_eq!(pool["members"][1]["recommended"], true);
    }

    #[test]
    fn single_account_vendors_emit_no_pools() {
        let quotas = vec![json!({
            "id": "q1",
            "vendor": "claude",
            "accountId": "acct-1",
            "windowKey": "claude-session",
            "name": "Current session",
            "usedPercent": 40.0,
            "status": "normal",
        })];
        assert!(build_quota_pools(&quotas, &[], 0).is_empty());
    }
}
