//! ClinePass: the 5-hour, weekly and monthly limits of Cline's subscription,
//! read with the sign-in Cline keeps. The request and its answer follow
//! CodexBar's ClinePass reader (MIT, `897bff3`); where the sign-in is kept
//! and how it is sent follow Cline's own SDK (Apache-2.0, `7a7878c`).
//!
//! Cline keeps its sign-in in `providers.json` (`providers.cline.settings`):
//! a browser sign-in's short-lived access token, sent as `workos:<token>`,
//! or an API key, as `CLINE_API_KEY` also gives one. The token is never
//! refreshed or written: Cline renews it whenever it runs, so an expired one
//! keeps the last reading until Cline runs again.
use crate::account::{
    account_id, read_json, resolve, status, Host, HttpFailure, HttpRequest, Provider, Reading,
    Unread,
};
use crate::naming::window_names::Period;
use crate::{l10n::tr, Location};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};
use std::time::Duration;

/// Cline's provider settings, under its data folder.
pub const PROVIDERS: Location =
    Location::home(&["CLINE_DIR"], &[".cline"], "data/settings/providers.json");
/// The same, where `CLINE_DATA_DIR` moves the data folder.
pub const PROVIDERS_MOVED: Location =
    Location::named_by(&["CLINE_DATA_DIR"], "settings/providers.json");
/// The plan's limits, as Cline's service reports them.
pub const USAGE_LIMITS: &str = "https://api.cline.bot/api/v1/users/me/plan/usage-limits";
/// How Cline marks its browser sign-in's token when it sends it.
const WORKOS: &str = "workos:";

/// ClinePass. Cline's work on this computer spends it, and leaves its token
/// fresh.
pub static PROVIDER: Provider = Provider::new("cline", read);

/// A sign-in to send.
struct SignIn {
    token: String,
    /// When it stops working, in milliseconds since 1970; None for a key.
    expires: Option<i64>,
    account: Option<String>,
    email: Option<String>,
}

/// A credential or name as it can be sent: trimmed, not empty, one line.
fn clean(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty() && !text.bytes().any(|b| matches!(b, 0 | b'\r' | b'\n')))
        .then(|| text.to_owned())
}

fn text(value: &Value) -> Option<String> {
    value.as_str().and_then(clean)
}

/// A JWT's `exp`, in milliseconds.
fn jwt_expiry(token: &str) -> Option<i64> {
    let claims = token
        .split('.')
        .nth(1)
        .filter(|_| token.split('.').count() == 3)?;
    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims).ok()?).ok()?;
    claims["exp"]
        .as_f64()
        .map(|seconds| (seconds * 1000.0) as i64)
}

/// The sign-in Cline would send to its own service, in Cline's order: the
/// browser sign-in, then a saved key, then `CLINE_API_KEY`.
fn sign_in(host: &dyn Host) -> Option<SignIn> {
    let settings = [PROVIDERS_MOVED, PROVIDERS]
        .iter()
        .flat_map(|location| resolve(host, location))
        .find_map(|path| read_json(host, &path))
        .map(|file| file["providers"]["cline"]["settings"].clone())
        .unwrap_or(Value::Null);
    let auth = &settings["auth"];
    let account = text(&auth["accountId"]);
    let email = text(&auth["metadata"]["userInfo"]["email"]);
    if let Some(access) = text(&auth["accessToken"]) {
        let bare = access
            .get(..WORKOS.len())
            .filter(|prefix| prefix.eq_ignore_ascii_case(WORKOS))
            .map_or(access.as_str(), |_| &access[WORKOS.len()..]);
        let expires = auth["expiresAt"]
            .as_f64()
            .filter(|at| at.is_finite() && *at > 0.0)
            .map(|at| at as i64)
            .or_else(|| jwt_expiry(bare));
        return Some(SignIn {
            token: format!("{WORKOS}{bare}"),
            expires,
            account,
            email,
        });
    }
    text(&settings["apiKey"])
        .or_else(|| text(&auth["apiKey"]))
        .or_else(|| host.env("CLINE_API_KEY").and_then(clean))
        .map(|token| SignIn {
            token,
            expires: None,
            account,
            email,
        })
}

fn signed_out() -> Unread {
    Unread::SignedOut(tr!("Sign in to Cline to read ClinePass"))
}

fn unsupported() -> Unread {
    Unread::Failed(tr!("ClinePass's limits have an unsupported format"))
}

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let sign_in = sign_in(host).ok_or_else(signed_out)?;
    let now = chrono::Utc::now().timestamp_millis();
    if sign_in.expires.is_some_and(|at| at <= now + 60_000) {
        return Err(Unread::Failed(tr!(
            "Cline renews its sign-in the next time it runs; ClinePass updates then"
        )));
    }
    let authorization = format!("Bearer {}", sign_in.token);
    let body = host
        .http_json(&HttpRequest {
            url: USAGE_LIMITS,
            secrets: &[("Authorization", &authorization)],
            headers: &[],
            body: None,
            timeout: Duration::from_secs(15),
        })
        .map_err(|failure| match failure {
            // The service no longer takes the sign-in. A 403 may only mean
            // the account has no plan to read, so it is not taken for one.
            HttpFailure::Status(401) => Unread::SignedOut(tr!(
                "Cline's sign-in is no longer accepted; sign in to Cline again"
            )),
            HttpFailure::Status(403 | 404) => {
                Unread::Failed(tr!("This Cline account has no ClinePass limits to read"))
            }
            HttpFailure::BadHeader => {
                Unread::Failed(tr!("Cline's sign-in cannot be used in a request"))
            }
            HttpFailure::Unreachable => {
                failure.unread_as(tr!("ClinePass's limits could not be reached"))
            }
            other => other.unread(),
        })?;
    let id = account_id(
        "cline",
        &[sign_in
            .account
            .as_deref()
            .or(sign_in.email.as_deref())
            .unwrap_or("default")],
    );
    let quotas = parse(&body, &id, sign_in.email.as_deref()).ok_or_else(unsupported)?;
    let mut reading = Reading::empty("cline");
    reading.account = Some(json!({
        "id": id, "provider": "cline", "label": sign_in.email,
        "plan": (!quotas.is_empty()).then_some("ClinePass"),
    }));
    reading.quotas = quotas;
    Ok(reading)
}

/// The windows of `{success, data: {limits: [{type, percentUsed, resetsAt}]}}`:
/// `five_hour`, `weekly` and `monthly`; other kinds are left out. None when
/// the answer is not that shape.
fn parse(body: &Value, id: &str, email: Option<&str>) -> Option<Vec<Value>> {
    if body["success"] != true {
        return None;
    }
    let mut rows = Vec::new();
    for limit in body["data"]["limits"].as_array()? {
        let kind = limit["type"].as_str()?;
        let (seconds, name) = match kind {
            "five_hour" => (5 * 3600, "5-hour limit"),
            "weekly" => (7 * 86_400, "Weekly limit"),
            "monthly" => (30 * 86_400, "Monthly limit"),
            _ => continue,
        };
        let used = limit["percentUsed"]
            .as_f64()
            .filter(|n| n.is_finite())?
            .clamp(0.0, 100.0);
        let reset = match &limit["resetsAt"] {
            Value::Null => None,
            value => Some(
                chrono::DateTime::parse_from_rfc3339(value.as_str()?.trim())
                    .ok()?
                    .to_rfc3339(),
            ),
        };
        let short = Period::from_seconds(Some(seconds as f64)).map(Period::short_name);
        rows.push(json!({
            "id": format!("{id}/cline:{kind}"), "vendor": "cline", "consumerRoutes": ["cline#cline"], "accountId": id, "account": email,
            "name": name, "shortLabel": short, "usedPercent": used, "resetAt": reset,
            "windowDurationSeconds": seconds, "allModels": true, "status": status(used),
        }));
    }
    Some(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    const SETTINGS: &str = "/u/.cline/data/settings/providers.json";

    /// A JWT whose only claim that matters is `exp`, `hours` from now.
    fn token(hours: i64) -> String {
        let exp = chrono::Utc::now().timestamp() + hours * 3600;
        let claims = URL_SAFE_NO_PAD.encode(json!({"exp": exp, "sub": "u1"}).to_string());
        format!("eyJhbGciOiJSUzI1NiJ9.{claims}.c2lnbmF0dXJl")
    }

    /// Cline's `providers.json` with a browser sign-in, as its SDK saves it.
    fn providers(access: &str, expires: Option<i64>) -> String {
        json!({"version": 1, "lastUsedProvider": "cline-pass", "modes": {}, "providers": {"cline": {
            "settings": {"provider": "cline", "auth": {
                "accessToken": access, "refreshToken": "never-sent", "accountId": "usr_1",
                "expiresAt": expires,
                "metadata": {"provider": "google", "userInfo": {"email": "dev@example.com", "name": "Dev"}},
            }},
            "updatedAt": "2026-11-01T10:00:00.000Z", "tokenSource": "oauth",
        }}})
        .to_string()
    }

    fn limits() -> Value {
        json!({"success": true, "data": {"limits": [
            {"type": "five_hour", "percentUsed": 37.5, "resetsAt": "2026-11-04T15:00:00.000Z"},
            {"type": "weekly", "percentUsed": 120, "resetsAt": "2026-11-09T00:00:00Z"},
            {"type": "monthly", "percentUsed": 4, "resetsAt": null},
            {"type": "daily_burst", "percentUsed": 99},
        ]}})
    }

    #[test]
    fn the_browser_sign_in_reads_the_plans_windows() {
        let access = token(1);
        let host = FakeHost::new("/u")
            .file(SETTINGS, &providers(&access, None))
            .answer(USAGE_LIMITS, Ok(limits()));
        let reading = read(&host).unwrap();
        let id = account_id("cline", &["usr_1"]);
        let account = reading.account.unwrap();
        assert_eq!(account["id"], id.as_str());
        assert_eq!(account["label"], "dev@example.com");
        assert_eq!(account["plan"], "ClinePass");
        assert_eq!(reading.quotas.len(), 3, "an unknown kind is left out");
        let five = &reading.quotas[0];
        assert_eq!(five["id"], format!("{id}/cline:five_hour"));
        assert_eq!(five["vendor"], "cline");
        assert_eq!(five["name"], "5-hour limit");
        assert_eq!(five["shortLabel"], "5h");
        assert_eq!(five["usedPercent"], 37.5);
        assert_eq!(five["windowDurationSeconds"], 18_000);
        assert_eq!(five["resetAt"], "2026-11-04T15:00:00+00:00");
        assert_eq!(reading.quotas[1]["usedPercent"], 100.0);
        assert_eq!(reading.quotas[1]["shortLabel"], "Weekly");
        assert!(reading.quotas[2]["resetAt"].is_null());

        let asked = host.asked.borrow();
        assert_eq!(
            asked[..],
            [(USAGE_LIMITS.into(), Some(format!("Bearer workos:{access}")))]
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
    fn a_saved_prefix_is_sent_once_and_a_key_or_the_variable_stands_in() {
        let access = format!("workos:{}", token(1));
        let host = FakeHost::new("/u")
            .file(SETTINGS, &providers(&access, None))
            .answer(USAGE_LIMITS, Ok(limits()));
        read(&host).unwrap();
        assert_eq!(host.asked.borrow()[0].1, Some(format!("Bearer {access}")));

        let key = json!({"version": 1, "providers": {"cline": {"settings": {"provider": "cline", "apiKey": "sk-cline-1"}}}});
        let host = FakeHost::new("/u")
            .file(SETTINGS, &key.to_string())
            .answer(USAGE_LIMITS, Ok(limits()));
        read(&host).unwrap();
        assert_eq!(
            host.asked.borrow()[0].1.as_deref(),
            Some("Bearer sk-cline-1")
        );

        let host = FakeHost::new("/u")
            .env("CLINE_API_KEY", "sk-env")
            .answer(USAGE_LIMITS, Ok(limits()));
        let reading = read(&host).unwrap();
        assert_eq!(host.asked.borrow()[0].1.as_deref(), Some("Bearer sk-env"));
        assert_eq!(
            reading.account.unwrap()["id"],
            account_id("cline", &["default"]).as_str()
        );
    }

    #[test]
    fn cline_data_dir_moves_the_settings() {
        let moved = format!("{}/elsewhere", "/u");
        let host = FakeHost::new("/u")
            .env("CLINE_DATA_DIR", &moved)
            .file(SETTINGS, &providers(&token(-1), None))
            .file(
                &format!("{moved}/settings/providers.json"),
                &providers(&token(1), None),
            )
            .answer(USAGE_LIMITS, Ok(limits()));
        assert_eq!(read(&host).unwrap().quotas.len(), 3);
    }

    #[test]
    fn an_expired_token_waits_for_cline_and_none_is_signed_out() {
        let host = FakeHost::new("/u").file(SETTINGS, &providers(&token(-1), None));
        assert!(matches!(read(&host), Err(Unread::Failed(_))));
        assert_eq!(
            read(&host).err().unwrap(),
            "Cline renews its sign-in the next time it runs; ClinePass updates then"
        );
        // The saved expiry counts over the token's own.
        let past = chrono::Utc::now().timestamp_millis() - 1000;
        let host = FakeHost::new("/u").file(SETTINGS, &providers(&token(5), Some(past)));
        assert!(matches!(read(&host), Err(Unread::Failed(_))));
        assert!(
            host.asked.borrow().is_empty(),
            "an expired token is not sent"
        );

        let host = FakeHost::new("/u");
        assert!(matches!(read(&host), Err(Unread::SignedOut(_))));
        assert!(host.asked.borrow().is_empty());
    }

    #[test]
    fn failures_say_whose_they_are() {
        let host = |answer| {
            FakeHost::new("/u")
                .file(SETTINGS, &providers(&token(1), None))
                .answer(USAGE_LIMITS, answer)
        };
        assert!(matches!(
            read(&host(Err(HttpFailure::Status(401)))),
            Err(Unread::SignedOut(_))
        ));
        assert!(matches!(
            read(&host(Err(HttpFailure::Status(403)))),
            Err(Unread::Failed(_))
        ));
        assert!(matches!(
            read(&host(Err(HttpFailure::Limited {
                retry_after: Some(30)
            }))),
            Err(Unread::Transient {
                retry_after: Some(_),
                ..
            })
        ));
        assert_eq!(
            read(&host(Err(HttpFailure::Unreachable))).err().unwrap(),
            "ClinePass's limits could not be reached"
        );
        assert!(matches!(
            read(&host(Ok(json!({"success": false})))),
            Err(Unread::Failed(_))
        ));
        assert!(matches!(
            read(&host(Ok(
                json!({"success": true, "data": {"limits": [{"type": "weekly", "percentUsed": "most"}]}})
            ))),
            Err(Unread::Failed(_))
        ));
        // A signed-in account without the plan: no windows, no plan.
        let reading = read(&host(Ok(json!({"success": true, "data": {"limits": []}})))).unwrap();
        assert!(reading.quotas.is_empty());
        assert!(reading.account.unwrap()["plan"].is_null());
    }

    #[test]
    fn clinepass_is_clines_provider() {
        let provider = crate::account_provider("cline").unwrap();
        assert!(std::ptr::eq(provider, &PROVIDER));
        assert!(provider.local_work);
        assert_eq!(provider.sources(), ["cline"]);
    }
}
