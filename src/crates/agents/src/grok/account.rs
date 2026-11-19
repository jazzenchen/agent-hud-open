//! Grok Bot's personal quota: from Cursor's servers when Cursor is signed in
//! to the Bot's account (`weekly.rs`), else from the cache the Bot keeps
//! itself. For the cache, when the Bot read it stays the observation time:
//! reading the file now does not make old data fresh. Its tokens come from
//! Cursor's servers (`usage.rs`).
//!
//! The Bot fills its cache from the same servers, so while they miss an
//! answer the cache is no newer than the last reading: that failure goes to
//! the engine as it is, which keeps the last reading through a short outage.
//! Any other server failure falls back to the cache, with the reason.
use super::cache as bot;
use crate::account::{
    add_readers, legacy_unresolved, locate, read_by, status, Host, Provider, QuotaReading, Reading,
    Unread,
};
use crate::cursor::account::{sign_in, SignIn};
use crate::naming::window_names;
use crate::{iso, l10n::tr, Location, PerOs, Support};
use serde_json::{json, Value};

/// Grok Bot's desktop cache, where it is known.
pub const CACHE_LOCATION: Location = Location::app_data(&["Grok Bot"], "sand-client-persistence");
/// Grok Bot's desktop cache.
pub const CACHE: PerOs<Location> = PerOs {
    macos: Support::Verified(CACHE_LOCATION),
    windows: Support::Verified(CACHE_LOCATION),
    linux: Support::Unsupported("Grok Bot's cache location on Linux is not known yet"),
};

/// Grok's quota covers its CLI and its Bot, and the Bot's bots run on
/// Cursor's servers, so it is read on the account interval.
pub static PROVIDER: Provider = Provider {
    local_work: false,
    work_sources: &["grok-bot", "grok-cli"],
    ..Provider::new("grok-bot", read)
};

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let mut r = Reading::empty("grok-bot");
    let Ok(directory) = locate(host, &CACHE) else {
        r.notice = Some(tr!(
            "Grok Bot cache acquisition is not implemented on this platform"
        ));
        return Ok(r);
    };
    let Some(slot) = bot::account_through(host, &directory) else {
        r.notice = Some(tr!(
            "Sign in to Grok Bot to expose its cached personal quota"
        ));
        return Ok(r);
    };
    let now = chrono::Utc::now().timestamp_millis();
    // Cursor's sign-in, read once: the Bot's account is read with it. It is
    // another app's, so only while Cursor's own quota reading allows it.
    let cursor_allowed = host
        .quota_reading(crate::cursor::AGENT.id)
        .allows(QuotaReading::Full);
    let cursor = if cursor_allowed {
        sign_in(host).map_err(|why| why.reason().to_owned())
    } else {
        Err(cursor_off())
    };
    let same = match &cursor {
        Ok(Some(cursor)) if super::weekly::same_account(&slot, cursor) => Some(cursor),
        _ => None,
    };
    // The account's name, known when Cursor is signed in to it.
    let email = same.and_then(|cursor| cursor.email.as_deref());
    let quota = match super::weekly::read(host, same, now) {
        Ok(Some(usage)) => quota(&usage, &slot, now, email),
        Ok(None) => cached(host, &directory, &slot, now, email),
        Err(why @ Unread::Transient { .. }) => return Err(why),
        // The servers failed otherwise: the Bot's own cache, with the reason.
        Err(why) => match cached(host, &directory, &slot, now, email) {
            Ok(mut r) => {
                r.notice = Some(joined(r.notice.take(), why.reason().to_owned()));
                Ok(r)
            }
            Err(_) => Err(why.reason().to_owned()),
        },
    }
    .map(|mut r| {
        read_through(&mut r, same.is_some());
        r
    });
    let tokens = super::usage::read(host, &directory, &slot, &cursor, now);
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
        // Nothing left without Cursor's sign-in: say so, as an off reading does.
        (Err(why), _) if !cursor_allowed => {
            let mut r = Reading::empty("grok-bot");
            r.notice = Some(joined(Some(cursor_off()), why));
            Ok(r)
        }
        (Err(why), _) => Err(why.into()),
    }
}

/// Why the Bot is read without Cursor's sign-in.
fn cursor_off() -> String {
    tr!("Cursor's quota reading is off in Settings, so Grok Bot is read without Cursor's sign-in.")
}

/// The Bot's account is read through the Bot, and through Cursor too when
/// Cursor is signed in to it: xAI merged that Cursor account into SuperGrok,
/// so one account serves both.
fn read_through(r: &mut Reading, cursor: bool) {
    read_by(r, "grok-bot");
    if cursor {
        let reader = json!({ "readBy": ["cursor"] });
        for account in r.account.iter_mut() {
            add_readers(account, &reader);
        }
    }
}

/// Whether Grok Bot is signed in to `cursor`'s account, the one Cursor is
/// signed in to.
pub(crate) fn bot_signed_in_to(host: &dyn Host, cursor: &SignIn) -> bool {
    locate(host, &CACHE)
        .ok()
        .and_then(|directory| bot::account_through(host, &directory))
        .is_some_and(|slot| super::weekly::same_account(&slot, cursor))
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
    email: Option<&str>,
) -> Result<Reading, String> {
    // Grok Bot 1.1.0 writes schema v3 (it adds prepaid-balance fields); older
    // Bot versions wrote v2. Accept either so a fresh cache is not discarded.
    let value = bot::value_through(host, directory, &bot::quota_key(slot), 3)
        .or_else(|| bot::value_through(host, directory, &bot::quota_key(slot), 2))
        .ok_or_else(|| tr!("Grok Bot cached quota is unavailable; refresh in Bot"))?;
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
        return Err(tr!(
            "Grok Bot's cached quota is more than a day old; open Grok Bot to refresh it."
        ));
    };
    let mut r = quota(&value["reading"]["usage"], slot, observed, email)?;
    r.notice = Some(tr!("Quota from Grok Bot cache; refresh in Bot to update"));
    if bot::account_through(host, directory).as_deref() != Some(slot) {
        return Err(tr!("Grok Bot account changed while reading its cache"));
    }
    Ok(r)
}

fn no_personal_quota() -> String {
    tr!("Grok Bot has no personal weekly limit for this account or team.")
}

/// The quota of Grok Bot's weekly `usage` (as its cache keeps it, or as
/// `weekly` reads it from the servers) for `slot`, read at `observed`, on
/// the account named `email` where Cursor's sign-in to it tells.
fn quota(u: &Value, slot: &str, observed: i64, email: Option<&str>) -> Result<Reading, String> {
    // A trial reports its usage without including a weekly allowance; that
    // reading is still the Bot's own quota, so show it rather than claim the
    // account has no personal limit. Accounts that are neither trial nor
    // carry an included limit (a pooled team allowance) keep the notice.
    let trial = u["isSandTrial"] == true;
    if !u["isSandTrial"].is_boolean()
        || u["isTeamSeat"] == true
        || (u["hasNonZeroIncludedLimit"] != true && !trial)
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
        return Err(tr!("Grok Bot cache reset time is invalid"));
    }
    let observed = iso(observed);
    r.quotas.push(json!({"id":format!("{id}/grok"),"vendor":"grok-bot","consumerSource":"grok-bot","accountId":id,"legacyWindowId":format!("{id}/grok"),"name":if trial{"Trial usage limit"}else{"Weekly usage limit"},"shortLabel":if trial{"Trial".to_owned()}else{window_names::Period::Week.short_name()},"usedPercent":used,"resetAt":reset,"windowDurationSeconds":if trial{None}else{Some(604800)},"allModels":true,"observedAt":observed,"status":used.map(status).unwrap_or("unknown")}));
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
        json!({"id":id,"provider":"grok-bot","identity":"unresolved","label":email,"plan":plan,"checkedAt":observed}),
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

    fn host_with_schema(quota: Value, schema: u64) -> FakeHost {
        let (account_file, account) = entry(bot::ACCOUNT_KEY, 1, json!("slot-1"));
        let (quota_file, quota) = entry(&bot::quota_key("slot-1"), schema, quota);
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
    fn the_bots_account_is_read_through_cursor_too_when_cursor_is_signed_in_to_it() {
        // No Cursor here: the Bot's account is the Bot's alone.
        let mut reading = read(&host_with(fresh_quota(false))).unwrap();
        assert_eq!(
            reading.account.as_ref().unwrap()["readBy"],
            json!(["grok-bot"])
        );
        read_through(&mut reading, true);
        assert_eq!(
            reading.account.unwrap()["readBy"],
            json!(["grok-bot", "cursor"])
        );
    }

    #[test]
    fn quota_from_schema_v3_cache_is_read() {
        let reading = read(&host_with_schema(fresh_quota(false), 3)).unwrap();
        assert_eq!(reading.quotas[0]["usedPercent"], 42.0);
        assert_eq!(reading.quotas[0]["windowDurationSeconds"], 604800);
    }

    #[test]
    fn a_trial_has_no_weekly_window() {
        let reading = read(&host_with(fresh_quota(true))).unwrap();
        assert_eq!(reading.quotas[0]["name"], "Trial usage limit");
        assert!(reading.quotas[0]["windowDurationSeconds"].is_null());
    }

    #[test]
    fn a_trial_without_an_included_limit_still_reports_its_usage() {
        let mut quota = fresh_quota(true);
        quota["reading"]["usage"]["hasNonZeroIncludedLimit"] = json!(false);
        let reading = read(&host_with(quota)).unwrap();
        assert_eq!(reading.quotas[0]["name"], "Trial usage limit");
        assert_eq!(reading.quotas[0]["usedPercent"], 42.0);
        assert_eq!(reading.account.unwrap()["plan"], "SuperGrok");
    }

    #[test]
    fn other_systems_say_the_cache_is_not_supported_yet() {
        let mut host = host_with(fresh_quota(false));
        host.os = Os::Linux;
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

    /// The Bot signed in to the account Cursor is signed in to, with a
    /// fresh cache, and Cursor's sign-in in its database.
    fn with_cursor(dir: &tempfile::TempDir, quota: Value) -> FakeHost {
        let slot = "auth0|user_123";
        let (account_file, account) = entry(bot::ACCOUNT_KEY, 1, json!(slot));
        let (quota_file, quota) = entry(&bot::quota_key(slot), 2, quota);
        let claims = json!({"exp": chrono::Utc::now().timestamp() + 3600, "sub": slot});
        let token = format!(
            "h.{}.s",
            base64::Engine::encode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                serde_json::to_vec(&claims).unwrap(),
            )
        );
        let path = dir.path().join("state.vscdb");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS ItemTable (key TEXT PRIMARY KEY, value BLOB);",
        )
        .unwrap();
        db.execute(
            "INSERT OR REPLACE INTO ItemTable VALUES ('cursorAuth/accessToken', ?1)",
            [&token],
        )
        .unwrap();
        let mut host = FakeHost::new("/u")
            .file(&account_file, &account)
            .file(&quota_file, &quota);
        host.databases.insert(CURSOR_DB.into(), path);
        host
    }

    const CURSOR_DB: &str = "/u/AppData/Cursor/User/globalStorage/state.vscdb";

    fn cursors(host: &FakeHost) -> bool {
        let opened = host.read.borrow().iter().any(|path| {
            path == std::path::Path::new(CURSOR_DB) || path == std::path::Path::new("Cursor")
        });
        opened
            || host
                .asked
                .borrow()
                .iter()
                .any(|(url, _)| url.contains("cursor.sh"))
    }

    #[test]
    fn cursor_off_opens_and_sends_nothing_of_cursors() {
        let dir = tempfile::tempdir().unwrap();
        // In full, Cursor's sign-in is read and sent.
        let host = with_cursor(&dir, fresh_quota(false));
        let _ = read(&host);
        assert!(cursors(&host), "Cursor's sign-in is used in full");
        let mut host = with_cursor(&dir, fresh_quota(false));
        host.quota_levels.insert("cursor".into(), QuotaReading::Off);
        let reading = read(&host).unwrap();
        assert!(!cursors(&host), "nothing of Cursor's is opened or sent");
        // The Bot's own cache still shows, and says why there are no tokens.
        assert_eq!(reading.quotas[0]["usedPercent"], 42.0);
        assert_eq!(reading.account.unwrap()["readBy"], json!(["grok-bot"]));
        assert!(reading.notice.unwrap().contains(&cursor_off()));
    }

    #[test]
    fn cursor_off_with_nothing_left_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let mut quota = fresh_quota(false);
        quota["expiresAtMs"] = json!(1);
        let mut host = with_cursor(&dir, quota);
        host.quota_levels.insert("cursor".into(), QuotaReading::Off);
        let reading = read(&host).unwrap();
        assert!(reading.quotas.is_empty() && reading.account.is_none());
        assert!(reading.notice.unwrap().starts_with(&cursor_off()));
        assert!(!cursors(&host));
    }
}
