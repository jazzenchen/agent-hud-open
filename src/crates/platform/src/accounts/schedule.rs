//! When each provider's next reading is due.
use super::{date_ms, Control, Entry};
use agent_hud_agents::account::Provider;
use std::time::{Duration, Instant};

/// Retries after failed readings wait 60 s, then twice as long each time,
/// up to this.
pub(super) const MOST_BACKOFF: Duration = Duration::from_secs(15 * 60);

/// How long after a failed reading the next one waits, by how many failed in
/// a row.
fn backoff(failures: u32) -> Duration {
    Duration::from_secs(60)
        .saturating_mul(1u32 << failures.saturating_sub(1).min(10))
        .min(MOST_BACKOFF)
}
/// How often a provider is read while a turn of its sources runs.
const RUNNING: u64 = 120;
/// How often a provider is read while its sources are quiet, or where this
/// computer's work does not move its windows.
const QUIET: u64 = 240;

/// When a provider's next reading is due: every 120 s while a turn of its
/// sources runs, every 240 s while a session is in flight but quiet or where
/// this computer's work does not move its windows, a few seconds after work
/// that finished since the last reading, and an idle provider whose windows
/// all reset later only at the first reset. Every interval waits the
/// reading's spread past it (0–60 s, drawn again after every reading), so
/// providers keep out of step with each other and with the vendor's clock;
/// no reading follows another within 60 s.
pub(super) fn due_after(facts: &Provider, entry: &Entry, state: &Control) -> Duration {
    let provider = facts.id;
    if entry.restored {
        return Duration::ZERO;
    }
    let elapsed = entry.attempted.elapsed();
    // What the service asked holds even for a refresh the user asked for.
    let asked = entry.retry_at.map_or(Duration::ZERO, |at| {
        at.saturating_duration_since(Instant::now())
    });
    let spacing = Duration::from_secs(60).saturating_sub(elapsed);
    let now = chrono::Utc::now().timestamp_millis();
    let resets = entry
        .reading
        .quotas
        .iter()
        .filter_map(|row| row["resetAt"].as_str().and_then(date_ms))
        .collect::<Vec<_>>();
    let sources = facts.sources();
    let running = sources.iter().any(|source| state.running.contains(*source));
    let live = sources.iter().any(|source| state.live.contains(*source));
    let local = facts.local_work && !state.account_homes.contains_key(provider);
    // This computer's work for the provider that finished since the last
    // reading is read a quarter of the spread after it ended (0–15 s): the
    // vendor has the last call on its books by then, and the providers one
    // turn's end makes due are not read back to back.
    let after_finish = sources
        .iter()
        .filter_map(|source| state.finished.get(*source))
        .filter(|at| **at > entry.attempted_ms)
        .max()
        .filter(|_| local)
        .map(|at| Duration::from_millis((at + entry.spread as i64 * 250 - now).max(0) as u64));
    // After failed readings the next try backs off, whatever the interval;
    // a refresh the user asked for tries at once, and this computer's work
    // finishing as above (signing in again and working is how many failures
    // end).
    if entry.failures > 0 {
        let mut retry = if state.force.contains(provider) {
            Duration::ZERO
        } else {
            backoff(entry.failures).saturating_sub(elapsed)
        };
        if let Some(after) = after_finish {
            retry = retry.min(after);
        }
        return retry.max(spacing).max(asked);
    }
    let quiet_known_reset = local
        && !entry.failed
        && !resets.is_empty()
        && resets.iter().all(|reset| *reset > entry.attempted_ms);
    let interval = if state.force.contains(provider) {
        60
    } else if running {
        RUNNING + entry.spread
    } else if live || !local || !quiet_known_reset {
        QUIET + entry.spread
    } else {
        86400
    };
    let mut pause = Duration::from_secs(interval).saturating_sub(elapsed);
    if let Some(after) = after_finish.filter(|_| !running && !live) {
        pause = pause.min(after);
    }
    for reset in resets
        .into_iter()
        .filter(|reset| *reset > entry.attempted_ms)
    {
        pause = pause.min(Duration::from_millis(
            (reset + entry.spread as i64 * 1000 - now).max(0) as u64,
        ));
    }
    // Account requests are never <60s apart.
    pause.max(spacing).max(asked)
}
