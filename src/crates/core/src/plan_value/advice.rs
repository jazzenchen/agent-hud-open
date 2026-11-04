//! A tier that fits better, only where the quota readings plainly show it:
//! the completed cycles of a long window, their peaks, and the advice.
use super::*;

/// A cycle that reached this share of its window ran out.
const RAN_OUT: f64 = 95.0;
/// A tier fits when the busiest use would take at most this share of it.
const FITS: f64 = 80.0;
/// The completed cycles advice looks back over, and how many it needs.
const CYCLES: usize = 4;
const LEAST_CYCLES: usize = 2;
/// A cycle whose last reading came within this share of its length of its
/// end, and at least 20 minutes, shows its peak: within a cycle the used
/// share only grows. A week's is 7 hours.
const END_SHARE: i64 = 24;
const END_MS: i64 = 20 * 60_000;

/// The completed cycles in a window's readings, (time, used share, plan
/// key) oldest first, as `quota_history` parts them, and also where two
/// readings lie a whole window length apart; the cycle still running is
/// left out. `cycle_start` is the current cycle's start, when known.
pub(super) fn completed(
    samples: &[(i64, f64, Option<String>)],
    cycle_start: Option<i64>,
    length: i64,
    plan: &str,
) -> Vec<Cycle> {
    let close = |run: &[&(i64, f64, Option<String>)], ends_known: bool| Cycle {
        peak: run.iter().map(|sample| sample.1).fold(0.0, f64::max),
        ends_known,
        same_plan: run.iter().all(|sample| sample.2.as_deref() == Some(plan)),
    };
    let slack = (length / END_SHARE).max(END_MS);
    let mut cycles = Vec::new();
    let mut run = Vec::<&(i64, f64, Option<String>)>::new();
    for sample in samples {
        if let Some(previous) = run.last().copied() {
            let at = quota_history::reset_between(
                (previous.0, previous.1),
                (sample.0, sample.1),
                cycle_start,
            );
            // How close the last reading came to the end: the known start
            // of the next cycle, else the first reading seen after it.
            let ends_known = match at {
                Some(at) if Some(at) == cycle_start => previous.0 >= at - slack,
                Some(_) => sample.0 - previous.0 <= slack,
                None => false,
            };
            if at.is_some() || sample.0 - previous.0 >= length {
                cycles.push(close(&run, ends_known));
                run.clear();
            }
        }
        run.push(sample);
    }
    // No reading yet since the current cycle began: the last run ended too.
    if let (Some(start), Some(last)) = (cycle_start, run.last()) {
        if last.0 < start {
            cycles.push(close(&run, last.0 >= start - slack));
        }
    }
    cycles
}

/// Advice for an account on `plan`: a higher tier when its longest window
/// ran out (`RAN_OUT`) in at least two of its last `CYCLES` completed cycles
/// under this plan, the cheapest with a stated multiple that would have held
/// the busiest of them to `FITS`; a lower one when at least two such cycles
/// all show their peak and the busiest use of every window of the account
/// would still have fitted a cheaper tier with a stated multiple. `long` is
/// the longest window, `windows` every timed window of the account, each
/// with its readings. None when unsure, or for a tier without a multiple.
pub(super) fn advise(
    catalog: &Catalog,
    plan: &Plan,
    long: &(Value, Vec<(i64, f64, Option<String>)>),
    windows: &[(Value, Vec<(i64, f64, Option<String>)>)],
) -> Option<Value> {
    plan.multiple?;
    let (row, samples) = long;
    let seconds = row["windowDurationSeconds"].as_f64()?;
    let length = (seconds * 1000.0) as i64;
    let reset = row["resetAt"].as_str().and_then(metering::date_ms);
    let cycles = completed(samples, reset.map(|reset| reset - length), length, plan.id)
        .into_iter()
        .filter(|cycle| cycle.same_plan)
        .collect::<Vec<_>>();
    let recent = &cycles[cycles.len().saturating_sub(CYCLES)..];
    if recent.len() < LEAST_CYCLES {
        return None;
    }
    let ran_out = recent.iter().filter(|cycle| cycle.peak >= RAN_OUT).count();
    let busiest = recent.iter().map(|cycle| cycle.peak).fold(0.0, f64::max);
    // A window's use on another tier, by both tiers' multiples for a window
    // of its length (a tier may allow a different multiple in five hours).
    let fits = |tier: &Plan, used: f64, seconds: f64| {
        plan.multiple_for(seconds)
            .zip(tier.multiple_for(seconds))
            .is_some_and(|(mine, other)| used * mine / other <= FITS)
    };
    let window = json!({"id":row["id"],"name":row["name"],"shortLabel":row["shortLabel"],"windowDurationSeconds":row["windowDurationSeconds"]});
    let advice = |kind: &str, tier: &Plan, used: f64| {
        json!({"kind":kind,"plan":plan_json(tier),"monthlyDeltaUSD":tier.monthly_usd-plan.monthly_usd,
            "window":window,"cycles":recent.len(),"ranOut":ran_out,"busiestPercent":used})
    };
    if ran_out >= LEAST_CYCLES {
        return catalog
            .plans
            .iter()
            .find(|tier| tier.monthly_usd > plan.monthly_usd && fits(tier, busiest, seconds))
            .map(|tier| advice("upgrade", tier, busiest));
    }
    if !recent.iter().all(|cycle| cycle.ends_known) {
        return None;
    }
    // Every window must fit: the long one's cycles and what it uses now,
    // and the highest reading of each other window under this plan, each by
    // the multiples for its own length.
    let mut uses = vec![(busiest, seconds)];
    for (row, samples) in windows {
        let Some(length) = row["windowDurationSeconds"].as_f64() else {
            continue;
        };
        let mut used = 0.0f64;
        if row["status"] != "stale" {
            used = used.max(row["usedPercent"].as_f64().unwrap_or(0.0));
        }
        for sample in samples
            .iter()
            .filter(|sample| sample.2.as_deref() == Some(plan.id))
        {
            used = used.max(sample.1);
        }
        uses.push((used, length));
    }
    let used = uses.iter().map(|(used, _)| *used).fold(0.0, f64::max);
    catalog
        .plans
        .iter()
        .find(|tier| {
            tier.monthly_usd < plan.monthly_usd
                && uses.iter().all(|(used, length)| fits(tier, *used, *length))
        })
        .map(|tier| advice("downgrade", tier, used))
}
