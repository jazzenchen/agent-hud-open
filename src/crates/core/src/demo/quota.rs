//! The demo's year of quota readings: each Claude and Codex window read every
//! hour its models worked and once at every day's end, full as far as the
//! work in its cycle had come. Past cycles fill with their work against the
//! last two months' pace, so the light months of the year sit low, a busy
//! week now and then runs out, and the current cycle ends at the sample's
//! reading now. The other vendors' windows get one reading each evening.
use super::data::{EXTRAS, HISTORY_DAYS, READINGS};
use crate::{metering, plan_value, AgentHudCore};
use std::collections::BTreeMap;

const HOUR: i64 = 3_600_000;
const DAY: i64 = 86_400_000;

/// A quota window's readings: its account, the provider and plan it is read
/// under, the models that spend it, and how full an ordinary cycle ends.
struct Track {
    key: &'static str,
    account: &'static str,
    provider: &'static str,
    plan: &'static str,
    models: &'static [&'static str],
    typical: f64,
}

const CLAUDE: &[&str] = &["claude-fable-5-1", "claude-opus-5-5", "claude-sonnet-5-5"];
const CODEX: &[&str] = &["codex-auto-review", "gpt-6.1-sol"];
const SPARK: &[&str] = &["gpt-6.1-sol"];

const TRACKS: [Track; 6] = [
    Track {
        key: "claude-session",
        account: "claude:demo",
        provider: "claude",
        plan: "max_20x",
        models: CLAUDE,
        typical: 45.0,
    },
    Track {
        key: "claude-weekly",
        account: "claude:demo",
        provider: "claude",
        plan: "max_20x",
        models: CLAUDE,
        typical: 82.0,
    },
    Track {
        key: "claude-weekly-fable",
        account: "claude:demo",
        provider: "claude",
        plan: "max_20x",
        models: &["claude-fable-5-1"],
        typical: 64.0,
    },
    Track {
        key: "codex",
        account: "codex:demo",
        provider: "codex",
        plan: "pro",
        models: CODEX,
        typical: 58.0,
    },
    Track {
        key: "codex:spark:primary",
        account: "codex:demo",
        provider: "codex",
        plan: "pro",
        models: SPARK,
        typical: 30.0,
    },
    Track {
        key: "codex:spark:secondary",
        account: "codex:demo",
        provider: "codex",
        plan: "pro",
        models: SPARK,
        typical: 26.0,
    },
];

/// The readings of one window as (observed, share used, reset), oldest first.
fn readings(
    track: &Track,
    now: i64,
    history: &[(i64, &'static str, &'static str, u64, u64, u64)],
) -> Vec<(i64, f64, i64)> {
    let reading = READINGS
        .iter()
        .find(|reading| reading.key == track.key)
        .expect("a demo window");
    let length = reading.duration * 1000;
    let reset = now + reading.reset_in * 1000;
    let since = now - HISTORY_DAYS as i64 * DAY;
    // Tokens by the hour they end in.
    let mut hours: BTreeMap<i64, u64> = BTreeMap::new();
    for row in history.iter().filter(|row| track.models.contains(&row.2)) {
        *hours
            .entry((row.0.div_euclid(HOUR) + 1) * HOUR)
            .or_default() += row.3 + row.4;
    }
    // Cycles back from the current one, which ends at `reset`.
    let mut cycles = Vec::new();
    let mut end = reset;
    while end > since {
        let start = end - length;
        let worked: Vec<(i64, u64)> = hours
            .range(start + 1..=end.min(now))
            .map(|(at, tokens)| (*at, *tokens))
            .collect();
        cycles.push((start, end, worked));
        end = start;
    }
    // The pace of the last two months' cycles is the ordinary one.
    let totals: Vec<u64> = cycles[1..]
        .iter()
        .filter(|cycle| cycle.1 > now - 60 * DAY)
        .map(|cycle| cycle.2.iter().map(|hour| hour.1).sum())
        .filter(|total| *total > 0)
        .collect();
    let mean = totals.iter().sum::<u64>() as f64 / totals.len().max(1) as f64;
    let weekly = length >= DAY;
    let mut out = Vec::new();
    for (index, (start, end, worked)) in cycles.into_iter().enumerate().rev() {
        let total: u64 = worked.iter().map(|hour| hour.1).sum();
        if total == 0 {
            continue;
        }
        let peak = if index == 0 {
            reading.used
        } else {
            (track.typical * (total as f64 / mean).powf(0.6)).clamp(1.0, 100.0)
        };
        let full = |at: i64| {
            let done: u64 = worked
                .iter()
                .take_while(|hour| hour.0 <= at)
                .map(|hour| hour.1)
                .sum();
            peak * done as f64 / total as f64
        };
        let mut times: Vec<i64> = worked.iter().map(|hour| hour.0).collect();
        if weekly {
            // A reading at each Beijing day's end in the cycle.
            let mut day = (start + 8 * HOUR).div_euclid(DAY) * DAY - 8 * HOUR + DAY - 60_000;
            while day < end.min(now) {
                if day > start {
                    times.push(day);
                }
                day += DAY;
            }
        }
        times.retain(|at| *at > since && *at < now);
        times.sort_unstable();
        times.dedup();
        out.extend(times.into_iter().map(|at| (at, full(at), end)));
    }
    out
}

/// The other vendors' windows read at 6 in the evening in Beijing on each
/// day before today: (observed, window id, share used). Their sample's
/// reading sets the size, lighter early in the year and at weekends.
fn extra_readings(now: i64) -> Vec<(i64, String, f64)> {
    let offset = 8 * HOUR;
    let evening = (now + offset).div_euclid(DAY) * DAY - offset + 18 * HOUR;
    let mut out = Vec::new();
    for (index, (vendor, _, remaining, _)) in EXTRAS.iter().enumerate() {
        let id = format!("{vendor}:demo:window");
        for back in 1..HISTORY_DAYS as i64 {
            let at = evening - back * DAY;
            if at >= now {
                continue;
            }
            // 1970-01-01 was a Thursday: 0 is Sunday and 6 Saturday.
            let weekday = ((at + offset).div_euclid(DAY) + 4).rem_euclid(7);
            let rest = if weekday == 0 || weekday == 6 {
                0.4
            } else {
                1.0
            };
            let ramp = 0.35 + 0.65 * (1.0 - back as f64 / HISTORY_DAYS as f64);
            let wave = 0.55 + 0.45 * ((back * 37 + index as i64 * 11).rem_euclid(13)) as f64 / 12.0;
            out.push((
                at,
                id.clone(),
                ((100.0 - remaining) * ramp * wave * rest).clamp(0.0, 100.0),
            ));
        }
    }
    out
}

impl AgentHudCore {
    /// Writes the demo's year of quota readings for the Claude and Codex
    /// windows, before the account projection adds the reading now.
    pub(crate) fn seed_demo_quotas(
        &mut self,
        now: i64,
        history: &[(i64, &'static str, &'static str, u64, u64, u64)],
    ) -> Result<(), String> {
        let Some(database) = &mut self.database else {
            return Ok(());
        };
        let transaction = database.transaction().map_err(|error| error.to_string())?;
        for track in &TRACKS {
            let id = format!("{}:{}", track.account, track.key);
            let plan = plan_value::plan_key(track.provider, track.plan);
            for (at, used, reset) in readings(track, now, history) {
                metering::insert_quota_sample(
                    &transaction,
                    &id,
                    at,
                    (100.0 - used).clamp(0.0, 100.0),
                    plan.as_deref(),
                    Some(reset),
                )
                .map_err(|error| error.to_string())?;
            }
        }
        for (at, id, used) in extra_readings(now) {
            metering::insert_quota_sample(&transaction, &id, at, 100.0 - used, None, None)
                .map_err(|error| error.to_string())?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        self.meters.samples = None;
        Ok(())
    }
}
