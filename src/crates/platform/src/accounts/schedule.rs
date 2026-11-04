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
/// When a provider's next reading is due, as Open's `accountCheck`: a
/// running turn is read every 150 s, a session in flight but quiet every
/// 300 s, a provider whose windows this Mac's work does not move every 300 s,
/// work that finished since the last reading at once, and an idle provider
/// whose windows all reset later only at the first reset. Every interval
/// waits the reading's spread past it, and no reading follows another within
/// 60 s.
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
    let finished = sources
        .iter()
        .filter_map(|source| state.finished.get(*source))
        .any(|at| *at > entry.attempted_ms);
    let local = facts.local_work && !state.account_homes.contains_key(provider);
    // After failed readings the next try backs off, whatever the interval;
    // a refresh the user asked for, or this computer's work finishing, tries
    // at once (signing in again and working is how many failures end).
    if entry.failures > 0 {
        let now_due = state.force.contains(provider) || (local && finished);
        let retry = if now_due {
            Duration::ZERO
        } else {
            backoff(entry.failures).saturating_sub(elapsed)
        };
        return retry.max(spacing).max(asked);
    }
    let quiet_known_reset = local
        && !entry.failed
        && !resets.is_empty()
        && resets.iter().all(|reset| *reset > entry.attempted_ms);
    let interval = if state.force.contains(provider) {
        60
    } else if running {
        150 + entry.spread
    } else if live || !local || !quiet_known_reset {
        300 + entry.spread
    } else {
        86400
    };
    let mut pause = Duration::from_secs(interval).saturating_sub(elapsed);
    if local && !running && !live && finished {
        pause = Duration::ZERO;
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
