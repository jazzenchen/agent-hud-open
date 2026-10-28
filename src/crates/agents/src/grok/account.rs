//! Grok Bot's personal quota: from Cursor's servers when Cursor is signed in
//! to the Bot's account (`weekly.rs`), else from the cache the Bot keeps
//! itself. For the cache, when the Bot read it stays the observation time:
//! reading the file now does not make old data fresh. Its tokens come from
//! Cursor's servers (`usage.rs`).
use super::cache as bot;
use crate::account::{legacy_unresolved, locate, status, Host, Reading};
use crate::naming::window_names;
use crate::{l10n, Location, PerOs, Support};
use serde_json::{json, Value};

/// Grok Bot's desktop cache, where it is known.
pub const CACHE_LOCATION: Location = Location::app_data(&["Grok Bot"], "sand-client-persistence");
/// Grok Bot's desktop cache.
pub const CACHE: PerOs<Location> = PerOs {
    macos: Support::Verified(CACHE_LOCATION),
    windows: Support::Unsupported("Grok Bot's cache location on Windows is not known yet"),
    linux: Support::Unsupported("Grok Bot's cache location on Linux is not known yet"),
};

fn iso(ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d: chrono::DateTime<chrono::Utc>| d.to_rfc3339())
}

pub fn read(host: &dyn Host) -> Result<Reading, String> {
    let mut r = Reading::empty("grok-bot");
    let Ok(directory) = locate(host, &CACHE) else {
        r.notice = Some(l10n::text(
            "此系统暂不支持读取 Grok Bot 缓存",
            "Grok Bot cache acquisition is not implemented on this platform",
        ));
        return Ok(r);
    };
    let Some(slot) = bot::account_through(host, &directory) else {
        r.notice = Some(l10n::text(
            "登录 Grok Bot 后读取其缓存的个人额度",
            "Sign in to Grok Bot to expose its cached personal quota",
        ));
        return Ok(r);
    };
    let now = chrono::Utc::now().timestamp_millis();
    let tokens = super::usage::read(host, &directory, &slot, now);
    let quota = match super::weekly::read(host, &slot, now) {
        Ok(Some(usage)) => quota(&usage, &slot, now),
        Ok(None) => cached(host, &directory, &slot, now),
        // The servers failed: the Bot's own cache, with the reason.
        Err(why) => match cached(host, &directory, &slot, now) {
            Ok(mut r) => {
                r.notice = Some(joined(r.notice.take(), why));
                Ok(r)
            }
            Err(_) => Err(why),
        },
    };
    match (quota, tokens) {
        (Ok(mut r), Ok(records)) => {
            r.session_records = records;
            Ok(r)
        }
        (Ok(mut r), Err(why)) => {
            r.notice = Some(joined(r.notice.take(), why));
            Ok(r)
        }
        // A missing quota does not hold the tokens back: the bots run
        // whether or not the Bot was opened today.
        (Err(why), Ok(records)) if !records.is_empty() => {
            let mut r = Reading::empty("grok-bot");
            r.session_records = records;
            r.notice = Some(why);
            Ok(r)
        }
        (Err(why), _) => Err(why),
    }
}

/// `notice` and then `why`, each said once.
fn joined(notice: Option<String>, why: String) -> String {
    match notice {
        Some(notice) if notice.split(" · ").any(|part| part == why) => notice,
        Some(notice) => format!("{notice} · {why}"),
        None => why,
    }
}

/// The plan name Grok Bot keeps when it has none to show; the Bot itself
/// never shows it.
const NO_PLAN_NAME: &str = "Grok Bot Plan";

/// The weekly quota the Bot's cache holds for `slot`, while the Bot's own
/// expiry (a day after it read it) has not passed.
fn cached(
    host: &dyn Host,
    directory: &std::path::Path,
    slot: &str,
    now: i64,
) -> Result<Reading, String> {
    let value = bot::value_through(host, directory, &bot::quota_key(slot), 2).ok_or_else(|| {
        l10n::text(
            "Grok Bot 缓存额度不可用，请在 Bot 中刷新",
            "Grok Bot cached quota is unavailable; refresh in Bot",
        )
    })?;
    if value["kind"] != "present" || !value["selectedTeamId"].is_null() {
        return Err(no_personal_quota());
    }
    let (expires, observed) = (
        value["expiresAtMs"].as_i64(),
        value["reading"]["readAtMs"].as_i64(),
    );
    let Some(observed) = observed.filter(|at| {
        *at <= now && now - at < 86_400_000 && expires.is_some_and(|e| e > now && e > *at)
    }) else {
        return Err(l10n::text(
            "Grok Bot 缓存的额度已超过一天，打开 Grok Bot 即可刷新",
            "Grok Bot's cached quota is more than a day old; open Grok Bot to refresh it.",
        ));
    };
    let mut r = quota(&value["reading"]["usage"], slot, observed)?;
    r.notice = Some(l10n::text(
        "额度来自 Grok Bot 缓存；在 Bot 中刷新可更新读数",
        "Quota from Grok Bot cache; refresh in Bot to update",
    ));
    if bot::account_through(host, directory).as_deref() != Some(slot) {
        return Err(l10n::text(
            "读取缓存期间 Grok Bot 账户发生了变化",
            "Grok Bot account changed while reading its cache",
        ));
    }
    Ok(r)
}

fn no_personal_quota() -> String {
    l10n::text(
        "Grok Bot 在当前账户或团队下没有个人周额度",
        "Grok Bot has no personal weekly limit for this account or team.",
    )
}

/// The quota of Grok Bot's weekly `usage` (as its cache keeps it, or as
/// `weekly` reads it from the servers) for `slot`, read at `observed`.
fn quota(u: &Value, slot: &str, observed: i64) -> Result<Reading, String> {
    if u["hasNonZeroIncludedLimit"] != true
        || !u["isSandTrial"].is_boolean()
        || u["isTeamSeat"] == true
    {
        return Err(no_personal_quota());
    }
    let mut r = Reading::empty("grok-bot");
    // The Swift app's account for this Bot slot; its rows are that app's `grok` windows.
    let id = legacy_unresolved("Grok", &format!("grok-bot:{}", bot::hash(&[slot])));
    let used = u["percentUsed"]
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.)
        .map(|n| n.clamp(0., 100.));
    let reset = u["nextResetMs"].as_i64().and_then(iso);
    if !u["nextResetMs"].is_null() && reset.is_none() {
        return Err(l10n::text(
            "Grok Bot 缓存的重置时间无效",
            "Grok Bot cache reset time is invalid",
        ));
    }
    let observed = iso(observed);
    let trial = u["isSandTrial"] == true;
    r.quotas.push(json!({"id":format!("{id}/grok"),"vendor":"grok-bot","accountId":id,"legacyWindowId":format!("{id}/grok"),"name":if trial{"Trial usage limit"}else{"Weekly usage limit"},"shortLabel":if trial{"Trial".to_owned()}else{window_names::Period::Week.short_name()},"usedPercent":used,"resetAt":reset,"windowDurationSeconds":if trial{None}else{Some(604800)},"allModels":true,"observedAt":observed,"status":used.map(status).unwrap_or("unknown")}));
    let cap = u["onDemand"]["limitCents"].as_f64();
    let extra = u["onDemand"]["usedCents"].as_f64();
    if cap.is_some_and(|n| n > 0.) {
        let used = cap.zip(extra).map(|(c, e)| (e / c * 100.).clamp(0., 100.));
        r.quotas.push(json!({"id":format!("{id}/grok:extra"),"vendor":"grok-bot","accountId":id,"legacyWindowId":format!("{id}/grok:extra"),"name":"Extra usage","shortLabel":"Extra","usedPercent":used,"resetAt":reset,"allModels":true,"observedAt":observed,"status":used.map(status).unwrap_or("unknown")}));
    }
    // A merged (SuperGrok) account goes by its merged plan, as the Bot shows it.
    let plan = u["unifiedPlanName"]
        .as_str()
        .or_else(|| u["grokPlanLabel"].as_str().filter(|p| *p != NO_PLAN_NAME))
        .filter(|p| !p.is_empty());
    r.account = Some(
        json!({"id":id,"provider":"grok-bot","identity":"unresolved","plan":plan,"checkedAt":observed}),
    );
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{fake::FakeHost, FileInfo};
    use crate::Os;
    use serde_json::Value;

    const DIR: &str = "/u/AppData/Grok Bot/sand-client-persistence";

    fn entry(key: &str, schema: u64, value: Value) -> (String, String) {
        let file = bot::path(DIR.as_ref(), key);
        let text = format!(
            "{}\n{}",
            serde_json::to_string(key).unwrap(),
            json!({"schemaVersion": schema, "value": value})
        );
        (file.to_string_lossy().into_owned(), text)
    }

    fn host_with(quota: Value) -> FakeHost {
        let (account_file, account) = entry(bot::ACCOUNT_KEY, 1, json!("slot-1"));
        let (quota_file, quota) = entry(&bot::quota_key("slot-1"), 2, quota);
        FakeHost::new("/u")
            .file(&account_file, &account)
            .file(&quota_file, &quota)
    }

    fn fresh_quota(trial: bool) -> Value {
        let now = chrono::Utc::now().timestamp_millis();
        json!({
            "kind": "present",
            "selectedTeamId": null,
            "expiresAtMs": now + 3_600_000,
            "reading": {"readAtMs": now - 60_000, "usage": {
                "hasNonZeroIncludedLimit": true,
                "isSandTrial": trial,
                "percentUsed": 42.0,
                "nextResetMs": now + 86_400_000,
                "grokPlanLabel": "SuperGrok",
                "onDemand": {"limitCents": 1000, "usedCents": 950},
            }},
        })
    }

    #[test]
    fn the_cache_is_read_where_the_descriptor_puts_it() {
        let reading = read(&host_with(fresh_quota(false))).unwrap();
        let id = legacy_unresolved("Grok", &format!("grok-bot:{}", bot::hash(&["slot-1"])));
        assert_eq!(reading.quotas.len(), 2);
        assert_eq!(reading.quotas[0]["id"], format!("{id}/grok"));
        // The Swift app's account for slot-1, as its quota history keys it.
        let open = "account:481bd5386e9b3de64d1ae81e84810325bcac7510e46cea250c975c2fa4fb3623";
        assert_eq!(id, open);
        assert_eq!(reading.quotas[0]["legacyWindowId"], format!("{open}/grok"));
        assert_eq!(
            reading.quotas[1]["legacyWindowId"],
            format!("{open}/grok:extra")
        );
        assert_eq!(reading.quotas[0]["usedPercent"], 42.0);
        assert_eq!(reading.quotas[0]["windowDurationSeconds"], 604800);
        assert_eq!(reading.quotas[1]["usedPercent"], 95.0);
        assert_eq!(reading.quotas[1]["status"], "critical");
        assert_eq!(reading.account.unwrap()["plan"], "SuperGrok");
    }

    #[test]
    fn a_trial_has_no_weekly_window() {
        let reading = read(&host_with(fresh_quota(true))).unwrap();
        assert_eq!(reading.quotas[0]["name"], "Trial usage limit");
        assert!(reading.quotas[0]["windowDurationSeconds"].is_null());
    }

    #[test]
    fn other_systems_say_the_cache_is_not_supported_yet() {
        let mut host = host_with(fresh_quota(false));
        host.os = Os::Windows;
        let reading = read(&host).unwrap();
        assert_eq!(
            reading.notice.as_deref(),
            Some("Grok Bot cache acquisition is not implemented on this platform")
        );
        assert!(reading.quotas.is_empty());
    }

    #[test]
    fn stale_or_linked_caches_are_refused() {
        let mut quota = fresh_quota(false);
        quota["expiresAtMs"] = json!(1);
        assert!(read(&host_with(quota)).is_err());

        let mut host = host_with(fresh_quota(false));
        let file = bot::path(DIR.as_ref(), bot::ACCOUNT_KEY);
        host.info.insert(
            file,
            FileInfo {
                len: 10,
                modified_ms: None,
                symlink: true,
            },
        );
        let reading = read(&host).unwrap();
        assert!(
            reading.quotas.is_empty(),
            "a linked account file is not read"
        );
    }
}
