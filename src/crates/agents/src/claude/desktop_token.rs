//! Claude's plan limits with the Claude desktop app's own sign-in token.
//!
//! The desktop app keeps its sign-ins in its `config.json`, under
//! `oauth:tokenCacheV2`: Electron safe storage (the same key as its cookies)
//! over a JSON object of
//! `acct:<account>|<client>:<organization>:https://api.anthropic.com:<scopes>`
//! to `{token, expiresAt, profileReadAt, subscriptionType, rateLimitTier, …}`
//! (read in desktop 2026-11). One organization may have several, an older
//! one still naming the plan it was issued under. A token whose scopes hold `user:profile` reads
//! api.anthropic.com's usage, as a signed-in Claude Code does. The desktop
//! app's claude.ai cookie read the same until 2026-11-22; claude.ai met it
//! with a browser check for hours at a time, so it is no longer used.
//!
//! The token is only read and sent as it is. It is never refreshed: that
//! would replace the desktop app's own sign-in. An expired one is left for
//! the desktop app to renew, and one the service refuses is not sent again.
use super::account::{profile, Folder};
use super::desktop_pace::Last;
use super::desktop_usage::{desktop_user, DESKTOP};
use super::quota_windows::{plan_reading, profile_plan, windows};
use crate::account::{read_by, read_json, Host, HttpFailure, HttpRequest, Reading, Unread};
use crate::l10n::tr;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::time::Duration;

const API: &str = "https://api.anthropic.com/api/oauth";
const CACHE: &str = "oauth:tokenCacheV2";
const SERVICE: &str = ":https://api.anthropic.com:";

/// One of the desktop app's sign-ins that may read the usage.
struct SignIn {
    /// Names this token without holding it: its cache key and expiry.
    id: String,
    token: String,
    account: String,
    organization: String,
    expires: i64,
    /// When the desktop app last read this sign-in's plan; 0 when it never did.
    profiled: i64,
    plan: Value,
}

/// The desktop app's unexpired sign-ins whose scopes hold `user:profile`.
fn sign_ins(host: &dyn Host, now: i64) -> Vec<SignIn> {
    let config = host.app_data(DESKTOP).join("config.json");
    let Some(cache) = read_json(host, &config)
        .and_then(|config| STANDARD.decode(config[CACHE].as_str()?).ok())
        .and_then(|sealed| host.unseal_safe_storage(DESKTOP, &sealed).found())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return vec![];
    };
    let entries = cache.as_object().into_iter().flatten();
    entries
        .filter_map(|(key, entry)| {
            let (who, scopes) = key.strip_prefix("acct:")?.split_once(SERVICE)?;
            let (account, rest) = who.split_once('|')?;
            let organization = rest.split(':').nth(1)?;
            let expires = entry["expiresAt"].as_f64()? as i64;
            let token = entry["token"].as_str().filter(|token| !token.is_empty())?;
            (scopes.split(' ').any(|scope| scope == "user:profile") && expires > now + 60_000)
                .then(|| SignIn {
                    id: format!("{key}@{expires}"),
                    token: token.into(),
                    account: account.into(),
                    organization: organization.into(),
                    expires,
                    profiled: entry["profileReadAt"].as_f64().unwrap_or(0.0) as i64,
                    plan: json!({
                        "organizationType": entry["subscriptionType"].as_str().map(|plan| format!("claude_{plan}")),
                        "organizationRateLimitTier": entry["rateLimitTier"],
                    }),
                })
        })
        .collect()
}

/// Whether the desktop app keeps a token that may read the usage.
pub(super) fn has_token(host: &dyn Host) -> bool {
    !sign_ins(host, chrono::Utc::now().timestamp_millis()).is_empty()
}

/// The plan limits of the account the desktop app last signed in to: in
/// Claude Code's organization when that is the same account, with the
/// sign-in whose plan the desktop app read last, then the one that lasts
/// longest. None when the desktop app keeps no token that may read them, or
/// the service refused the one it keeps.
pub(super) fn from_token(
    host: &dyn Host,
    folder: &Folder,
    last: &mut Last,
) -> Option<Result<Reading, Unread>> {
    let now = chrono::Utc::now().timestamp_millis();
    let user = desktop_user(host);
    let cli = profile(host, folder);
    let preferred = (cli["accountUuid"].as_str() == user.as_deref() && user.is_some())
        .then(|| cli["organizationUuid"].as_str().map(str::to_owned))
        .flatten();
    let sign_in = sign_ins(host, now)
        .into_iter()
        .filter(|sign_in| user.as_deref().is_none_or(|user| sign_in.account == user))
        .filter(|sign_in| last.refused.as_deref() != Some(sign_in.id.as_str()))
        .max_by_key(|sign_in| {
            (
                preferred.as_deref() == Some(sign_in.organization.as_str()),
                sign_in.profiled,
                sign_in.expires,
            )
        })?;
    let bearer = format!("Bearer {}", sign_in.token);
    let get = |url: &str| {
        host.http_json(&HttpRequest {
            url,
            secrets: &[("Authorization", bearer.as_str())],
            headers: &[
                ("Accept", "application/json"),
                ("anthropic-beta", "oauth-2025-04-20"),
            ],
            body: None,
            timeout: Duration::from_secs(15),
        })
    };
    let limits = match get(&format!("{API}/usage")) {
        Ok(limits) => limits,
        // Ended, or without the scope after all.
        Err(HttpFailure::Status(401 | 403)) => {
            last.refused = Some(sign_in.id);
            return None;
        }
        Err(failure) => return Some(Err(failure.unread())),
    };
    let Some(windows) = windows(&limits) else {
        return Some(Err(tr!(
            "The Claude desktop app's sign-in returned unrecognised usage data"
        )
        .into()));
    };
    let who = if cli["accountUuid"] == sign_in.account.as_str()
        && cli["organizationUuid"] == sign_in.organization.as_str()
    {
        // The account Claude Code last signed in to: its name and plan.
        cli
    } else {
        // The token names no one; its account is asked for its name once.
        if last.label.is_none() {
            last.label = get(&format!("{API}/profile"))
                .ok()
                .and_then(|profile| profile["account"]["email"].as_str().map(str::to_owned));
        }
        let mut who = sign_in.plan;
        who["accountUuid"] = json!(sign_in.account);
        who["organizationUuid"] = json!(sign_in.organization);
        who["emailAddress"] = json!(last.label);
        who
    };
    let plan = profile_plan(&who);
    let mut reading = plan_reading(host, windows, &who, plan, &folder.path);
    read_by(&mut reading, crate::split::CLAUDE_DESKTOP);
    Some(Ok(reading))
}
