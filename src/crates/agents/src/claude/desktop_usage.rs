//! Claude's plan limits through the Claude desktop app's claude.ai sign-in.
use super::account::{profile, Folder};
use super::quota_windows::{plan_reading, profile_plan, windows};
use crate::account::{add_readers, append_notice, legacy_account, merge, names_none, read_by};
use crate::account::{read_json, Answer, Host, HttpFailure, HttpRequest, QuotaReading};
use crate::account::{Reading, Unread};
use crate::l10n::tr;
use crate::Base;
use serde_json::{json, Value};
use std::time::Duration;

/// The Claude desktop app's data folder, where it keeps its claude.ai sign-in.
const DESKTOP: &str = "Claude";
const SITE: &str = "claude.ai";
const WEB_API: &str = "https://claude.ai/api";

fn desktop_unreadable() -> String {
    tr!("Couldn't read the Claude desktop app's sign-in; retrying shortly")
}

/// claude.ai answered with Cloudflare's browser check. The desktop app, a
/// browser, passes it; this request can't. A signed-out Claude Code reads
/// without it once signed in again, so that is the way back.
fn desktop_challenged(signed_out: bool) -> String {
    if signed_out {
        tr!("Claude Code is signed out and claude.ai asked for a browser check. Run claude to sign in again to update the quota")
    } else {
        tr!("claude.ai asked for a browser check, so the Claude desktop app's sign-in can't read the quota; retrying shortly")
    }
}

/// The plan limits through the Claude desktop app's claude.ai sign-in, as
/// claude.ai's own usage page reads them. None when the desktop app has no
/// sign-in to offer, or the user did not allow reading it: the engine's
/// reason then stands. `signed_out`: Claude Code is installed but signed out.
/// Only the default `folder` is read this way.
/// The account the Claude desktop app last signed in to.
fn desktop_user(host: &dyn Host) -> Option<String> {
    read_json(
        host,
        &host.folder(Base::AppData).join(DESKTOP).join("config.json"),
    )?["lastKnownAccountUuid"]
        .as_str()
        .filter(|user| !user.is_empty())
        .map(str::to_owned)
}
/// Whether the desktop app last signed in to the account Claude Code last
/// signed in to in `folder`.
pub(super) fn desktop_is_claude_codes(host: &dyn Host, folder: &Folder) -> bool {
    desktop_user(host).is_some_and(|user| profile(host, folder)["accountUuid"] == user.as_str())
}
pub(super) fn from_desktop(
    host: &dyn Host,
    folder: &Folder,
    signed_out: bool,
) -> Option<Result<Reading, Unread>> {
    // Another app's sign-in: only when the user lets quota reading go that far.
    if !host
        .quota_reading(super::AGENT.id)
        .allows(QuotaReading::Full)
    {
        return None;
    }
    let session = match host.app_cookie(DESKTOP, SITE, "sessionKey") {
        Answer::Found(value) if !value.trim().is_empty() => value,
        Answer::Failed => return Some(Err(Unread::transient(desktop_unreadable()))),
        _ => return None,
    };
    let cookie = format!("sessionKey={}", session.trim());
    let get = |url: &str| {
        host.http_json(&HttpRequest {
            url,
            secrets: &[("Cookie", cookie.as_str())],
            headers: &[("Accept", "application/json")],
            body: None,
            timeout: Duration::from_secs(15),
        })
    };
    // A sign-in claude.ai refuses has ended: as good as none. Its browser
    // check says nothing about the sign-in, so it is named as itself.
    let failed = |failure: HttpFailure| match failure {
        HttpFailure::Status(401 | 403) => None,
        HttpFailure::Challenged => Some(Err(if signed_out {
            Unread::SignedOut(desktop_challenged(true))
        } else {
            Unread::transient(desktop_challenged(false))
        })),
        failure => Some(Err(failure.unread())),
    };
    let user = desktop_user(host);
    let organization = host
        .app_cookie(DESKTOP, SITE, "lastActiveOrg")
        .found()
        .map(|organization| organization.trim().to_owned())
        .filter(|organization| !organization.is_empty());
    let cli = profile(host, folder);
    let same = desktop_is_claude_codes(host, folder)
        && cli["organizationUuid"].is_string()
        && organization
            .as_deref()
            .is_none_or(|organization| cli["organizationUuid"] == organization);
    let who = if same {
        // The account Claude Code last signed in to: its name and plan.
        cli
    } else {
        let account = match get(&format!("{WEB_API}/account")) {
            Ok(account) => account,
            Err(failure) => return failed(failure),
        };
        let memberships = account["memberships"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let membership = memberships
            .iter()
            .find(|membership| {
                organization.is_some()
                    && membership["organization"]["uuid"].as_str() == organization.as_deref()
            })
            .or(memberships.first())
            .cloned()
            .unwrap_or(Value::Null);
        let tier = membership["organization"]["rate_limit_tier"].as_str();
        json!({
            "accountUuid": user.as_deref().or(account["uuid"].as_str()),
            "organizationUuid": organization.as_deref().or(membership["organization"]["uuid"].as_str()),
            "emailAddress": account["email_address"],
            "organizationType": tier.filter(|tier| tier.contains("claude_max")).map(|_| "claude_max"),
            "organizationRateLimitTier": tier,
        })
    };
    let Some(organization) = who["organizationUuid"]
        .as_str()
        .filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
    else {
        return Some(Err(Unread::transient(desktop_unreadable())));
    };
    let limits = match get(&format!("{WEB_API}/organizations/{organization}/usage")) {
        Ok(limits) => limits,
        Err(failure) => return failed(failure),
    };
    let Some(windows) = windows(&limits) else {
        return Some(Err(tr!(
            "The Claude desktop app's sign-in returned unrecognised usage data"
        )
        .into()));
    };
    let plan = profile_plan(&who);
    let mut reading = plan_reading(host, windows, &who, plan, &folder.path);
    read_by(&mut reading, crate::split::CLAUDE_DESKTOP);
    Some(Ok(reading))
}

/// Claude Code's reading with the Claude desktop app's beside it: the
/// desktop app's own sign-in is read every time, whatever Claude Code's
/// folders said, and the readings are merged by account (a desktop app and
/// a command line are separate clients, each read, owner 2026-11-18). One
/// account read both ways shows once, from Claude Code's reading unless that
/// is only a stale record. A failure of either way says nothing while the
/// other read the same account. `consulted`: a signed-out default folder
/// asked the desktop app already.
pub(super) fn with_desktop(
    host: &dyn Host,
    folders: &[Folder],
    result: Result<Reading, Unread>,
    consulted: bool,
) -> Result<Reading, Unread> {
    // What Claude Code's folders read, unless the desktop app's sign-in
    // stood in for one.
    let result = result.map(|mut code| {
        read_by(&mut code, super::AGENT.id);
        code
    });
    if consulted {
        return result;
    }
    let Some(desktop) = from_desktop(host, &folders[0], false) else {
        return result;
    };
    // The accounts of the folders the desktop app last signed in to too.
    let user = desktop_user(host);
    let shared: Vec<String> = folders
        .iter()
        .map(|folder| profile(host, folder))
        .filter(|who| user.is_some() && who["accountUuid"].as_str() == user.as_deref())
        .filter_map(|who| {
            legacy_account(
                "Claude",
                who["accountUuid"].as_str().unwrap_or(""),
                who["organizationUuid"].as_str().unwrap_or(""),
            )
        })
        .collect();
    let covered = !shared.is_empty();
    match (result, desktop) {
        (Ok(mut code), Ok(desktop)) if code.failure.is_none() || !covered => {
            // A login with no plan limits (an API key, a relay) is no news
            // beside the desktop app's account: the command line's own page
            // tells where its calls go.
            if names_none(&code) && !names_none(&desktop) {
                code.notice = None;
            }
            merge(&mut code, desktop);
            Ok(code)
        }
        // The same account, read fresh: Claude Code's stale record goes, and
        // the account is still read through both.
        (Ok(code), Ok(mut desktop)) => {
            for account in desktop
                .account
                .iter_mut()
                .chain(&mut desktop.additional_accounts)
            {
                for known in code.account.iter().chain(&code.additional_accounts) {
                    if known["id"] == account["id"] {
                        add_readers(account, known);
                    }
                }
            }
            Ok(desktop)
        }
        (Err(failure), Ok(mut desktop)) => {
            if !covered {
                append_notice(&mut desktop, failure.reason().to_owned());
            }
            Ok(desktop)
        }
        (Ok(mut code), Err(failure)) => {
            if !covered {
                append_notice(&mut code, format!("Claude Desktop: {}", failure.reason()));
            }
            // The desktop app is signed in to Claude Code's account though
            // its own reading failed (claude.ai's browser check): the
            // account is still used in both.
            let desktop = json!({"readBy":[crate::split::CLAUDE_DESKTOP]});
            for account in code.account.iter_mut().chain(&mut code.additional_accounts) {
                if account["id"]
                    .as_str()
                    .is_some_and(|id| shared.iter().any(|known| known == id))
                {
                    add_readers(account, &desktop);
                }
            }
            Ok(code)
        }
        (Err(failure), Err(_)) => Err(failure),
    }
}
