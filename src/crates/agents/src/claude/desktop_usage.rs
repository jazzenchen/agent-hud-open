//! Claude's plan limits through the Claude desktop app's claude.ai sign-in.
use super::account::{profile, Folder};
use super::quota_windows::{plan_reading, profile_plan, windows};
use crate::account::{read_json, Answer, Host, HttpFailure, HttpRequest, Reading, Unread};
use crate::l10n;
use crate::Base;
use serde_json::{json, Value};
use std::time::Duration;

/// The Claude desktop app's data folder, where it keeps its claude.ai sign-in.
const DESKTOP: &str = "Claude";
const SITE: &str = "claude.ai";
const WEB_API: &str = "https://claude.ai/api";

fn desktop_unreadable() -> String {
    l10n::text(
        "暂时无法读取 Claude 桌面版的登录，稍后自动重试",
        "Couldn't read the Claude desktop app's sign-in; retrying shortly",
    )
}

/// claude.ai answered with Cloudflare's browser check. The desktop app, a
/// browser, passes it; this request can't. A signed-out Claude Code reads
/// without it once signed in again, so that is the way back.
fn desktop_challenged(signed_out: bool) -> String {
    if signed_out {
        l10n::text(
            "Claude Code 已退出登录，claude.ai 也要求浏览器验证；运行 claude 重新登录后更新额度",
            "Claude Code is signed out and claude.ai asked for a browser check. Run claude to sign in again to update the quota",
        )
    } else {
        l10n::text(
            "claude.ai 要求浏览器验证，暂时无法通过 Claude 桌面版读取额度，稍后自动重试",
            "claude.ai asked for a browser check, so the Claude desktop app's sign-in can't read the quota; retrying shortly",
        )
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
        return Some(Err(l10n::text(
            "Claude 桌面版的登录返回了无法识别的额度数据",
            "The Claude desktop app's sign-in returned unrecognised usage data",
        )
        .into()));
    };
    let plan = profile_plan(&who);
    Some(Ok(plan_reading(host, windows, &who, plan, &folder.path)))
}
