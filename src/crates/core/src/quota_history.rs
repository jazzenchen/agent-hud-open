//! The quota windows' readings over a stretch of the last 30 days, for the
//! Statistics window's quota chart: each window's used share in runs drawn
//! without a break, thinned to a point or two a bin, and the resets between
//! its cycles. One window is read at a time by its key, never the whole
//! table, and only when a page asks.
use super::*;
use rusqlite::params;

/// The furthest a history reaches back: what `quota_sample` keeps.
const KEPT_MS: i64 = 30 * 86_400_000;
/// About how many bins a history is thinned to, whatever its length.
const BINS: i64 = 200;
/// The shortest bin.
const MIN_BIN_MS: i64 = 60_000;
/// Readings further apart than this are a gap, unless the share stayed put
/// within one window length: nobody read the window in between, so no line
/// is drawn across it.
const GAP_MS: i64 = 20 * 60_000;
/// A fall in the used share of this many points is a reset, as
/// `View::metrics` splits a cycle's readings.
const RESET_FALL: f64 = 5.0;

impl AgentHudCore {
    pub(super) fn quota_history(&mut self, params: Value) -> Result<Value, String> {
        let date = |key: &str| -> Result<Option<i64>, String> {
            match params.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(value) => value
                    .as_i64()
                    .or_else(|| value.as_str().and_then(metering::date_ms))
                    .filter(|at| *at >= 0)
                    .map(Some)
                    .ok_or_else(|| {
                        format!(
                            "quota_history.{key} must be an RFC3339 timestamp or epoch milliseconds"
                        )
                    }),
            }
        };
        let to = date("to")?.unwrap_or_else(|| Utc::now().timestamp_millis());
        let from = date("from")?.unwrap_or(to - 86_400_000);
        if to <= from {
            return Err("quota_history.from must be before quota_history.to".into());
        }
        let from = from.max(to - KEPT_MS);
        let bin = ((to - from) / BINS).max(MIN_BIN_MS);
        let db = self
            .database
            .as_ref()
            .ok_or("Quota history is unavailable")?;
        let read = |id: &str| -> rusqlite::Result<Vec<(i64, f64)>> {
            // The reading before the stretch, so a line can enter it from the
            // left, then the readings in it, by the table's own key.
            let mut before = db.prepare_cached(
                "SELECT observed_at_ms,remaining FROM quota_sample
                 WHERE window_id=?1 AND observed_at_ms<?2
                 ORDER BY observed_at_ms DESC LIMIT 1",
            )?;
            let mut within = db.prepare_cached(
                "SELECT observed_at_ms,remaining FROM quota_sample
                 WHERE window_id=?1 AND observed_at_ms>=?2 AND observed_at_ms<=?3
                 ORDER BY observed_at_ms",
            )?;
            let used = |r: &rusqlite::Row<'_>| {
                Ok((
                    r.get::<_, i64>(0)?,
                    (100.0 - r.get::<_, f64>(1)?).clamp(0.0, 100.0),
                ))
            };
            let mut samples = before
                .query_map(params![id, from], used)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for sample in within.query_map(params![id, from, to], used)? {
                samples.push(sample?);
            }
            Ok(samples)
        };
        let mut windows = Vec::new();
        for row in self
            .accounts
            .quotas
            .iter()
            .filter(|row| self.shown(row, "vendor"))
        {
            let Some(id) = row["id"].as_str() else {
                continue;
            };
            let samples = read(id).map_err(|_| {
                l10n::text(
                    "无法从本地数据库读取额度记录",
                    "Quota history could not be read from SQLite",
                )
            })?;
            let length = row["windowDurationSeconds"]
                .as_f64()
                .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
                .map(|seconds| (seconds * 1000.0) as i64);
            let reset = row["resetAt"].as_str().and_then(metering::date_ms);
            let cycle_start = reset.zip(length).map(|(reset, length)| reset - length);
            let (segments, resets) = history(&samples, from, to, bin, cycle_start, length);
            windows.push(json!({
                "id": id,
                "vendor": row["vendor"],
                "accountId": row["accountId"],
                "account": row["account"],
                "name": row["name"],
                "shortLabel": row["shortLabel"],
                "windowDurationSeconds": length.map(|length| length / 1000),
                "resetAt": row["resetAt"],
                "segments": segments
                    .iter()
                    .map(|points| points
                        .iter()
                        .map(|(at, used)| json!([at, (used * 100.0).round() / 100.0]))
                        .collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
                "resets": resets,
            }));
        }
        naming::window_names::settle_short_names(&mut windows);
        Ok(json!({
            "kind": "quota_history",
            "generatedAt": Utc::now().to_rfc3339(),
            "from": DateTime::<Utc>::from_timestamp_millis(from).map(|at| at.to_rfc3339()),
            "to": DateTime::<Utc>::from_timestamp_millis(to).map(|at| at.to_rfc3339()),
            "binSeconds": bin / 1000,
            "windows": windows,
        }))
    }
}

/// Where a new cycle began between two readings of a window, (time, used
/// share) one after the other: the current cycle's start (`cycle_start`)
/// when it passed between them, else the later reading's time when the
/// used share fell by `RESET_FALL` points, the earliest the reset is known
/// to have happened; none within one cycle.
pub(crate) fn reset_between(
    previous: (i64, f64),
    next: (i64, f64),
    cycle_start: Option<i64>,
) -> Option<i64> {
    cycle_start
        .filter(|start| (previous.0 + 1..=next.0).contains(start))
        .or((previous.1 - next.1 >= RESET_FALL).then_some(next.0))
}

/// A window's readings, oldest first, as (time, used share), drawn from
/// `from` to `to`: the runs a line joins, each thinned to its first reading
/// and the last of every `bin`, and the resets that start a new cycle in
/// the stretch. Two readings are joined unless a reset falls between them,
/// the used share falling by `RESET_FALL` points or the current cycle's
/// start (`cycle_start`) passing, or they are more than `GAP_MS` apart
/// with the share moved or a whole window length (`length`, unknown counts
/// as none) between them. Within one cycle the share only grows, so the
/// same share at both ends of a gap shorter than a window is what it was
/// throughout. A reset found only by the fall is placed at the first
/// reading of the new cycle, the earliest it is known to have happened.
fn history(
    samples: &[(i64, f64)],
    from: i64,
    to: i64,
    bin: i64,
    cycle_start: Option<i64>,
    length: Option<i64>,
) -> (Vec<Vec<(i64, f64)>>, Vec<i64>) {
    let mut runs = Vec::<Vec<(i64, f64)>>::new();
    let mut resets = Vec::new();
    let mut run = Vec::new();
    for (index, sample) in samples.iter().copied().enumerate() {
        if let Some(previous) = index.checked_sub(1).map(|index| samples[index]) {
            let reset = reset_between(previous, sample, cycle_start);
            let apart = sample.0 - previous.0;
            let gap = apart > GAP_MS
                && ((sample.1 - previous.1).abs() >= 0.01
                    || length.is_none_or(|length| apart >= length));
            if let Some(at) = reset {
                if (from..=to).contains(&at) {
                    resets.push(at);
                }
            }
            if reset.is_some() || gap {
                runs.push(std::mem::take(&mut run));
            }
        }
        run.push(sample);
    }
    runs.push(run);
    // A reset the current cycle started at, read before or after the
    // stretch's first reading, is still a boundary in it.
    if let Some(start) = cycle_start.filter(|start| (from..=to).contains(start)) {
        if !resets.contains(&start) {
            resets.push(start);
            resets.sort_unstable();
        }
    }
    let segments = runs
        .into_iter()
        // A run wholly before the stretch, the reading that leads into it
        // alone, has nothing to draw.
        .filter(|run| run.last().is_some_and(|last| last.0 >= from))
        .map(|run| {
            let mut points = vec![run[0]];
            for (index, point) in run.iter().enumerate().skip(1) {
                let last_of_bin = run
                    .get(index + 1)
                    .is_none_or(|next| next.0.div_euclid(bin) != point.0.div_euclid(bin));
                if last_of_bin {
                    points.push(*point);
                }
            }
            points
        })
        .collect();
    (segments, resets)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;
    const MINUTE: i64 = 60_000;

    #[test]
    fn readings_join_within_a_cycle_and_part_at_resets_and_gaps() {
        let t = 1_800_000_000_000i64;
        let samples = [
            (t, 10.0),
            (t + 5 * MINUTE, 12.0),
            // A reset: the share falls by more than five points.
            (t + 10 * MINUTE, 1.0),
            (t + 15 * MINUTE, 3.0),
            // Agent HUD was not running and the share moved: a gap.
            (t + 3 * HOUR, 20.0),
            (t + 3 * HOUR + 5 * MINUTE, 22.0),
            // Quiet for hours at the same share, within one window: joined.
            (t + 7 * HOUR, 22.0),
        ];
        let (segments, resets) =
            history(&samples, t, t + 8 * HOUR, MINUTE, None, Some(5 * 24 * HOUR));
        assert_eq!(
            segments,
            vec![
                vec![(t, 10.0), (t + 5 * MINUTE, 12.0)],
                vec![(t + 10 * MINUTE, 1.0), (t + 15 * MINUTE, 3.0)],
                vec![
                    (t + 3 * HOUR, 20.0),
                    (t + 3 * HOUR + 5 * MINUTE, 22.0),
                    (t + 7 * HOUR, 22.0)
                ],
            ]
        );
        assert_eq!(resets, [t + 10 * MINUTE]);
        // Without a known window length, the same quiet hours are a gap.
        let (segments, _) = history(&samples, t, t + 8 * HOUR, MINUTE, None, None);
        assert_eq!(segments.len(), 4);
    }

    #[test]
    fn the_current_cycles_start_is_a_reset_and_bins_keep_each_cycles_peak() {
        let t = 1_800_000_000_000i64;
        // Every minute for two hours, the share rising a point every quarter
        // hour; the current cycle started an hour in, from 3%: too small a
        // fall to tell a reset by.
        let samples = (0..120)
            .map(|minute| {
                let used = if minute < 60 {
                    (minute / 15) as f64
                } else {
                    ((minute - 60) / 15) as f64
                };
                (t + minute * MINUTE, used)
            })
            .collect::<Vec<_>>();
        let start = t + 60 * MINUTE;
        let (segments, resets) = history(
            &samples,
            t,
            t + 2 * HOUR,
            30 * MINUTE,
            Some(start - 30_000),
            Some(5 * HOUR),
        );
        assert_eq!(resets, [start - 30_000]);
        assert_eq!(
            segments,
            vec![
                // The first reading, then the last of each half hour: the
                // last before the reset is the cycle's peak.
                vec![(t, 0.0), (t + 29 * MINUTE, 1.0), (t + 59 * MINUTE, 3.0)],
                vec![
                    (start, 0.0),
                    (t + 89 * MINUTE, 1.0),
                    (t + 119 * MINUTE, 3.0)
                ],
            ]
        );
    }

    #[test]
    fn the_reading_before_the_stretch_leads_a_line_into_it() {
        let t = 1_800_000_000_000i64;
        let from = t + HOUR;
        let (segments, _) = history(
            &[(t, 30.0), (t + 2 * HOUR, 30.0)],
            from,
            t + 3 * HOUR,
            MINUTE,
            None,
            Some(7 * 24 * HOUR),
        );
        assert_eq!(segments, vec![vec![(t, 30.0), (t + 2 * HOUR, 30.0)]]);
        // Alone, with the share moved by the next reading, it draws nothing.
        let (segments, _) = history(
            &[(t, 30.0), (t + 2 * HOUR, 40.0)],
            from,
            t + 3 * HOUR,
            MINUTE,
            None,
            Some(7 * 24 * HOUR),
        );
        assert_eq!(segments, vec![vec![(t + 2 * HOUR, 40.0)]]);
    }
}
