//! A window's readings cut into its cycles, for the quota history's bars: each
//! cycle from its start to its reset, with the most of it that was used. The
//! used share only rises within a cycle, so its highest reading is what the
//! cycle consumed.
use super::RESET_FALL;
use serde_json::{json, Value};

const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cycle {
    pub(crate) start: i64,
    pub(crate) end: i64,
    /// The highest used share read in it.
    pub(crate) used: f64,
    pub(crate) readings: usize,
    /// When it was last read.
    pub(crate) last: i64,
}

impl Cycle {
    fn new(start: i64) -> Self {
        Self {
            start,
            end: start,
            used: 0.0,
            readings: 0,
            last: start,
        }
    }

    fn read(&mut self, at: i64, used: f64) {
        self.used = self.used.max(used);
        self.readings += 1;
        self.last = at;
    }

    /// As the quota history sends it: running at `now`, or read for the last
    /// time well before its reset, so it may have used more (`partial`).
    pub(crate) fn json(&self, now: i64) -> Value {
        let current = self.end > now;
        let gap = ((self.end - self.start) / 10).max(30 * 60_000);
        json!({
            "start": self.start,
            "end": self.end,
            "used": (self.used * 10.0).round() / 10.0,
            "readings": self.readings,
            "current": current,
            "partial": !current && self.last < self.end - gap,
        })
    }
}

/// Whether the share falling from `before` to `after` is a reset: a fall of
/// `RESET_FALL` points, or to nothing from at least a point.
fn reset(before: f64, after: f64) -> bool {
    before - after >= RESET_FALL || (after <= 0.0 && before >= 1.0)
}

/// A reading: when, the share used, and the reset it reported, where it was
/// kept (readings from before the column have none).
pub(crate) type Reading = (i64, f64, Option<i64>);

/// Where one cycle gives way to the next, between two readings: at the later
/// reading, or exactly at the earlier one's reset when the reported reset
/// moved on (`end`), the next one then starting a length before its own.
struct Turn {
    at: i64,
    end: Option<i64>,
    start: Option<i64>,
}

/// The cycles of a window `length` ms long whose next reset is at `reset_at`,
/// from its readings oldest first. A new cycle begins where the reported
/// reset moves on, ending the last one at its reset; readings with no reset
/// kept fall back to the share: a fall of `RESET_FALL` points or to 0. A
/// window shorter than a day has a cycle from its first use to its length or
/// the next reset, so idle time is none; a longer one runs from reset to
/// reset, and where nothing shows a reset (the share stayed put) it is cut at
/// its resets counted back from `reset_at`. The last cycle ends at its reset.
pub(crate) fn cycles(
    samples: &[Reading],
    length: Option<i64>,
    reset_at: Option<i64>,
) -> Vec<Cycle> {
    let length = length.filter(|length| *length > 0);
    let long = length.is_none_or(|length| length >= DAY_MS);
    let turns: Vec<Turn> = samples
        .windows(2)
        .filter_map(|pair| {
            let (a, b) = (pair[0], pair[1]);
            let moved = matches!((a.2, b.2), (Some(old), Some(new)) if new > old + 60_000);
            (moved || reset(a.1, b.1)).then(|| Turn {
                at: b.0,
                end: if moved { a.2 } else { None },
                start: if moved {
                    b.2.zip(length).map(|(next, length)| next - length)
                } else {
                    None
                },
            })
        })
        .collect();
    let reset_at = samples.last().and_then(|last| last.2).or(reset_at);
    // The reset at or before `at`, counted back from the next one.
    let mark_before = |at: i64| -> Option<i64> {
        let (length, reset_at) = (length?, reset_at?);
        Some(reset_at - (reset_at - at + length - 1).div_euclid(length) * length)
    };
    let mut out: Vec<Cycle> = Vec::new();
    let mut open: Option<Cycle> = None;
    let mut passed = 0;
    for &(at, used, reported) in samples {
        while passed < turns.len() && turns[passed].at <= at {
            if let Some(mut cycle) = open.take() {
                let turn = &turns[passed];
                cycle.end = match (turn.end, length) {
                    (Some(end), _) => end.max(cycle.last),
                    (None, Some(length)) if !long => turn.at.min(cycle.start + length),
                    _ => turn.at,
                };
                out.push(cycle);
            }
            passed += 1;
        }
        if let (Some(length), false) = (length, long) {
            if open
                .as_ref()
                .is_some_and(|cycle| at >= cycle.start + length)
            {
                let mut cycle = open.take().unwrap();
                cycle.end = cycle.start + length;
                out.push(cycle);
            }
        }
        if open.is_none() && (long || used > 0.0) {
            let after = out.last().map_or(i64::MIN, |cycle| cycle.end);
            let start = if !long {
                // A five-hour cycle began a length before the reset it reports.
                reported
                    .zip(length)
                    .map(|(reset, length)| reset - length)
                    .filter(|start| *start <= at)
                    .unwrap_or(at)
            } else if passed > 0 {
                let turn = &turns[passed - 1];
                turn.end.or(turn.start).unwrap_or(turn.at)
            } else {
                mark_before(at).unwrap_or(at)
            };
            open = Some(Cycle::new(start.max(after).min(at)));
        }
        if let Some(cycle) = open.as_mut() {
            cycle.read(at, used);
        }
    }
    if let Some(mut cycle) = open {
        cycle.end = match (long, length) {
            (false, Some(length)) => cycle.start + length,
            _ => reset_at
                .filter(|reset| *reset > cycle.last)
                .or(length.map(|length| cycle.start + length))
                .unwrap_or(cycle.last)
                .max(cycle.last),
        };
        out.push(cycle);
    }
    match (long, length, reset_at) {
        (true, Some(length), Some(reset_at)) => split(out, samples, length, reset_at),
        _ => out,
    }
}

/// Long cycles cut at the window's resets counted back from `reset_at`, each
/// piece with the readings in it; pieces with none are left out.
fn split(cycles: Vec<Cycle>, samples: &[Reading], length: i64, reset_at: i64) -> Vec<Cycle> {
    let mut out = Vec::new();
    for cycle in cycles {
        if (cycle.end - cycle.start) as f64 <= 1.2 * length as f64 {
            out.push(cycle);
            continue;
        }
        let slack = length / 5;
        let mut edges = vec![cycle.start];
        let mut mark = reset_at - ((reset_at - cycle.start) / length) * length;
        while mark < cycle.end - slack {
            if mark > cycle.start + slack {
                edges.push(mark);
            }
            mark += length;
        }
        edges.push(cycle.end);
        for pair in edges.windows(2) {
            let mut piece = Cycle::new(pair[0]);
            piece.end = pair[1];
            for &(at, used, _) in samples
                .iter()
                .filter(|(at, _, _)| (pair[0]..pair[1]).contains(at))
            {
                piece.read(at, used);
            }
            if piece.readings > 0 {
                out.push(piece);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3_600_000;

    fn shares(cycles: &[Cycle]) -> Vec<(i64, i64, f64)> {
        cycles
            .iter()
            .map(|cycle| (cycle.start / H, cycle.end / H, cycle.used))
            .collect()
    }

    #[test]
    fn a_five_hour_window_has_a_cycle_from_its_first_use_to_its_length_or_reset() {
        let samples = [
            (0, 0.0),
            (H, 10.0),
            (2 * H, 40.0),
            (3 * H, 0.0),
            (4 * H, 0.0),
            (5 * H, 20.0),
            (11 * H, 30.0),
        ]
        .map(|(at, used)| (at, used, None));
        assert_eq!(
            shares(&cycles(&samples, Some(5 * H), None)),
            [(1, 3, 40.0), (5, 10, 20.0), (11, 16, 30.0)],
            "a reset ends the first, its length the second, and idle time is no cycle"
        );
    }

    #[test]
    fn a_weekly_window_runs_reset_to_reset_and_is_cut_at_its_weeks_where_it_stayed_put() {
        let week = 168 * H;
        let reset_at = 3 * week;
        let samples = [
            (10 * H, 30.0),
            (100 * H, 92.0),
            (120 * H, 0.0),
            (130 * H, 2.0),
            (week + 10 * H, 2.0),
            (2 * week + 10 * H, 2.0),
            (2 * week + 20 * H, 5.0),
        ]
        .map(|(at, used)| (at, used, None));
        let found = shares(&cycles(&samples, Some(week), Some(reset_at)));
        assert_eq!(
            found,
            [
                (0, 120, 92.0),
                (120, 168, 2.0),
                (168, 336, 2.0),
                (336, 504, 5.0)
            ],
            "an early reset, then a share that never fell, cut on the weekly marks"
        );
    }

    #[test]
    fn a_reported_reset_ends_a_cycle_exactly_even_when_the_share_hardly_fell() {
        // Five-hour windows: the first reset at 5 h, the next window opened
        // at 7 h (reset 12 h) and used only 2%.
        let samples = [
            (H, 4.0, Some(5 * H)),
            (3 * H, 3.0, Some(5 * H)),
            (8 * H, 2.0, Some(12 * H)),
        ];
        assert_eq!(
            shares(&cycles(&samples, Some(5 * H), None)),
            [(0, 5, 4.0), (7, 12, 2.0)],
            "each from a length before its reset to the reset itself"
        );
    }

    #[test]
    fn a_cycle_read_long_before_its_reset_may_have_used_more() {
        let mut cycle = Cycle::new(0);
        cycle.end = 168 * H;
        cycle.read(10 * H, 40.0);
        assert_eq!(cycle.json(1_000 * H)["partial"], true);
        assert_eq!(cycle.json(100 * H)["current"], true);
        cycle.read(167 * H, 60.0);
        assert_eq!(cycle.json(1_000 * H)["partial"], false);
        assert_eq!(cycle.json(1_000 * H)["used"], 60.0);
    }
}
