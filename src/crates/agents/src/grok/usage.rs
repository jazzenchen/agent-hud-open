//! Grok Bot's tokens, per bot and day, from Cursor's servers: the request
//! Grok Bot's own usage page sends (`GetDailySpendByCategory` grouped by bot).
//! The bots run on Cursor's servers, so this Mac keeps none of their tokens.
//! An account merged into SuperGrok is not metered there and reads nothing;
//! an account that was not merged reads each bot's daily total, its
//! sub-agents, groups and routines included.
use super::cache as bot;
use crate::account::{Host, HttpFailure, HttpRequest};
use crate::cursor::account::{sign_in, SignIn};
use crate::l10n;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::time::Duration;

const SERVICE: &str = "https://api2.cursor.sh/aiserver.v1.DashboardService/";
/// `SpendGroupByCategory.GROK_BOT` and `SpendType.ALL`, as Grok Bot's bundle
/// declares them. The server takes the numbers; the bare name "GROK_BOT" is
/// not a value it knows and falls back to grouping by model.
const BY_BOT: u64 = 4;
const ALL_SPEND: u64 = 3;
/// The category the server gives spend no bot claims.
const UNATTRIBUTED: &str = "other-bots";

fn post(host: &dyn Host, token: &str, method: &str, body: &Value) -> Result<Value, HttpFailure> {
    host.http_json(&HttpRequest {
        url: &format!("{SERVICE}{method}"),
        secrets: &[("Authorization", &format!("Bearer {token}"))],
        headers: &[("Connect-Protocol-Version", "1")],
        body: Some(body),
        timeout: Duration::from_secs(12),
    })
}

fn number(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

/// The session records of the last seven days of Grok Bot's per-bot usage,
/// for the Bot signed in as `slot`. Empty for an account the servers do not
/// meter.
pub(super) fn read(
    host: &dyn Host,
    directory: &Path,
    slot: &str,
    now_ms: i64,
) -> Result<Vec<Value>, String> {
    let Some(SignIn { token, .. }) = sign_in(host)? else {
        return Err(l10n::text(
            "Grok Bot 的 Token 用 Cursor 的登录读取；登录 Cursor 后可见",
            "Grok Bot's tokens are read with Cursor's sign-in; sign in to Cursor to see them.",
        ));
    };
    let unreachable = |failure: HttpFailure| match failure {
        HttpFailure::Unreachable => l10n::text(
            "无法读取 Grok Bot 的 Token 用量，请检查网络",
            "Grok Bot's token usage could not be reached; check the connection.",
        ),
        other => other.message(),
    };
    let me = post(host, &token, "GetMe", &json!({})).map_err(unreachable)?;
    let start = ((now_ms - 7 * 86_400_000) / 86_400_000) * 86_400_000;
    let mut body = json!({
        "periodStartMs": start.to_string(),
        "periodEndMs": now_ms.to_string(),
        "groupBy": BY_BOT,
        "spendType": ALL_SPEND,
        "clientType": "grok-bot",
        "products": ["sand"],
    });
    if let Some(user) = me["userId"]
        .as_i64()
        .map(Value::from)
        .or_else(|| me["userId"].as_str().map(Value::from))
    {
        body["userId"] = user;
    }
    let reply = post(host, &token, "GetDailySpendByCategory", &body).map_err(unreachable)?;
    let Some(rows) = reply["dailySpend"].as_array().or_else(|| {
        // An account with nothing metered answers an empty object.
        reply
            .as_object()
            .filter(|o| !o.contains_key("dailySpend"))
            .map(|_| &EMPTY)
    }) else {
        return Err(format_error());
    };
    let roster = roster(host, directory, slot);
    let mut totals = BTreeMap::<(String, i64), u64>::new();
    for row in rows {
        let (Some(category), Some(day)) = (row["category"].as_str(), number(&row["day"])) else {
            return Err(format_error());
        };
        let Some(tokens) = number(&row["totalTokens"]).filter(|t| *t > 0) else {
            continue;
        };
        let category = match category {
            "Other" | "unattributed" => UNATTRIBUTED,
            id => id,
        };
        if !bot::agent_id(category) {
            return Err(format_error());
        }
        *totals.entry((category.to_owned(), day as i64)).or_default() += tokens;
    }
    // Cursor signed in to another account than Grok Bot would bring that
    // account's bots: rows must name at least one of this Bot's.
    if !totals.is_empty() && !totals.keys().any(|(id, _)| roster.contains(id)) {
        return Err(l10n::text(
            "Cursor 登录的账户与 Grok Bot 不同，无法读取 Grok Bot 的 Token",
            "Cursor is signed in to another account than Grok Bot; its tokens are not read.",
        ));
    }
    Ok(totals
        .into_iter()
        .map(|((id, day), tokens)| {
            let title = if id == UNATTRIBUTED {
                Some(l10n::text("其他 Bot", "Other bots"))
            } else if roster.contains(&id) {
                None
            } else {
                // A bot since removed from the roster.
                Some(format!("Grok Bot · {}", &id[..id.len().min(8)]))
            };
            json!({
                "source": "grok-bot",
                "id": bot::session_id(slot, &id),
                "title": title,
                "model": "grok-bot",
                "key": format!("spend-{id}-{}", day / 1000),
                "at": day,
                "input": tokens,
                "output": 0,
                "read": 0,
                "write": 0,
            })
        })
        .collect())
}

static EMPTY: Vec<Value> = Vec::new();

fn format_error() -> String {
    l10n::text(
        "Grok Bot 的 Token 用量格式无法识别",
        "Grok Bot's token usage has an unsupported format.",
    )
}

/// The bots in the Bot's roster for `slot`.
fn roster(host: &dyn Host, directory: &Path, slot: &str) -> HashSet<String> {
    bot::value_through(host, directory, &bot::roster_key(slot), 4)
        .and_then(|roster| {
            roster["rows"].as_array().map(|rows| {
                rows.iter()
                    .filter_map(|row| row["id"].as_str().filter(|id| bot::agent_id(id)))
                    .map(str::to_owned)
                    .collect()
            })
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    const DIR: &str = "/u/AppData/Grok Bot/sand-client-persistence";
    const CURSOR_DB: &str = "/u/AppData/Cursor/User/globalStorage/state.vscdb";
    const ME: &str = "https://api2.cursor.sh/aiserver.v1.DashboardService/GetMe";
    const SPEND: &str =
        "https://api2.cursor.sh/aiserver.v1.DashboardService/GetDailySpendByCategory";
    const WEEKLY: &str = "https://api2.cursor.sh/aiserver.v1.DashboardService/GetSandUsageStatus";
    const PERIOD: &str =
        "https://api2.cursor.sh/aiserver.v1.DashboardService/GetCurrentPeriodUsage";
    const DAY: i64 = 1_792_972_800_000;
    /// The Bot's slot when Cursor is signed in to the same account.
    const SAME: &str = "auth0|user_123";

    fn entry(key: &str, schema: u64, value: Value) -> (String, String) {
        let file = bot::path(DIR.as_ref(), key);
        let text = format!(
            "{}\n{}",
            serde_json::to_string(key).unwrap(),
            json!({"schemaVersion": schema, "value": value})
        );
        (file.to_string_lossy().into_owned(), text)
    }

    fn token() -> String {
        let claims = json!({"exp": chrono::Utc::now().timestamp() + 3600, "sub": "auth0|user_123"});
        format!(
            "h.{}.s",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
        )
    }

    /// Grok Bot signed in as slot-1 with two bots, Cursor signed in to
    /// another account, and the servers answering `spend` and, when given,
    /// `quota` in the cache.
    fn host(dir: &tempfile::TempDir, spend: Value, quota: Option<Value>) -> FakeHost {
        host_as(dir, "slot-1", spend, quota)
    }

    /// `host`, with Grok Bot signed in as `slot`.
    fn host_as(
        dir: &tempfile::TempDir,
        slot: &str,
        spend: Value,
        quota: Option<Value>,
    ) -> FakeHost {
        let (account_file, account) = entry(bot::ACCOUNT_KEY, 1, json!(slot));
        let (roster_file, roster) = entry(
            &bot::roster_key(slot),
            4,
            json!({"rows": [{"id": "bot-a", "title": "SEO"}, {"id": "bot-b", "title": "Sego"}]}),
        );
        let state = dir.path().join("state.vscdb");
        let db = rusqlite::Connection::open(&state).unwrap();
        db.execute_batch("CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value BLOB);")
            .unwrap();
        db.execute(
            "INSERT INTO ItemTable VALUES ('cursorAuth/accessToken', ?1)",
            [token()],
        )
        .unwrap();
        let mut host = FakeHost::new("/u")
            .file(&account_file, &account)
            .file(&roster_file, &roster)
            .answer(ME, Ok(json!({"userId": 123})))
            .answer(SPEND, Ok(spend));
        if let Some(quota) = quota {
            let (quota_file, quota) = entry(&bot::quota_key(slot), 2, quota);
            host = host.file(&quota_file, &quota);
        }
        host.databases.insert(CURSOR_DB.into(), state);
        host
    }

    fn row(category: &str, day: i64, tokens: &str) -> Value {
        json!({"day": day.to_string(), "category": category, "totalTokens": tokens})
    }

    #[test]
    fn each_bots_daily_total_is_read_with_the_request_grok_bot_sends() {
        let dir = tempfile::tempdir().unwrap();
        let spend = json!({"dailySpend": [
            row("bot-a", DAY, "4008832"),
            row("Other", DAY, "10"),
            row("gone-bot-1", DAY - 86_400_000, "5"),
            row("bot-b", DAY, "0"),
        ]});
        let host = host(&dir, spend, None);
        let records = read(&host, DIR.as_ref(), "slot-1", DAY + 3_600_000).unwrap();
        assert_eq!(records.len(), 3, "a day without tokens is left out");
        let by = |id: &str| {
            records
                .iter()
                .find(|r| r["id"] == bot::session_id("slot-1", id))
                .unwrap()
                .clone()
        };
        let seo = by("bot-a");
        assert_eq!(seo["source"], "grok-bot");
        assert!(seo["title"].is_null(), "the roster names its bots");
        assert_eq!(seo["input"], 4_008_832);
        assert_eq!(seo["key"], format!("spend-bot-a-{}", DAY / 1000));
        assert!(seo.get("archived").is_none());
        assert_eq!(by("other-bots")["title"], "Other bots");
        assert_eq!(by("gone-bot-1")["title"], "Grok Bot · gone-bot");

        let sent = host.sent.borrow();
        let (secrets, body) = &sent[1];
        assert!(secrets[0].1.starts_with("Bearer "));
        let body = body.as_ref().unwrap();
        assert_eq!(body["groupBy"], 4);
        assert_eq!(body["spendType"], 3);
        assert_eq!(body["userId"], 123);
        assert_eq!(body["clientType"], "grok-bot");
        assert_eq!(body["products"], json!(["sand"]));
    }

    #[test]
    fn a_merged_account_reads_nothing_and_another_accounts_bots_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            read(&host(&dir, json!({}), None), DIR.as_ref(), "slot-1", DAY)
                .unwrap()
                .is_empty()
        );
        let dir = tempfile::tempdir().unwrap();
        let theirs = json!({"dailySpend": [row("not-ours", DAY, "7")]});
        assert_eq!(
            read(&host(&dir, theirs, None), DIR.as_ref(), "slot-1", DAY).unwrap_err(),
            "Cursor is signed in to another account than Grok Bot; its tokens are not read."
        );
        let dir = tempfile::tempdir().unwrap();
        let odd = json!({"dailySpend": "none"});
        assert!(read(&host(&dir, odd, None), DIR.as_ref(), "slot-1", DAY).is_err());
    }

    #[test]
    fn the_reading_shows_a_failure_and_a_stale_quota_does_not_hold_tokens_back() {
        let now = chrono::Utc::now().timestamp_millis();
        let fresh = json!({
            "kind": "present", "selectedTeamId": null, "expiresAtMs": now + 3_600_000,
            "reading": {"readAtMs": now - 60_000, "usage": {
                "hasNonZeroIncludedLimit": true, "isSandTrial": false, "percentUsed": 42.0,
                "nextResetMs": now + 86_400_000, "grokPlanLabel": "SuperGrok"}},
        });
        let today = (now / 86_400_000) * 86_400_000;
        let spend = json!({"dailySpend": [row("bot-a", today, "9")]});

        // Fresh quota and tokens: both, and no failure to tell.
        let dir = tempfile::tempdir().unwrap();
        let reading =
            super::super::account::read(&host(&dir, spend.clone(), Some(fresh.clone()))).unwrap();
        assert_eq!(reading.quotas.len(), 1);
        assert_eq!(reading.session_records.len(), 1);

        // The servers unreachable: the quota stays and the panel says why.
        let dir = tempfile::tempdir().unwrap();
        let mut unreachable = host(&dir, spend.clone(), Some(fresh.clone()));
        unreachable.web.remove(SPEND);
        let reading = super::super::account::read(&unreachable).unwrap();
        assert_eq!(reading.quotas.len(), 1);
        assert!(reading
            .notice
            .unwrap()
            .ends_with(" · Grok Bot's token usage could not be reached; check the connection."));

        // An expired quota cache: the tokens still come, with the reason.
        let mut expired = fresh.clone();
        expired["expiresAtMs"] = json!(1);
        let dir = tempfile::tempdir().unwrap();
        let reading =
            super::super::account::read(&host(&dir, spend, Some(expired.clone()))).unwrap();
        assert!(reading.quotas.is_empty());
        assert_eq!(reading.session_records.len(), 1);
        assert!(reading.notice.is_some());
        // Without tokens the expired cache is the error it was.
        let dir = tempfile::tempdir().unwrap();
        assert!(super::super::account::read(&host(&dir, json!({}), Some(expired))).is_err());
    }
    fn cached_quota(now: i64, usage: Value) -> Value {
        json!({"kind": "present", "selectedTeamId": null, "expiresAtMs": now + 3_600_000,
               "reading": {"readAtMs": now - 60_000, "usage": usage}})
    }

    #[test]
    fn the_weekly_quota_comes_from_the_servers_when_cursor_is_the_bots_account() {
        let reset = (chrono::Utc::now() + chrono::Duration::days(3)).to_rfc3339();
        let dir = tempfile::tempdir().unwrap();
        let host = host_as(&dir, SAME, json!({}), None)
            .answer(
                WEEKLY,
                Ok(json!({"usagePercent": 0.37, "nextResetTimestampUtc": reset,
                          "hasNonZeroIncludedLimit": true, "grokPlanLabel": "Grok Bot Plan",
                          "isUnified": true, "cursorPlanName": "Super"})),
            )
            .answer(PERIOD, Ok(json!({})));
        let reading = super::super::account::read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 1);
        assert_eq!(reading.quotas[0]["usedPercent"], 0.37);
        assert_eq!(reading.quotas[0]["name"], "Weekly usage limit");
        let account = reading.account.unwrap();
        assert_eq!(
            account["plan"], "Super",
            "a merged account goes by its merged plan"
        );
        let checked = chrono::DateTime::parse_from_rfc3339(account["checkedAt"].as_str().unwrap());
        assert!(chrono::Utc::now().timestamp() - checked.unwrap().timestamp() < 60);
        assert!(reading.notice.is_none(), "no cache and no failure to tell");
    }

    #[test]
    fn the_bots_cache_stands_in_when_the_servers_fail_or_cursor_is_another_account() {
        let now = chrono::Utc::now().timestamp_millis();
        let usage = json!({"hasNonZeroIncludedLimit": true, "isSandTrial": false,
                           "percentUsed": 3.0, "grokPlanLabel": "Grok Bot Plan"});

        // Servers unreachable: the cache, and the reason beside its notice.
        let mut merged = usage.clone();
        merged["unifiedPlanName"] = json!("Super");
        let dir = tempfile::tempdir().unwrap();
        let same = host_as(&dir, SAME, json!({}), Some(cached_quota(now, merged)));
        let reading = super::super::account::read(&same).unwrap();
        assert_eq!(reading.quotas[0]["usedPercent"], 3.0);
        assert_eq!(reading.account.unwrap()["plan"], "Super");
        assert!(reading
            .notice
            .unwrap()
            .ends_with(" · Grok Bot's weekly usage could not be reached; check the connection."));

        // Cursor signed in to another account: its quota is not asked for,
        // and the placeholder plan name is no plan.
        let dir = tempfile::tempdir().unwrap();
        let other = host(&dir, json!({}), Some(cached_quota(now, usage))).answer(
            WEEKLY,
            Ok(json!({"usagePercent": 99.0, "hasNonZeroIncludedLimit": true})),
        );
        let reading = super::super::account::read(&other).unwrap();
        assert_eq!(reading.quotas[0]["usedPercent"], 3.0);
        assert!(reading.account.unwrap()["plan"].is_null());
        assert!(!other.asked.borrow().iter().any(|(url, _)| url == WEEKLY));
    }

    #[test]
    fn an_old_cache_says_so_when_the_servers_cannot_stand_in() {
        let now = chrono::Utc::now().timestamp_millis();
        let mut old = cached_quota(
            now,
            json!({"hasNonZeroIncludedLimit": true, "isSandTrial": false,
                                               "percentUsed": 3.0}),
        );
        old["expiresAtMs"] = json!(now - 1);
        let dir = tempfile::tempdir().unwrap();
        let why = super::super::account::read(&host(&dir, json!({}), Some(old))).err();
        assert_eq!(
            why.as_deref(),
            Some("Grok Bot's cached quota is more than a day old; open Grok Bot to refresh it.")
        );
    }
}
