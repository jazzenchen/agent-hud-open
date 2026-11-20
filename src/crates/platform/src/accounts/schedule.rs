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
/// How often a provider is read otherwise, whether or not anything uses it
/// here: a reading is the vendor's own count, wherever it was spent.
const QUIET: u64 = 180;
/// How long a turn's end waits before its reading, at the least: ends that
/// follow within it put the reading off again, so turns that end together
/// are read once.
const SETTLE_MS: i64 = 5_000;

/// When a provider's next reading is due: every 120 s while a turn of its
/// sources runs and every 180 s otherwise, and 5–15 s after the last of its
/// sources' turns that ended since the last reading. Every interval waits the
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
    // The last of its sources' turns that ended since the last reading is read
    // 5 s plus a sixth of the spread after it ended (5–15 s): the vendor has
    // the last call on its books by then, a turn that ends meanwhile puts the
    // reading off again, and the providers one turn's end makes due are not
    // read back to back. Turns that keep ending still wait no longer than the
    // interval below.
    let after_finish = sources
        .iter()
        .filter_map(|source| state.finished.get(*source))
        .filter(|at| **at > entry.attempted_ms)
        .max()
        .map(|at| {
            let settled = at + SETTLE_MS + entry.spread as i64 * 1000 / 6;
            Duration::from_millis((settled - now).max(0) as u64)
        });
    // After failed readings the next try backs off, whatever the interval;
    // a refresh the user asked for tries at once, and a turn ending as above
    // (signing in again and working is how many failures end).
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
    let interval = if state.force.contains(provider) {
        60
    } else if running {
        RUNNING + entry.spread
    } else {
        QUIET + entry.spread
    };
    let mut pause = Duration::from_secs(interval).saturating_sub(elapsed);
    if let Some(after) = after_finish {
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
