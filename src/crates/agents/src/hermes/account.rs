//! Nous Portal's credits, for Hermes signed in to it: the share of the
//! month's subscription credits used and the top-up balance, from the account
//! endpoint Hermes itself reads (`GET /api/oauth/account`), with the access
//! token Hermes keeps in `auth.json`.
//!
//! The token is only read. Nous rotates its refresh tokens and revokes the
//! whole sign-in when one is used twice, so a refresh here would sign Hermes
//! out; the refresh token never leaves the file. The access token lives a
//! short while and Hermes renews it when it runs, so an expired one is never
//! sent: it is a reading that got no answer (`Unread::Transient`), which
//! leaves the last credits as they were for a while and then shows them
//! stale, saying that Hermes renews its sign-in when it next runs. The token
//! goes only to a `nousresearch.com` address over HTTPS.
//!
//! The file shapes and the trust rule for the portal's address follow
//! CodexBar's Nous reader (MIT, `NousSettingsReader.swift` and `nous.ts`),
//! checked against Hermes 0.19's `hermes_cli/auth.py` and
//! `hermes_cli/nous_account.py`.
use crate::account::{
    account_id, read_json, resolve, status, Host, HttpFailure, HttpRequest, Provider, Reading,
    Unread,
};
use crate::naming::window_names::Period;
use crate::{l10n, Location};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};
use std::time::Duration;

/// Hermes's sign-ins: `{"providers": {"nous": {access_token, expires_at,
/// portal_base_url, …}}}`, or a credential pool `{"credential_pool":
/// {"nous": [...]}}`.
const AUTH: Location = Location::home(&["HERMES_HOME"], &[".hermes"], "auth.json");
/// The copy every profile shares.
const SHARED: Location = Location::home(&["HERMES_HOME"], &[".hermes"], "shared/nous_auth.json");
const PORTAL: &str = "https://portal.nousresearch.com";
/// A token this close to its expiry is not sent.
const SKEW_MS: i64 = 60_000;

/// Nous Portal's credits, under Hermes. Its token is fresh only after Hermes
/// ran, so the reading follows Hermes's work on this computer.
pub static PROVIDER: Provider = Provider::new("hermes", read);

/// Hermes's sign-in to Nous Portal.
struct SignIn {
    token: String,
    portal: String,
    /// When the token stops working, in milliseconds since 1970.
    expires: Option<i64>,
}

fn text(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
        .filter(|n| n.is_finite())
}

fn date_ms(value: &Value) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(text(value)?)
        .ok()
        .map(|date| date.timestamp_millis())
}

/// A JWT's claims, read without checking its signature: only to know when it
/// expires and whose it is.
fn claims(token: &str) -> Option<Value> {
    let mut parts = token.split('.');
    let (_, claims, _) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims.trim_end_matches('=')).ok()?).ok()
}

/// The portal's address, when the stored one is `https` on
/// `nousresearch.com` or one of its subdomains and names no path, query or
/// user; else the default portal, so a stale or tampered address never gets
/// the token.
fn portal(stored: Option<&str>) -> String {
    let trusted = stored
        .and_then(|address| url::Url::parse(address.trim_end_matches('/')).ok())
        .filter(|url| {
            url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && matches!(url.path(), "" | "/")
                && url.host_str().is_some_and(|host| {
                    let host = host.to_ascii_lowercase();
                    host == "nousresearch.com" || host.ends_with(".nousresearch.com")
                })
        });
    match trusted {
        Some(url) => url.as_str().trim_end_matches('/').to_owned(),
        None => PORTAL.to_owned(),
    }
}

fn from_state(state: &Value) -> Option<SignIn> {
    let token = text(&state["access_token"])
        .filter(|token| !token.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n')))?;
    let expires = date_ms(&state["expires_at"]).or_else(|| {
        claims(token)
            .and_then(|claims| number(&claims["exp"]))
            .map(|seconds| (seconds * 1000.0) as i64)
    });
    Some(SignIn {
        token: token.to_owned(),
        portal: portal(text(&state["portal_base_url"])),
        expires,
    })
}

/// The sign-in a Hermes auth file holds, in the order Hermes takes them: its
/// `providers.nous` state, else the pool entry with the newest agent key,
/// then the newest token, then the lowest priority number; else a bare state
/// (the shared copy). None when the file names no Nous sign-in.
fn sign_in(auth: &Value) -> Option<SignIn> {
    if let Some(found) = from_state(&auth["providers"]["nous"]) {
        return Some(found);
    }
    let pooled = auth["credential_pool"]["nous"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let found = from_state(entry)?;
            let agent = date_ms(&entry["agent_key_expires_at"]).unwrap_or(0);
            let priority = entry["priority"].as_i64().unwrap_or(0);
            Some(((agent, found.expires.unwrap_or(0), -priority), found))
        })
        .max_by_key(|(order, _)| *order)
        .map(|(_, found)| found);
    pooled.or_else(|| from_state(auth))
}

/// Not read now: Hermes renews the token itself, and nothing here can.
fn expired() -> Unread {
    Unread::transient(l10n::text(
        "Hermes 下次运行时会续期 Nous Portal 的登录，之后更新额度",
        "Hermes renews its Nous Portal sign-in the next time it runs; the credits update then",
    ))
}

fn unsupported() -> Unread {
    Unread::Failed(l10n::text(
        "Nous Portal 的额度格式无法识别",
        "Nous Portal's credits have an unsupported format",
    ))
}

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut stale = false;
    for location in [AUTH, SHARED] {
        for path in resolve(host, &location) {
            let Some(found) = read_json(host, &path).as_ref().and_then(sign_in) else {
                continue;
            };
            if found.expires.is_some_and(|at| at <= now + SKEW_MS) {
                stale = true;
                continue;
            }
            return credits(host, &found);
        }
    }
    if stale {
        return Err(expired());
    }
    // Hermes is not signed in to Nous Portal: it uses another provider, so
    // there is nothing to show and nothing to sign in to.
    Ok(Reading::empty("hermes"))
}

fn credits(host: &dyn Host, sign_in: &SignIn) -> Result<Reading, Unread> {
    let url = format!("{}/api/oauth/account", sign_in.portal);
    let authorization = format!("Bearer {}", sign_in.token);
    let body = host
        .http_json(&HttpRequest {
            url: &url,
            secrets: &[("Authorization", &authorization)],
            headers: &[("Accept", "application/json")],
            body: None,
            timeout: Duration::from_secs(15),
        })
        .map_err(|failure| match failure {
            HttpFailure::Status(401) => Unread::SignedOut(l10n::text(
                "Nous Portal 不再接受 Hermes 的登录，运行 hermes 重新登录",
                "Nous Portal no longer accepts Hermes's sign-in; run hermes to sign in again",
            )),
            HttpFailure::Status(403) => Unread::Failed(l10n::text(
                "Nous Portal 拒绝显示这个账户",
                "Nous Portal refused to show this account",
            )),
            HttpFailure::BadHeader => Unread::Failed(l10n::text(
                "Hermes 的 Nous Portal 登录凭据无法用于请求",
                "Hermes's Nous Portal sign-in cannot be used in a request",
            )),
            HttpFailure::Unreachable => failure.unread_as(l10n::text(
                "无法连接 Nous Portal",
                "Nous Portal could not be reached",
            )),
            other => other.unread(),
        })?;
    let user = claims(&sign_in.token)
        .and_then(|claims| text(&claims["sub"]).map(str::to_owned))
        .unwrap_or_default();
    parse(&body, &user)
}

/// The reading an account answer gives: the month's subscription credits as
/// a quota (used = the grant less what remains), and the top-up credits as a
/// balance in US dollars, which is what a Nous credit is.
fn parse(body: &Value, user: &str) -> Result<Reading, Unread> {
    if !body.is_object() || !body["error"].is_null() {
        return Err(unsupported());
    }
    let subscription = &body["subscription"];
    let access = &body["paid_service_access"];
    let monthly = number(&subscription["monthly_credits"]);
    if monthly.is_some_and(|monthly| monthly < 0.0) {
        return Err(unsupported());
    }
    let remaining = number(&subscription["credits_remaining"])
        .or_else(|| number(&access["subscription_credits_remaining"]));
    let purchased = number(&body["purchased_credits_remaining"])
        .or_else(|| number(&access["purchased_credits_remaining"]));
    let usable = number(&access["total_usable_credits"]);
    if [monthly, remaining, purchased, usable]
        .iter()
        .all(Option::is_none)
    {
        return Err(unsupported());
    }
    let reset = match &subscription["current_period_end"] {
        Value::Null => None,
        value => Some(
            date_ms(value)
                .and_then(chrono::DateTime::from_timestamp_millis)
                .ok_or_else(unsupported)?
                .to_rfc3339(),
        ),
    };
    let email = text(&body["user"]["email"]);
    let organisation = text(&body["organisation"]["id"])
        .or_else(|| text(&access["organisation_id"]))
        .unwrap_or("");
    let id = account_id("hermes", &[organisation, email.unwrap_or(user)]);
    let mut reading = Reading::empty("hermes");
    if let (Some(monthly), Some(remaining)) = (monthly.filter(|m| *m > 0.0), remaining) {
        let spent = (monthly - remaining.max(0.0)).max(0.0);
        let used = (spent / monthly * 100.0).clamp(0.0, 100.0);
        reading.quotas.push(json!({
            "id": format!("{id}/hermes:credits"), "vendor": "hermes", "accountId": id, "account": email,
            "name": "Monthly credits", "shortLabel": Period::Month.short_name(), "usedPercent": used,
            "resetAt": reset, "allModels": true, "status": status(used),
            "nativeUsage": {"used": spent, "capacity": monthly, "unit": "credits"},
        }));
    }
    let balance = |key: &str, label: String, amount: f64| {
        json!({
            "id": format!("{id}/hermes:{key}"), "vendor": "hermes", "accountId": id, "account": email,
            "label": label, "amount": amount, "currency": "USD",
        })
    };
    if let Some(purchased) = purchased {
        reading.billing.push(balance(
            "top-up",
            l10n::text("充值额度", "Top-up credits"),
            purchased.max(0.0),
        ));
    } else if let (true, Some(usable)) = (reading.quotas.is_empty(), usable) {
        reading.billing.push(balance(
            "usable",
            l10n::text("可用额度", "Usable credits"),
            usable.max(0.0),
        ));
    }
    let plan = text(&subscription["plan"]);
    let label = email.or_else(|| text(&body["organisation"]["name"]));
    reading.account = Some(json!({"id": id, "provider": "hermes", "label": label, "plan": plan}));
    Ok(reading)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    const AUTH_FILE: &str = "/u/.hermes/auth.json";
    const ACCOUNT: &str = "https://portal.nousresearch.com/api/oauth/account";

    /// A JWT whose claims that matter are `exp`, `hours` from now, and `sub`.
    fn token(hours: i64) -> String {
        let exp = chrono::Utc::now().timestamp() + hours * 3600;
        let claims = URL_SAFE_NO_PAD.encode(json!({"exp": exp, "sub": "user-1"}).to_string());
        format!("eyJhbGciOiJSUzI1NiJ9.{claims}.c2lnbmF0dXJl")
    }

    /// Hermes's auth file, as `hermes login` leaves it, with `token` and the
    /// expiry its claims name.
    fn auth(token: &str, portal: &str) -> String {
        let exp = number(&claims(token).unwrap()["exp"]).unwrap() as i64;
        let expires = chrono::DateTime::from_timestamp(exp, 0)
            .unwrap()
            .to_rfc3339();
        include_str!("../../tests/fixtures/hermes/macos/auth.json")
            .replace("ACCESS_TOKEN", token)
            .replace("EXPIRES_AT", &expires)
            .replace("PORTAL", portal)
    }

    fn account() -> Value {
        serde_json::from_str(include_str!(
            "../../tests/fixtures/hermes/macos/oauth-account.json"
        ))
        .unwrap()
    }

    #[test]
    fn the_month_s_credits_and_the_top_up_balance() {
        let key = token(1);
        let host = FakeHost::new("/u")
            .file(AUTH_FILE, &auth(&key, "https://portal.nousresearch.com"))
            .answer(ACCOUNT, Ok(account()));
        let reading = read(&host).unwrap();
        let id = account_id("hermes", &["org-1", "dev@example.com"]);
        let row = &reading.quotas[0];
        assert_eq!(row["id"], format!("{id}/hermes:credits"));
        assert_eq!(row["vendor"], "hermes");
        assert_eq!(row["usedPercent"], 35.0);
        assert_eq!(row["shortLabel"], "Monthly");
        assert_eq!(row["resetAt"], "2026-12-01T00:00:00+00:00");
        assert_eq!(row["nativeUsage"]["capacity"], 20.0);
        assert_eq!(row["nativeUsage"]["used"], 7.0);
        assert_eq!(reading.billing.len(), 1);
        assert_eq!(reading.billing[0]["amount"], 12.5);
        assert_eq!(reading.billing[0]["currency"], "USD");
        let account = reading.account.unwrap();
        assert_eq!(account["plan"], "Plus");
        assert_eq!(account["label"], "dev@example.com");

        let asked = host.asked.borrow();
        assert_eq!(
            asked.as_slice(),
            [(ACCOUNT.to_owned(), Some(format!("Bearer {key}")))]
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
    fn the_token_goes_only_to_nous_research() {
        for stored in [
            "https://evil.example.com",
            "http://portal.nousresearch.com",
            "https://portal.nousresearch.com.evil.example",
            "https://user@portal.nousresearch.com",
            "https://portal.nousresearch.com/api?x=1",
        ] {
            let host = FakeHost::new("/u")
                .file(AUTH_FILE, &auth(&token(1), stored))
                .answer(ACCOUNT, Ok(account()));
            read(&host).unwrap();
            assert_eq!(host.asked.borrow()[0].0, ACCOUNT, "{stored}");
        }
        let staging = "https://staging.nousresearch.com/";
        let host = FakeHost::new("/u")
            .file(AUTH_FILE, &auth(&token(1), staging))
            .answer(
                "https://staging.nousresearch.com/api/oauth/account",
                Ok(account()),
            );
        assert_eq!(read(&host).unwrap().quotas.len(), 1);
    }

    #[test]
    fn an_expired_token_is_never_sent_and_no_sign_in_shows_nothing() {
        let host = FakeHost::new("/u").file(
            AUTH_FILE,
            &auth(&token(-1), "https://portal.nousresearch.com"),
        );
        let failed = read(&host).err().unwrap();
        assert!(matches!(
            failed,
            Unread::Transient {
                retry_after: None,
                ..
            }
        ));
        assert_eq!(
            failed,
            "Hermes renews its Nous Portal sign-in the next time it runs; the credits update then"
        );
        assert!(host.asked.borrow().is_empty());
        assert!(host.own.borrow().is_empty(), "nothing is refreshed");

        // Signed in to another provider only, or not at all.
        let other = json!({"version": 1, "providers": {"openrouter": {"api_key": "k"}}});
        for host in [
            FakeHost::new("/u").file(AUTH_FILE, &other.to_string()),
            FakeHost::new("/u"),
        ] {
            let reading = read(&host).unwrap();
            assert!(reading.quotas.is_empty() && reading.account.is_none());
            assert!(host.asked.borrow().is_empty());
        }
    }

    #[test]
    fn the_pool_and_the_shared_copy_stand_in() {
        let key = token(2);
        let pool = json!({"credential_pool": {"nous": [
            {"access_token": token(1), "agent_key_expires_at": "2026-01-01T00:00:00Z", "priority": 0},
            {"access_token": key, "agent_key_expires_at": "2027-01-01T00:00:00Z", "priority": 1},
        ]}});
        let host = FakeHost::new("/u")
            .file(AUTH_FILE, &pool.to_string())
            .answer(ACCOUNT, Ok(account()));
        read(&host).unwrap();
        assert_eq!(host.asked.borrow()[0].1, Some(format!("Bearer {key}")));

        let shared =
            json!({"_schema": 1, "access_token": key, "expires_at": "2099-01-01T00:00:00Z"});
        let host = FakeHost::new("/u")
            .file("/u/.hermes/shared/nous_auth.json", &shared.to_string())
            .answer(ACCOUNT, Ok(account()));
        assert_eq!(read(&host).unwrap().quotas.len(), 1);
    }

    #[test]
    fn failures_say_whose_they_are() {
        let host = |answer| {
            FakeHost::new("/u")
                .file(
                    AUTH_FILE,
                    &auth(&token(1), "https://portal.nousresearch.com"),
                )
                .answer(ACCOUNT, answer)
        };
        assert!(matches!(
            read(&host(Err(HttpFailure::Status(401)))),
            Err(Unread::SignedOut(_))
        ));
        assert!(matches!(
            read(&host(Err(HttpFailure::Unreachable))),
            Err(Unread::Transient { .. })
        ));
        assert!(matches!(
            read(&host(Err(HttpFailure::Status(503)))),
            Err(Unread::Transient { .. })
        ));
        for body in [
            json!({"error": "account_response_error"}),
            json!({"user": {"email": "dev@example.com"}}),
            json!({"subscription": {"monthly_credits": "lots"}}),
            json!({"subscription": {"monthly_credits": 20, "credits_remaining": 1, "current_period_end": "soon"}}),
        ] {
            assert!(
                matches!(read(&host(Ok(body.clone()))), Err(Unread::Failed(_))),
                "{body}"
            );
        }
    }

    #[test]
    fn a_free_account_shows_its_usable_credits() {
        let host = FakeHost::new("/u")
            .file(AUTH_FILE, &auth(&token(1), "https://portal.nousresearch.com"))
            .answer(
                ACCOUNT,
                Ok(json!({"paid_service_access": {"allowed": false, "total_usable_credits": "0.75"}})),
            );
        let reading = read(&host).unwrap();
        assert!(reading.quotas.is_empty());
        assert_eq!(reading.billing[0]["amount"], 0.75);
        assert_eq!(
            reading.account.unwrap()["id"],
            account_id("hermes", &["", "user-1"])
        );
    }

    #[test]
    fn hermes_is_its_own_provider() {
        let provider = crate::account_provider("hermes").unwrap();
        assert!(std::ptr::eq(provider, &PROVIDER));
        assert_eq!(provider.sources(), ["hermes"]);
    }
}
