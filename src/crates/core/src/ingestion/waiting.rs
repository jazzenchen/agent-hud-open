//! The logs waiting to be read and when the next pass is due: what the
//! watcher and hooks point at, and the cheap look between passes.
use crate::{log_files, AgentHudCore};
use chrono::Utc;
use std::{collections::HashSet, path::PathBuf, time::Duration};

/// What a pass reads at most while no log waits, in bytes.
const PASS_BUDGET: u64 = 16 * 1024 * 1024;
/// What a pass reads at most while logs wait to be read.
const WAITING_BUDGET: u64 = 128 * 1024 * 1024;

impl super::Reads {
    /// How many bytes this pass may read.
    pub(crate) fn budget(&self) -> u64 {
        self.budget.unwrap_or(PASS_BUDGET)
    }
}

impl AgentHudCore {
    /// Whether logs are still waiting to be read: while indexing, the
    /// collector reads again every 2 s (agent-hud-native-open
    /// UsageRefresh.indexingInterval).
    pub fn indexing(&self) -> bool {
        self.waiting_reads() > 0
    }

    /// Logs still to be read, leaving out those pausing after a failed read,
    /// which are neither read nor waited for meanwhile.
    pub(crate) fn waiting_reads(&self) -> usize {
        let now = Utc::now().timestamp_millis();
        self.reads
            .pending
            .iter()
            .filter(|path| !self.reads.failed.pausing(path, now))
            .count()
    }

    /// Paces reading after a pass whose reading took `took`. While logs wait,
    /// the next pass may read twice as much after a quick pass and half as
    /// much after a slow one, so a pass spends a quarter to half a second
    /// reading on whatever computer this is, between 16 and 128 MB. Once none
    /// waits, a pass reads 16 MB at most.
    pub(crate) fn pace_reads(&mut self, took: Duration) {
        let budget = self.reads.budget();
        self.reads.budget = Some(if self.waiting_reads() == 0 {
            PASS_BUDGET
        } else if took < Duration::from_millis(250) {
            (budget * 2).min(WAITING_BUDGET)
        } else if took > Duration::from_millis(500) {
            (budget / 2).max(PASS_BUDGET)
        } else {
            budget
        });
    }

    pub fn file_changes(&mut self, paths: Option<HashSet<PathBuf>>) {
        self.reads.listing.note_changes(paths);
    }

    /// The cheap half of collection, between passes: a stat-only look at the
    /// logs clients hold open, whose writes can reach the watcher late, and at
    /// Claude Desktop's session files. Reads no log; true when a pass is due.
    pub fn look(&mut self) -> bool {
        let active = self.active_logs();
        let mut due = self.reads.listing.stale(&self.provider_roots, &active);
        if let Some((at, _)) = &self.reads.bot_poll {
            if Utc::now().timestamp_millis() >= *at {
                if let Some((_, path)) = self.reads.bot_poll.take() {
                    self.reads.pending.insert(path.clone());
                    self.reads.forced.insert(path);
                    due = true;
                }
            }
        }
        if due {
            // The pass rechecks the logs held open at once and reads them.
            self.reads.active_checked_at = None;
        }
        // Archiving a session in Claude Desktop changes no log. Its files are
        // stat-checked here and only the changed ones read again.
        if self.claude_desktop.index.refresh(&self.provider_roots) {
            self.usage_projection = None;
            due = true;
        }
        due
    }

    /// When a pass is due with no event to wake it: the next walk of every
    /// log folder, or the first retry of a log whose read failed.
    pub fn next_pass_due(&self) -> Option<std::time::Instant> {
        let now = Utc::now().timestamp_millis();
        let retry = self
            .reads
            .pending
            .iter()
            .filter_map(|path| self.reads.failed.retry_at(path))
            .min()
            .map(|at| {
                std::time::Instant::now()
                    + std::time::Duration::from_millis(u64::try_from(at - now).unwrap_or(0))
            });
        let bot = self.reads.bot_poll.as_ref().map(|(at, _)| {
            std::time::Instant::now()
                + std::time::Duration::from_millis(u64::try_from(at - now).unwrap_or(0))
        });
        [self.reads.listing.next_reconcile(), retry, bot]
            .into_iter()
            .flatten()
            .min()
    }

    /// Whether logs a hook pointed at still wait to be read.
    pub fn has_reminded_reads(&self) -> bool {
        !self.reads.forced.is_empty()
    }

    pub fn remind_paths(&mut self, paths: Vec<PathBuf>) -> usize {
        let mut accepted = HashSet::new();
        for path in paths {
            let Ok(real) = path.canonicalize() else {
                continue;
            };
            for (root, source) in &self.provider_roots {
                let Ok(real_root) = root.canonicalize() else {
                    continue;
                };
                if (real == real_root || real.starts_with(&real_root))
                    && log_files::accepts_source(&real, source)
                {
                    let mapped = if root.is_file() {
                        root.clone()
                    } else {
                        match real.strip_prefix(&real_root) {
                            Ok(tail) => root.join(tail),
                            Err(_) => continue,
                        }
                    };
                    self.reads.pending.insert(mapped.clone());
                    self.reads.forced.insert(mapped.clone());
                    accepted.insert(mapped);
                    break;
                }
            }
        }
        accepted.len()
    }
}
