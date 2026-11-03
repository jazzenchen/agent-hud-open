//! Grok CLI's own credits: the plan's share used this period and its
//! on-demand use, from the billing endpoint the CLI itself calls, with the
//! sign-in the CLI keeps in `~/.grok/auth.json` (`GROK_HOME`). It is xAI's
//! account, apart from Grok Bot's quota (`account.rs`), so it is its own
//! provider, under the CLI's id.
//!
//! The CLI's token is never refreshed or written. It lives a few hours and
//! the CLI renews it whenever it runs, so an expired one reads as signed out
//! until the CLI runs again.
use crate::account::{
    account_id, read_json, resolve, status, Host, HttpFailure, HttpRequest, Provider, Reading,
    Unread,
};
use crate::naming::window_names::Period;
use crate::{l10n, Location};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};
use std::time::Duration;

/// The CLI's sign-ins: `{"<issuer>::<id>": {key, expires_at, email,
/// user_id, team_id, …}}`.
const AUTH: Location = Location::home(&["GROK_HOME"], &[".grok"], "auth.json");
/// What the CLI's plan has used this period, in credits.
const BILLING: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
/// The CLI's settings, which name the plan.
const SETTINGS: &str = "https://cli-chat-proxy.grok.com/v1/settings";
/// At most this many sign-ins are read, one after another.
const MOST: usize = 8;

/// Grok CLI's credits. Its token is fresh only after the CLI ran, so the
/// reading follows the CLI's work on this computer.
pub static PROVIDER: Provider = Provider::new("grok-cli", read);

/// One sign-in the CLI keeps.
struct SignIn {
    /// The entry's name in auth.json.
    name: String,
    key: String,
    /// When the token stops working, in milliseconds since 1970.
    expires: Option<i64>,
    email: Option<String>,
    user: Option<String>,
    team: Option<String>,
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
        .filter(|n| n.is_finite())
}

fn date_ms(value: &Value) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value.as_str()?.trim())
        .ok()
        .map(|date| date.timestamp_millis())
}

/// When a token expires: its JWT `exp`, else the entry's `expires_at` (a
/// date, or seconds or milliseconds since 1970).
fn expiry(key: &str, entry: &Value) -> Option<i64> {
    let jwt = key
        .split('.')
        .nth(1)
        .filter(|_| key.split('.').count() == 3)
        .and_then(|claims| URL_SAFE_NO_PAD.decode(claims).ok())
        .and_then(|claims| serde_json::from_slice::<Value>(&claims).ok())
        .and_then(|claims| number(&claims["exp"]))
        .map(|seconds| (seconds * 1000.0) as i64);
    jwt.or_else(|| date_ms(&entry["expires_at"])).or_else(|| {
        number(&entry["expires_at"]).map(|n| {
            if n > 1e10 {
                n as i64
            } else {
                (n * 1000.0) as i64
            }
        })
    })
}

/// Every sign-in in the CLI's auth.json with a token, by entry name.
fn sign_ins(host: &dyn Host) -> Vec<SignIn> {
    let Some(auth) = resolve(host, &AUTH)
        .first()
        .and_then(|path| read_json(host, path))
    else {
        return Vec::new();
    };
    let mut found = auth
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(name, entry)| {
            let key = text(&entry["key"])
                .filter(|key| !key.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n')))?;
            Some(SignIn {
                name: name.clone(),
                expires: expiry(&key, entry),
                key,
                email: text(&entry["email"]),
                user: text(&entry["user_id"]).or_else(|| text(&entry["principal_id"])),
                team: text(&entry["team_id"]),
            })
        })
        .collect::<Vec<_>>();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

fn signed_out() -> Unread {
    Unread::SignedOut(l10n::text(
        "登录 Grok CLI 后才能读取它的额度",
        "Sign in to Grok CLI to read its credits",
    ))
}

fn expired() -> Unread {
    Unread::SignedOut(l10n::text(
        "Grok CLI 的登录已过期，运行一次 grok 即可续期",
        "Grok CLI's sign-in has expired; running grok renews it",
    ))
}

fn unsupported() -> Unread {
    Unread::Failed(l10n::text(
        "Grok CLI 的额度格式无法识别",
        "Grok CLI's credits have an unsupported format",
    ))
}

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let sign_ins = sign_ins(host);
    if sign_ins.is_empty() {
        return Err(signed_out());
    }
    let now = chrono::Utc::now().timestamp_millis();
    let fresh = sign_ins
        .iter()
        .filter(|sign_in| sign_in.expires.is_none_or(|at| at > now + 60_000))
        .take(MOST)
        .collect::<Vec<_>>();
    if fresh.is_empty() {
        return Err(expired());
    }
    let mut reading = Reading::empty("grok-cli");
    let mut failures = Vec::new();
    for sign_in in fresh {
        if host.cancelled() {
            return Err(l10n::text("账户读取已停止", "Account reading was stopped").into());
        }
        match credits(host, sign_in) {
            Ok((quotas, account)) => {
                reading.quotas.extend(quotas);
                if reading.account.is_none() {
                    reading.account = Some(account);
                } else {
                    reading.additional_accounts.push(account);
                }
            }
            Err(unread) => failures.push(unread),
        }
    }
    let failed = Unread::combined(failures);
    if reading.account.is_none() {
        return Err(failed.unwrap_or_else(unsupported));
    }
    reading.notice = failed.map(|unread| unread.reason().to_owned());
    Ok(reading)
}

/// A request to the CLI's own service, as the CLI sends it.
fn get(host: &dyn Host, url: &str, key: &str) -> Result<Value, HttpFailure> {
    let authorization = format!("Bearer {key}");
    host.http_json(&HttpRequest {
        url,
        secrets: &[("Authorization", &authorization)],
        headers: &[("X-XAI-Token-Auth", "xai-grok-cli")],
        body: None,
        timeout: Duration::from_secs(10),
    })
}

/// One sign-in's windows and account.
fn credits(host: &dyn Host, sign_in: &SignIn) -> Result<(Vec<Value>, Value), Unread> {
    let body = get(host, BILLING, &sign_in.key).map_err(|failure| match failure {
        // The service no longer takes the CLI's token.
        HttpFailure::Status(401) => Unread::SignedOut(l10n::text(
            "Grok CLI 的登录已失效，运行 grok 重新登录",
            "Grok CLI's sign-in is no longer accepted; run grok to sign in again",
        )),
        HttpFailure::BadHeader => Unread::Failed(l10n::text(
            "Grok CLI 的登录凭据无法用于请求",
            "Grok CLI's sign-in cannot be used in a request",
        )),
        HttpFailure::Unreachable => failure.unread_as(l10n::text(
            "无法连接 Grok CLI 的额度服务",
            "Grok CLI's credits could not be reached",
        )),
        other => other.unread(),
    })?;
    let user = sign_in.user.as_deref().unwrap_or(&sign_in.name);
    let id = account_id("grok-cli", &[user, sign_in.team.as_deref().unwrap_or("")]);
    let quotas = parse(&body, &id, sign_in.email.as_deref()).ok_or_else(unsupported)?;
    // The plan's name is worth a second request, not a failed reading.
    let plan = get(host, SETTINGS, &sign_in.key)
        .ok()
        .and_then(|settings| text(&settings["subscription_tier_display"]));
    let account = json!({"id":id,"provider":"grok-cli","label":sign_in.email,"plan":plan});
    Ok((quotas, account))
}

/// A proto-JSON number, bare or wrapped as `{"val": n}`.
fn wrapped(value: &Value) -> Option<f64> {
    number(&value["val"]).or_else(|| number(value))
}

/// The windows of a `billing?format=credits` answer: the share of the
/// period's credits used (`config.creditUsagePercent`, left out when it is
/// zero), over `config.currentPeriod`, and the on-demand credits used of
/// their cap when both are reported. None when the period is not there.
fn parse(body: &Value, id: &str, email: Option<&str>) -> Option<Vec<Value>> {
    let config = &body["config"];
    let start = date_ms(&config["currentPeriod"]["start"])?;
    let end = date_ms(&config["currentPeriod"]["end"]).filter(|end| *end > start)?;
    let used = match &config["creditUsagePercent"] {
        Value::Null => 0.0,
        value => number(value)?,
    }
    .clamp(0.0, 100.0);
    let reset = chrono::DateTime::from_timestamp_millis(end).map(|at| at.to_rfc3339());
    let seconds = (end - start) / 1000;
    let period = Period::from_seconds(Some(seconds as f64));
    let (name, short) = match period {
        Some(Period::Week) => ("Weekly usage limit".to_owned(), "Weekly".to_owned()),
        Some(Period::Month) => ("Monthly usage limit".to_owned(), "Monthly".to_owned()),
        Some(period) => ("Usage limit".to_owned(), period.short_name()),
        None => ("Usage limit".to_owned(), "Usage".to_owned()),
    };
    let mut rows = vec![json!({
        "id": format!("{id}/grok-cli:credits"), "vendor": "grok-cli", "accountId": id, "account": email,
        "name": name, "shortLabel": short, "usedPercent": used, "resetAt": reset,
        "windowDurationSeconds": seconds, "allModels": true, "status": status(used),
    })];
    let cap = wrapped(&config["onDemandCap"]).filter(|cap| *cap > 0.0);
    if let (Some(cap), Some(spent)) = (cap, wrapped(&config["onDemandUsed"])) {
        let spent = spent.max(0.0);
        let used = (spent / cap * 100.0).clamp(0.0, 100.0);
        rows.push(json!({
            "id": format!("{id}/grok-cli:on-demand"), "vendor": "grok-cli", "accountId": id, "account": email,
            "name": "On-demand usage", "shortLabel": "On-demand", "usedPercent": used, "resetAt": reset,
            "allModels": true, "status": status(used),
            "nativeUsage": {"used": spent, "capacity": cap, "unit": "credits"},
        }));
    }
    Some(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    const AUTH_FILE: &str = "/u/.grok/auth.json";

    /// A JWT whose only claim that matters is `exp`, `hours` from now.
    fn token(hours: i64) -> String {
        let exp = chrono::Utc::now().timestamp() + hours * 3600;
        let claims = URL_SAFE_NO_PAD.encode(json!({"exp": exp, "sub": "u1"}).to_string());
        format!("eyJhbGciOiJSUzI1NiJ9.{claims}.c2lnbmF0dXJl")
    }

    fn auth(key: &str) -> String {
        json!({"https://auth.x.ai::u1": {
            "key": key, "refresh_token": "never-sent", "email": "dev@example.com",
            "user_id": "u1", "team_id": "t1", "auth_mode": "oidc",
            "expires_at": "2099-01-01T00:00:00Z",
        }})
        .to_string()
    }

    fn billing() -> Value {
        json!({"config": {
            "creditUsagePercent": 37.5,
            "currentPeriod": {"type": "WEEKLY", "start": "2026-11-01T00:00:00Z", "end": "2026-11-08T00:00:00.000Z"},
            "onDemandCap": {"val": "2500"},
            "onDemandUsed": {"val": 500},
        }})
    }

    #[test]
    fn the_clis_sign_in_reads_its_credits_and_plan() {
        let key = token(5);
        let host = FakeHost::new("/u")
            .file(AUTH_FILE, &auth(&key))
            .answer(BILLING, Ok(billing()))
            .answer(
                SETTINGS,
                Ok(json!({"subscription_tier_display": "SuperGrok"})),
            );
        let reading = read(&host).unwrap();
        let id = account_id("grok-cli", &["u1", "t1"]);
        let account = reading.account.unwrap();
        assert_eq!(account["id"], id.as_str());
        assert_eq!(account["label"], "dev@example.com");
        assert_eq!(account["plan"], "SuperGrok");
        assert_eq!(reading.quotas.len(), 2);
        let credits = &reading.quotas[0];
        assert_eq!(credits["id"], format!("{id}/grok-cli:credits"));
        assert_eq!(credits["vendor"], "grok-cli");
        assert_eq!(credits["usedPercent"], 37.5);
        assert_eq!(credits["name"], "Weekly usage limit");
        assert_eq!(credits["windowDurationSeconds"], 604_800);
        assert_eq!(credits["resetAt"], "2026-11-08T00:00:00+00:00");
        let on_demand = &reading.quotas[1];
        assert_eq!(on_demand["usedPercent"], 20.0);
        assert_eq!(on_demand["nativeUsage"]["capacity"], 2500.0);
        assert_eq!(on_demand["nativeUsage"]["unit"], "credits");

        let asked = host.asked.borrow();
        assert_eq!(asked.len(), 2);
        assert_eq!(asked[0], (BILLING.into(), Some(format!("Bearer {key}"))));
        assert_eq!(
            host.headers.borrow()[0],
            [("X-XAI-Token-Auth".into(), "xai-grok-cli".into())]
        );
        assert!(
            host.sent
                .borrow()
                .iter()
                .all(|(secrets, body)| body.is_none()
                    && secrets
                        .iter()
                        .all(|(_, value)| !value.contains("never-sent"))),
            "the refresh token never leaves the file"
        );
        assert!(host.own.borrow().is_empty(), "nothing is written");
    }

    #[test]
    fn a_zero_share_is_left_out_and_without_a_cap_there_is_no_on_demand_row() {
        let host = FakeHost::new("/u")
            .file(AUTH_FILE, &auth(&token(5)))
            .answer(
                BILLING,
                Ok(json!({"config": {"currentPeriod": {"start": "2026-10-01T00:00:00Z", "end": "2026-10-31T00:00:00Z"}}})),
            );
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 1);
        assert_eq!(reading.quotas[0]["usedPercent"], 0.0);
        assert_eq!(reading.quotas[0]["name"], "Monthly usage limit");
        assert!(
            reading.account.unwrap()["plan"].is_null(),
            "the plan is best effort"
        );
    }

    #[test]
    fn an_expired_or_missing_sign_in_is_signed_out_without_asking() {
        let host = FakeHost::new("/u").file(AUTH_FILE, &auth(&token(-1)));
        assert!(matches!(read(&host), Err(Unread::SignedOut(_))));
        assert_eq!(
            read(&host).err().unwrap(),
            "Grok CLI's sign-in has expired; running grok renews it"
        );
        assert!(host.asked.borrow().is_empty());
        assert!(host.own.borrow().is_empty(), "nothing is refreshed");

        // Without a JWT the entry's own expiry counts.
        let past = json!({"a::1": {"key": "opaque", "expires_at": "2020-01-01T00:00:00Z"}});
        let host = FakeHost::new("/u").file(AUTH_FILE, &past.to_string());
        assert_eq!(
            read(&host).err().unwrap(),
            "Grok CLI's sign-in has expired; running grok renews it"
        );

        let host = FakeHost::new("/u");
        assert!(matches!(read(&host), Err(Unread::SignedOut(_))));
        assert!(host.asked.borrow().is_empty());
    }

    #[test]
    fn grok_home_moves_the_sign_in() {
        let home = "/u";
        let grok = format!("{home}/grok-alt");
        let host = FakeHost::new(home)
            .env("GROK_HOME", &grok)
            .file(AUTH_FILE, &auth(&token(-1)))
            .file(&format!("{grok}/auth.json"), &auth(&token(5)))
            .answer(BILLING, Ok(billing()));
        assert_eq!(read(&host).unwrap().quotas.len(), 2);
    }

    #[test]
    fn failures_say_whose_they_are() {
        let host = |answer| {
            FakeHost::new("/u")
                .file(AUTH_FILE, &auth(&token(5)))
                .answer(BILLING, answer)
        };
        assert!(matches!(
            read(&host(Err(HttpFailure::Status(401)))),
            Err(Unread::SignedOut(_))
        ));
        assert!(matches!(
            read(&host(Err(HttpFailure::Unreachable))),
            Err(Unread::Transient { .. })
        ));
        assert_eq!(
            read(&host(Err(HttpFailure::Unreachable))).err().unwrap(),
            "Grok CLI's credits could not be reached"
        );
        assert!(matches!(
            read(&host(Ok(json!({"config": {}})))),
            Err(Unread::Failed(_))
        ));
        assert!(matches!(
            read(&host(Ok(json!({"config": {"creditUsagePercent": "lots",
                "currentPeriod": {"start": "2026-11-01T00:00:00Z", "end": "2026-11-08T00:00:00Z"}}})))),
            Err(Unread::Failed(_))
        ));
    }

    #[test]
    fn the_cli_is_its_own_provider() {
        let provider = crate::account_provider("grok-cli").unwrap();
        assert!(std::ptr::eq(provider, &PROVIDER));
        assert!(provider.local_work);
        assert_eq!(provider.sources(), ["grok-cli"]);
    }
}
