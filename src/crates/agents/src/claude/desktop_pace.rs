//! How often the Claude desktop app's sign-in is asked for the quota.
//!
//! The sign-in is another app's, kept for that app's own use, so Agent HUD
//! asks with it less often than with Claude Code's (owner, 2026-11-22: it
//! must never look like abuse to Claude's service): every five minutes, give
//! or take half a minute drawn anew each time, whatever the pace the engine
//! reads Claude at. Between two askings the last answer stands, as old as
//! it is. It is kept in Agent HUD's own folder, so a restart does not ask
//! sooner. A token the service refused is never sent again.
use crate::account::{Host, Reading, Unread};
use serde_json::{json, Value};

/// The time between two askings with the desktop app's sign-in, and how far
/// each one is moved either way.
pub(super) const EVERY_MS: i64 = 5 * 60_000;
const SPREAD_MS: i64 = 30_000;
const FILE: &str = "claude-desktop-reading.json";

/// What the last asking for the same desktop account left.
pub(super) struct Last {
    /// The token the service refused (`desktop_token`'s id for it).
    pub(super) refused: Option<String>,
    /// The account's name, so it is not asked for again.
    pub(super) label: Option<String>,
}

/// `ask`'s answer for the desktop account `user`, asked once in about
/// `EVERY_MS`; the kept answer otherwise, its windows dated when they were
/// read. Another account signed in to the desktop app is asked at once.
pub(super) fn paced(
    host: &dyn Host,
    user: Option<&str>,
    ask: impl FnOnce(&mut Last) -> Option<Result<Reading, Unread>>,
) -> Option<Result<Reading, Unread>> {
    let now = chrono::Utc::now().timestamp_millis();
    let memo = host
        .read_own(FILE, 1024 * 1024)
        .found()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|memo| memo["user"].as_str() == user)
        .unwrap_or(Value::Null);
    if let Some(kept) = kept(&memo, now) {
        return Some(kept);
    }
    let before = memo["refused"].as_str().map(str::to_owned);
    let mut last = Last {
        refused: before.clone(),
        label: memo["read"]["account"]["label"].as_str().map(str::to_owned),
    };
    let result = ask(&mut last);
    // Anywhere in the spread, by the clock's own fraction of a second.
    let moved = i64::from(chrono::Utc::now().timestamp_subsec_nanos()) % (2 * SPREAD_MS + 1);
    let due = now + EVERY_MS - SPREAD_MS + moved;
    let mut next = json!({"at": now, "due": due, "user": user, "refused": last.refused});
    match &result {
        Some(Ok(reading)) => {
            next["read"] = json!({"quotas": reading.quotas, "account": reading.account});
        }
        Some(Err(unread)) => {
            next["failed"] = json!({"kind": unread.kind(), "reason": unread.reason()});
            // The name outlives a failed asking.
            next["read"] = json!({"account": {"label": last.label}});
        }
        None if last.refused == before => return None,
        None => {}
    }
    // Unwritten, the next reading only asks sooner.
    let _ = host.write_own(FILE, next.to_string().as_bytes());
    result
}

/// The answer `memo` keeps, until the next asking is due. A failed asking
/// waits as long as one that read.
fn kept(memo: &Value, now: i64) -> Option<Result<Reading, Unread>> {
    let at = memo["at"].as_i64()?;
    if !(at..memo["due"].as_i64()?).contains(&now) {
        return None;
    }
    if let Some(reason) = memo["failed"]["reason"].as_str() {
        return Some(Err(match memo["failed"]["kind"].as_str() {
            Some("signedOut") => Unread::SignedOut(reason.into()),
            Some("transient") => Unread::transient(reason),
            _ => Unread::Failed(reason.into()),
        }));
    }
    let read_at = json!(crate::iso(at));
    let mut reading = Reading::empty("claude");
    reading.quotas = memo["read"]["quotas"].as_array()?.clone();
    for row in &mut reading.quotas {
        row["observedAt"] = read_at.clone();
    }
    let mut account = memo["read"]["account"].clone();
    account.as_object()?;
    account["checkedAt"] = read_at;
    reading.account = Some(account);
    Some(Ok(reading))
}
