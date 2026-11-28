//! Claude's plan limits through the Claude desktop app's sign-in, beside
//! Claude Code's own.
use super::account::{profile, Folder};
use super::desktop_pace::paced;
use super::desktop_token::{from_token, has_token};
use crate::account::{add_readers, append_notice, legacy_account, merge, names_none, read_by};
use crate::account::{read_json, Host, QuotaReading};
use crate::account::{Reading, Unread};
use serde_json::json;

/// The Claude desktop app's data folder, where it keeps its sign-in.
pub(super) const DESKTOP: &str = "Claude";

/// The account the Claude desktop app last signed in to.
pub(super) fn desktop_user(host: &dyn Host) -> Option<String> {
    read_json(host, &host.app_data(DESKTOP).join("config.json"))?["lastKnownAccountUuid"]
        .as_str()
        .filter(|user| !user.is_empty())
        .map(str::to_owned)
}
/// Whether the desktop app last signed in to the account Claude Code last
/// signed in to in `folder`.
pub(super) fn desktop_is_claude_codes(host: &dyn Host, folder: &Folder) -> bool {
    desktop_user(host).is_some_and(|user| profile(host, folder)["accountUuid"] == user.as_str())
}
/// The plan limits through the Claude desktop app's own sign-in
/// (`desktop_token`), asked at `desktop_pace`'s pace. None when the desktop
/// app has no sign-in to offer, or the user did not allow reading it: the
/// engine's reason then stands. Only the default `folder` is read this way.
pub(super) fn from_desktop(host: &dyn Host, folder: &Folder) -> Option<Result<Reading, Unread>> {
    // Another app's sign-in: only when the user lets quota reading go that far.
    if !host
        .quota_reading(super::AGENT.id)
        .allows(QuotaReading::Full)
    {
        return None;
    }
    // A desktop app that signed out keeps nothing to read with, and what was
    // read with its sign-in before no longer stands.
    if !has_token(host) {
        return None;
    }
    paced(host, desktop_user(host).as_deref(), |last| {
        from_token(host, folder, last)
    })
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
    let Some(desktop) = from_desktop(host, &folders[0]) else {
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
            // its own reading failed: the
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
