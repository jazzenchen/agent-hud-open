//! When a readings record goes out: held back while accounts are still being
//! read, and sent again only when what was read changed.
use serde_json::Value;

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
