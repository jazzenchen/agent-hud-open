//! Each Claude Code session's share of the 5-hour window.
use super::*;

impl crate::AgentHudCore {
    /// Each Claude Code session's share of the 5-hour window: its tokens since
    /// the window began over every log's, times the window's use. Tokens are
    /// Open's tokens in plus out (UsageLedger.tokens): input with cache
    /// writes, output with reasoning, and no cache reads.
    pub(crate) fn claude_window_shares(&self, now: i64) -> HashMap<PathBuf, f64> {
        let session_quota = self.accounts.quotas.iter().find(|q| {
            q["vendor"] == "claude"
                && (q["id"]
                    .as_str()
                    .is_some_and(|id| id.ends_with(":claude-session") || id == "claude-session")
                    || q["name"] == "Current session")
        });
        let utilization = session_quota
            .and_then(|q| q["usedPercent"].as_f64())
            .filter(|v| *v > 0.0)
            .unwrap_or(0.0);
        if utilization <= 0.0 {
            return HashMap::new();
        }
        let window_start = session_quota
            .and_then(|q| q["resetAt"].as_str())
            .and_then(metering::date_ms)
            .map(|reset| reset - 5 * 3_600_000)
            .unwrap_or(now - 5 * 3_600_000);
        let mut by_path = HashMap::<PathBuf, u64>::new();
        if let Some(db) = &self.database {
            // Without the hint SQLite walks the whole table in contribution
            // order to skip a GROUP BY sort, reading every event ever kept
            // to find the few in this window.
            if let Ok(mut stmt) = db.prepare_cached(
                "SELECT contribution, COALESCE(SUM(input + cache_write + output + reasoning), 0)
                 FROM usage_meter_event INDEXED BY usage_meter_event_date
                 WHERE source = 'claude' AND at_ms >= ?1
                 GROUP BY contribution",
            ) {
                if let Ok(rows) = stmt.query_map([window_start], |row| {
                    Ok((
                        PathBuf::from(row.get::<_, String>(0)?),
                        row.get::<_, i64>(1)?.max(0) as u64,
                    ))
                }) {
                    for row in rows.flatten() {
                        if row.1 > 0 {
                            by_path.insert(row.0, row.1);
                        }
                    }
                }
            }
        }
        if by_path.is_empty() {
            for log in self.logs.values() {
                if log.source == "claude"
                    && !log.gone
                    && log.updated_at.is_some_and(|at| at >= window_start)
                {
                    let tokens =
                        log.usage.input + log.usage.write + log.usage.output + log.usage.reasoning;
                    if tokens > 0 {
                        by_path.insert(log.path.clone(), tokens);
                    }
                }
            }
        }
        let total: u64 = by_path.values().sum();
        if total == 0 {
            return HashMap::new();
        }
        by_path
            .into_iter()
            .filter_map(|(path, tokens)| {
                let share = tokens as f64 / total as f64;
                (share > 0.0).then_some((path, share * utilization))
            })
            .collect()
    }
}
