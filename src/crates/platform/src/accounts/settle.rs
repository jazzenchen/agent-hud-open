//! What a reading leaves in its provider's entry.
use super::{date_ms, AccountReading, Entry, WINDOW_RETENTION_MS};
use agent_hud_agents::account::{Provider, Unread};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    time::{Duration, Instant},
};

/// How long a reading that missed an answer (`Unread::Transient`) keeps the
/// last good one on screen as it was, without greying it or saying why:
/// a network hiccup or a busy service passes by itself.
const GRACE_MS: i64 = 10 * 60_000;
/// The longest wait a service's `Retry-After` is followed for.
const MOST_RETRY_AFTER: Duration = Duration::from_secs(3600);

/// A provider's reading, as its entry keeps it. A good one replaces the
/// entry. A failed one goes through `failed_reading`, with the windows the
/// agent itself last recorded where it brings them (`Reading::failure`).
pub(super) fn settle(
    entries: &mut BTreeMap<&'static str, Entry>,
    facts: &'static Provider,
    result: Result<AccountReading, Unread>,
    (attempted, attempted_ms): (Instant, i64),
    spread: u64,
) {
    let (record, result) = match result {
        Ok(mut reading) => match reading.failure.take() {
            Some(unread) => (Some(reading), Err(unread)),
            None => (None, Ok(reading)),
        },
        Err(unread) => (None, Err(unread)),
    };
    match result {
        Ok(mut reading) => {
            if facts.keeps_unreported_windows {
                retain_windows(entries.get(facts.id), &mut reading, attempted_ms);
            }
            let notice = reading.notice.clone();
            entries.insert(
                facts.id,
                Entry {
                    reading,
                    checked_at: Some(chrono::Utc::now().to_rfc3339()),
                    attempted,
                    attempted_ms,
                    notice,
                    failed: false,
                    spread,
                    restored: false,
                    failures: 0,
                    failure: None,
                    failing_since: None,
                    retry_at: None,
                },
            );
        }
        Err(unread) => {
            let entry = entries.entry(facts.id).or_insert_with(|| Entry {
                reading: AccountReading::empty(facts.id),
                checked_at: None,
                attempted,
                attempted_ms,
                notice: None,
                failed: true,
                spread,
                restored: false,
                failures: 0,
                failure: None,
                failing_since: None,
                retry_at: None,
            });
            if let Some(record) = record {
                take_record(entry, record);
            }
            failed_reading(entry, unread, attempted, attempted_ms);
            entry.spread = spread;
        }
    }
}

/// What the agent itself last recorded, brought by a failed reading: each
/// window and account newer than the one the entry shows, or that it does
/// not show, takes its place, keeping the time it was read. A row without
/// its own time was read when the entry was.
fn take_record(entry: &mut Entry, record: AccountReading) {
    let checked = entry.checked_at.clone();
    let read_at =
        |row: &Value, key: &str| row[key].as_str().or(checked.as_deref()).and_then(date_ms);
    for row in record.quotas {
        match entry
            .reading
            .quotas
            .iter_mut()
            .find(|old| old["id"] == row["id"])
        {
            Some(old) if read_at(old, "observedAt") >= read_at(&row, "observedAt") => {}
            Some(old) => *old = row,
            None => entry.reading.quotas.push(row),
        }
    }
    for account in record.account.into_iter().chain(record.additional_accounts) {
        let reading = &mut entry.reading;
        let shown = reading
            .account
            .iter_mut()
            .chain(&mut reading.additional_accounts)
            .find(|old| old["id"] == account["id"]);
        if let Some(old) = shown {
            if read_at(old, "checkedAt") < read_at(&account, "checkedAt") {
                *old = account;
            }
        } else if reading.account.is_none() {
            reading.account = Some(account);
        } else {
            reading.additional_accounts.push(account);
        }
    }
}

/// Records a failed reading. A reading that only missed an answer, while the
/// last good one is under `GRACE_MS` old, changes nothing on screen; any
/// other failure is shown at once with its reason, and the last reading
/// stays, marked stale. Either way the next try backs off, and waits as long
/// as the service asked.
pub(super) fn failed_reading(
    entry: &mut Entry,
    unread: Unread,
    attempted: Instant,
    attempted_ms: i64,
) {
    let recent = entry
        .checked_at
        .as_deref()
        .and_then(date_ms)
        .is_some_and(|at| attempted_ms.saturating_sub(at) < GRACE_MS);
    let has_reading = !entry.reading.quotas.is_empty()
        || !entry.reading.billing.is_empty()
        || entry.reading.account.is_some();
    let unnoticed =
        !entry.failed && has_reading && recent && matches!(unread, Unread::Transient { .. });
    entry.attempted = attempted;
    entry.attempted_ms = attempted_ms;
    entry.restored = false;
    entry.failures = entry.failures.saturating_add(1);
    entry.failing_since.get_or_insert(attempted_ms);
    entry.retry_at = match &unread {
        Unread::Transient {
            retry_after: Some(wait),
            ..
        } => Some(attempted + (*wait).min(MOST_RETRY_AFTER)),
        _ => None,
    };
    if unnoticed {
        return;
    }
    entry.failure = Some(unread.kind());
    entry.notice = Some(unread.reason().to_owned());
    entry.failed = true;
}

/// An absent reading is not a reset (agent-hud-native-open
/// RetainedUsageProvider.retainingReadings): a Claude window this reading
/// left out keeps its last reading, and the time it was read, while the same
/// account is signed in (any of the reading's accounts, one per folder) and
/// for at most 30 days. Codex's app-server reports its complete window
/// inventory, so a window it leaves out retires at once; the other
/// providers' rules belong to their own readers.
pub(super) fn retain_windows(previous: Option<&Entry>, reading: &mut AccountReading, now: i64) {
    let Some(previous) = previous else {
        return;
    };
    let accounts = reading
        .account
        .iter()
        .chain(&reading.additional_accounts)
        .map(|account| account["id"].clone())
        .collect::<Vec<_>>();
    let reported = reading
        .quotas
        .iter()
        .filter_map(|row| row["id"].as_str().map(str::to_owned))
        .collect::<HashSet<_>>();
    let kept = previous
        .reading
        .quotas
        .iter()
        .filter(|row| row["id"].as_str().is_some_and(|id| !reported.contains(id)))
        .filter(|row| accounts.contains(&row["accountId"]))
        .filter_map(|row| {
            let mut row = row.clone();
            if row["observedAt"].is_null() {
                row["observedAt"] = json!(previous.checked_at);
            }
            let seen = row["observedAt"].as_str().and_then(date_ms)?;
            (now - seen < WINDOW_RETENTION_MS).then_some(row)
        })
        .collect::<Vec<_>>();
    reading.quotas.extend(kept);
}
