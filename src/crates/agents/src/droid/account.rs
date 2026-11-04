//! Factory Droid's usage limits, read with a Factory API key from the limits
//! endpoint the CLI itself calls (`GET /api/billing/limits`): the share used
//! of each five-hour, weekly and monthly window, for the Standard pool and,
//! where the plan has one, the Droid Core pool, and the extra usage balance.
//!
//! Only the API-key route: `FACTORY_API_KEY` in the environment, as the CLI
//! takes it, else in `~/.factory/.env`, where CodexBar looks for it. The key
//! is read, never written or refreshed. Droid's own sign-in (its keychain
//! entry, `auth.v2.*`) and the web app's cookies are never read.
//!
//! The windows' shapes and their reading (a positive `secondsRemaining`
//! before `windowEnd`, a window whose end passed counting as reset) follow
//! CodexBar's `FactoryStatusProbe` (MIT, commit 897bff3).
use crate::account::{
    account_id, status, Host, HttpFailure, HttpRequest, Provider, Reading, Unread,
};
use crate::{l10n, Location};
use serde_json::{json, Value};
use std::time::Duration;

/// The limits the CLI shows in `/usage`.
pub const LIMITS: &str = "https://api.factory.ai/api/billing/limits";
/// The variable that holds a Factory API key (`fk-…`).
pub const KEY: &str = "FACTORY_API_KEY";
/// A dotenv file with `FACTORY_API_KEY=…`.
const DOTENV: Location = Location::home(&[], &[".factory"], ".env");
const DOTENV_LIMIT: u64 = 64 * 1024;

/// Factory Droid's limits. Work on this computer spends them, as does any
/// other Droid signed in to the same account.
pub static PROVIDER: Provider = Provider::new("droid", read);

/// A key's value without the whitespace or quotes around it; None when empty
/// or unusable in a header.
fn cleaned(value: &str) -> Option<String> {
    let value = value.trim();
    let value = ['"', '\'']
        .iter()
        .find_map(|quote| {
            value
                .strip_prefix(*quote)
                .and_then(|rest| rest.strip_suffix(*quote))
        })
        .unwrap_or(value)
        .trim();
    (!value.is_empty() && !value.bytes().any(|byte| byte.is_ascii_control())).then(|| value.into())
}

/// `FACTORY_API_KEY` in a dotenv file: `KEY=value` or `export KEY=value`,
/// comments and other keys skipped.
fn dotenv_key(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let line = line.strip_prefix("export ").map_or(line, str::trim_start);
        let (name, value) = line.split_once('=')?;
        (name.trim() == KEY).then(|| cleaned(value)).flatten()
    })
}

/// The API key, from the environment first.
fn api_key(host: &dyn Host) -> Option<String> {
    if let Some(key) = host.env(KEY).and_then(cleaned) {
        return Some(key);
    }
    let path = crate::account::resolve(host, &DOTENV).into_iter().next()?;
    let bytes = host.read_file(&path, DOTENV_LIMIT).found()?;
    if bytes.len() as u64 > DOTENV_LIMIT {
        return None;
    }
    dotenv_key(std::str::from_utf8(&bytes).ok()?)
}

fn unsupported() -> Unread {
    Unread::Failed(l10n::text(
        "Factory 的用量上限格式无法识别",
        "Factory's usage limits have an unsupported format",
    ))
}

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let mut reading = Reading::empty("droid");
    let Some(key) = api_key(host) else {
        // Droid signs in on its own; only an API key lets Agent HUD read
        // its limits, and having none is no sign-out.
        reading.notice = Some(l10n::text(
            "设置 FACTORY_API_KEY 后才能读取 Factory Droid 的用量上限",
            "Set FACTORY_API_KEY to read Factory Droid's usage limits",
        ));
        return Ok(reading);
    };
    let authorization = format!("Bearer {key}");
    let body = host
        .http_json(&HttpRequest {
            url: LIMITS,
            secrets: &[("Authorization", &authorization)],
            headers: &[("Accept", "application/json"), ("X-Factory-Client", "cli")],
            body: None,
            timeout: Duration::from_secs(15),
        })
        .map_err(|failure| match failure {
            // Factory no longer takes the key: only a new one brings the
            // reading back.
            HttpFailure::Status(401 | 403) => Unread::SignedOut(l10n::text(
                "Factory 不再接受这个 API Key，请在 app.factory.ai 重新生成",
                "Factory no longer accepts this API key; create a new one at app.factory.ai",
            )),
            HttpFailure::BadHeader => Unread::Failed(l10n::text(
                "FACTORY_API_KEY 无法用于请求",
                "FACTORY_API_KEY cannot be used in a request",
            )),
            HttpFailure::Unreachable => failure.unread_as(l10n::text(
                "无法连接 Factory 的用量服务",
                "Factory's usage limits could not be reached",
            )),
            other => other.unread(),
        })?;
    if !body.is_object() {
        return Err(unsupported());
    }
    // The key, never kept or shown, names the account it belongs to.
    let id = account_id("droid", &[&key]);
    let now = chrono::Utc::now().timestamp_millis();
    reading.quotas = windows(&body, &id, now);
    if let Some(cents) = body["extraUsageBalanceCents"]
        .as_f64()
        .filter(|cents| cents.is_finite())
        .filter(|cents| *cents > 0.0 || body["extraUsageAllowed"] == true)
    {
        reading.billing.push(json!({
            "id": format!("{id}/droid:extra-usage"), "vendor": "droid", "accountId": id,
            "label": l10n::text("额外用量余额", "Extra usage balance"),
            "amount": cents / 100.0, "currency": "USD",
        }));
    }
    if reading.quotas.is_empty() {
        if body["limits"].is_object() {
            return Err(unsupported());
        }
        // A plan billed another way reports no windows here.
        reading.notice = Some(l10n::text(
            "这个 Factory 套餐不按用量窗口计费",
            "This Factory plan reports no usage windows",
        ));
    }
    reading.account = Some(json!({"id": id, "provider": "droid", "label": null, "plan": null}));
    Ok(reading)
}

/// A window's end: an ISO date, or seconds or milliseconds since 1970, as a
/// number or a numeric string.
fn window_end(value: &Value) -> Option<i64> {
    let number = value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse::<f64>().ok());
    if let Some(n) = number.filter(|n| n.is_finite() && *n > 0.0) {
        return Some(if n > 1e12 {
            n as i64
        } else {
            (n * 1000.0) as i64
        });
    }
    chrono::DateTime::parse_from_rfc3339(value.as_str()?.trim())
        .ok()
        .map(|date| date.timestamp_millis())
}

/// The rows of every window the pools report, Standard first.
fn windows(body: &Value, id: &str, now: i64) -> Vec<Value> {
    let mut rows = Vec::new();
    for (pool, prefix_en, prefix_zh) in
        [("standard", "", ""), ("core", "Droid Core ", "Droid Core ")]
    {
        let limits = &body["limits"][pool];
        if !limits.is_object() {
            continue;
        }
        for (key, short, seconds, name_en, name_zh) in [
            (
                "fiveHour",
                "5h",
                Some(5 * 3600),
                "5-hour limit",
                "5 小时额度",
            ),
            (
                "weekly",
                "Weekly",
                Some(7 * 86_400),
                "Weekly limit",
                "每周额度",
            ),
            ("monthly", "Monthly", None, "Monthly limit", "每月额度"),
        ] {
            let window = &limits[key];
            let Some(used) = window["usedPercent"].as_f64().filter(|n| n.is_finite()) else {
                continue;
            };
            let remaining = window["secondsRemaining"]
                .as_f64()
                .filter(|n| n.is_finite());
            let end = window_end(&window["windowEnd"]);
            let reset = remaining
                .filter(|seconds| *seconds > 0.0)
                .map(|seconds| now + (seconds * 1000.0) as i64)
                .or_else(|| end.filter(|end| *end > now));
            // Factory leaves a short window's last share after it ended; the
            // web app shows it reset.
            let used = if reset.is_none() && end.is_some() && remaining.is_none() {
                0.0
            } else {
                used.clamp(0.0, 100.0)
            };
            let name = format!(
                "{}{}",
                l10n::text(prefix_zh, prefix_en),
                l10n::text(name_zh, name_en)
            );
            let short = if pool == "core" {
                format!("Core {short}")
            } else {
                short.to_owned()
            };
            rows.push(json!({
                "id": format!("{id}/droid:{pool}:{key}"), "vendor": "droid", "accountId": id,
                "account": null, "name": name, "shortLabel": short, "usedPercent": used,
                "resetAt": reset.and_then(chrono::DateTime::from_timestamp_millis).map(|at| at.to_rfc3339()),
                "windowDurationSeconds": seconds, "allModels": pool == "standard", "status": status(used),
            }));
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    fn limits() -> Value {
        serde_json::from_str(include_str!(
            "../../tests/fixtures/droid/macos/billing-limits.json"
        ))
        .unwrap()
    }

    #[test]
    fn an_api_key_reads_each_window_of_both_pools() {
        let host = FakeHost::new("/u")
            .env(KEY, " fk-test-key ")
            .answer(LIMITS, Ok(limits()));
        let reading = read(&host).unwrap();
        let id = account_id("droid", &["fk-test-key"]);
        assert_eq!(reading.account.unwrap()["id"], id.as_str());
        let rows = reading
            .quotas
            .iter()
            .map(|row| {
                (
                    row["id"]
                        .as_str()
                        .unwrap()
                        .rsplit('/')
                        .next()
                        .unwrap()
                        .to_owned(),
                    row["usedPercent"].as_f64().unwrap(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rows,
            [
                ("droid:standard:fiveHour".to_owned(), 42.5),
                ("droid:standard:weekly".to_owned(), 18.0),
                // Its end passed: reset, as the web app shows it.
                ("droid:standard:monthly".to_owned(), 0.0),
                ("droid:core:fiveHour".to_owned(), 100.0),
                ("droid:core:weekly".to_owned(), 7.0),
                ("droid:core:monthly".to_owned(), 3.0),
            ]
        );
        let five = &reading.quotas[0];
        assert_eq!(five["shortLabel"], "5h");
        assert_eq!(five["windowDurationSeconds"], 18_000);
        assert_eq!(five["allModels"], true);
        assert!(
            five["resetAt"].is_string(),
            "secondsRemaining sets the reset"
        );
        let weekly = &reading.quotas[1];
        assert_eq!(weekly["resetAt"], "2099-01-08T00:00:00+00:00");
        assert!(reading.quotas[2]["resetAt"].is_null());
        assert_eq!(reading.quotas[3]["shortLabel"], "Core 5h");
        assert_eq!(reading.quotas[3]["status"], "critical");
        assert_eq!(reading.quotas[3]["allModels"], false);
        assert_eq!(reading.billing.len(), 1);
        assert_eq!(reading.billing[0]["amount"], 12.5);
        assert_eq!(reading.billing[0]["currency"], "USD");
        // The key goes only to Factory, only in the Authorization header.
        assert_eq!(
            *host.asked.borrow(),
            [(LIMITS.to_owned(), Some("Bearer fk-test-key".to_owned()))]
        );
    }

    #[test]
    fn the_key_can_come_from_the_factory_dotenv() {
        let text = "# Factory\nOTHER=1\nexport FACTORY_API_KEY=\"fk-from-file\"\n";
        assert_eq!(dotenv_key(text).as_deref(), Some("fk-from-file"));
        assert_eq!(dotenv_key("FACTORY_API_KEY=\n"), None);
        assert_eq!(dotenv_key("FACTORY_API_KEY_OLD=x\n"), None);
        let host = FakeHost::new("/u")
            .file("/u/.factory/.env", text)
            .answer(LIMITS, Ok(limits()));
        assert_eq!(read(&host).unwrap().quotas.len(), 6);
        assert_eq!(
            host.asked.borrow()[0].1.as_deref(),
            Some("Bearer fk-from-file")
        );
    }

    #[test]
    fn no_key_is_a_notice_and_a_refused_key_a_sign_out() {
        let reading = read(&FakeHost::new("/u")).unwrap();
        assert!(reading.quotas.is_empty());
        assert_eq!(
            reading.notice.as_deref(),
            Some("Set FACTORY_API_KEY to read Factory Droid's usage limits")
        );
        let refused = FakeHost::new("/u")
            .env(KEY, "fk-old")
            .answer(LIMITS, Err(HttpFailure::Status(401)));
        assert!(matches!(read(&refused), Err(Unread::SignedOut(_))));
        let offline = FakeHost::new("/u").env(KEY, "fk-k");
        assert!(matches!(read(&offline), Err(Unread::Transient { .. })));
        let busy = FakeHost::new("/u").env(KEY, "fk-k").answer(
            LIMITS,
            Err(HttpFailure::Limited {
                retry_after: Some(30),
            }),
        );
        assert!(matches!(
            read(&busy),
            Err(Unread::Transient {
                retry_after: Some(_),
                ..
            })
        ));
    }

    #[test]
    fn a_plan_without_windows_says_so_and_a_strange_answer_fails() {
        let legacy = FakeHost::new("/u").env(KEY, "fk-k").answer(
            LIMITS,
            Ok(json!({"usesTokenRateLimitsBilling": false, "extraUsageBalanceCents": 0})),
        );
        let reading = read(&legacy).unwrap();
        assert!(reading.quotas.is_empty() && reading.billing.is_empty());
        assert_eq!(
            reading.notice.as_deref(),
            Some("This Factory plan reports no usage windows")
        );
        let strange = FakeHost::new("/u").env(KEY, "fk-k").answer(
            LIMITS,
            Ok(json!({"limits": {"standard": {"fiveHour": {}}}})),
        );
        assert_eq!(
            read(&strange).err().unwrap(),
            "Factory's usage limits have an unsupported format"
        );
    }

    #[test]
    fn window_ends_read_as_dates_seconds_or_milliseconds() {
        assert_eq!(window_end(&json!(1_900_000_000)), Some(1_900_000_000_000));
        assert_eq!(
            window_end(&json!(1_900_000_000_123_i64)),
            Some(1_900_000_000_123)
        );
        assert_eq!(window_end(&json!("1900000000")), Some(1_900_000_000_000));
        assert_eq!(
            window_end(&json!("2099-01-08T00:00:00Z")),
            Some(4_071_513_600_000)
        );
        assert_eq!(window_end(&json!(null)), None);
        assert_eq!(window_end(&json!("soon")), None);
    }
}
