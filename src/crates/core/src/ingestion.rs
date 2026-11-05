//! Incremental transcript streaming, `session_mapping` resolution, and
//! timestamp-driven (`updated_at_ms`) SQLite synchronization.
use crate::{
    checkpoints::{self, LogHead},
    compressed,
    log_files::FileStamp,
    metering, source_db, AgentHudCore, Log, PARSER_VERSION,
};
use chrono::{DateTime, Utc};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

mod lines;
mod scan;
mod sync;
mod waiting;

pub(crate) use lines::{line, read_lines};

/// Logs whose last read failed. Each is tried again only after a pause that
/// doubles with every failure in a row, from 30 s up to the 300 s account
/// interval; what was read from it before stays in place, and its failure is
/// still reported while it pauses (agent-hud-native-open FailedReads).
/// The logs waiting to be read, and how the listing finds them.
#[derive(Default)]
pub(crate) struct Reads {
    pub(crate) listing: crate::LogFiles,
    pub(crate) active_checked_at: Option<std::time::Instant>,
    pub(crate) pending: HashSet<PathBuf>,
    /// Logs whose last read failed, waiting out a pause before the next try.
    pub(crate) failed: FailedReads,
    /// Logs a client's hook pointed at: read even when their size and time
    /// look unchanged.
    pub(crate) forced: HashSet<PathBuf>,
    /// While a Grok Bot turn works, when its roster is read again (ms) and
    /// which file: the client says nothing when a turn ends, so quiet is only
    /// seen by looking again.
    pub(crate) bot_poll: Option<(i64, PathBuf)>,
}

#[derive(Default)]
pub(crate) struct FailedReads {
    failures: HashMap<PathBuf, (u32, i64, String)>,
}

impl FailedReads {
    const FIRST_PAUSE_MS: i64 = 30_000;
    const LONGEST_PAUSE_MS: i64 = 300_000;

    /// When a log whose read failed may be tried again, in ms.
    pub(crate) fn retry_at(&self, path: &Path) -> Option<i64> {
        self.failures.get(path).map(|(_, retry, _)| *retry)
    }
    pub(crate) fn pausing(&self, path: &Path, now: i64) -> bool {
        self.failures
            .get(path)
            .is_some_and(|(_, retry, _)| now < *retry)
    }
    fn reason(&self, path: &Path) -> Option<&str> {
        self.failures
            .get(path)
            .map(|(_, _, reason)| reason.as_str())
    }
    pub(crate) fn failed(&mut self, path: &Path, reason: String, now: i64) {
        let count = self.failures.get(path).map_or(0, |(count, _, _)| *count) + 1;
        let pause = (Self::FIRST_PAUSE_MS << (count - 1).min(16)).min(Self::LONGEST_PAUSE_MS);
        self.failures
            .insert(path.to_path_buf(), (count, now + pause, reason));
    }
    pub(crate) fn succeeded(&mut self, path: &Path) {
        self.failures.remove(path);
    }
    /// Forgets the files that are no longer listed.
    fn keep(&mut self, listed: &HashMap<PathBuf, FileStamp>) {
        self.failures.retain(|path, _| listed.contains_key(path));
    }
}

/// Whether a listed log is read now: one last written within the ledger's
/// 31-day retention that changed since it was read, was never read, or was
/// read by another reader version or only partly. An older log holds no usage
/// the ledger keeps and is not read, even when an earlier version read it;
/// what it counted stays (agent-hud-native-open TailLogStore.index: logs modified
/// since the history cutoff, and older ones outdated or unfinished within the
/// retention). The Swift app also leaves unread a log 7 to 31 days old it
/// never read; this app reads it, so a first launch fills the 30-day token history.
pub(crate) fn due(log: Option<&LogHead>, file: &FileStamp, now_ms: i64) -> bool {
    within_retention(file, now_ms) && needs_read(log, file)
}

/// Whether a log was last written within the ledger's retention; one whose
/// time cannot be told counts as recent.
pub(crate) fn within_retention(file: &FileStamp, now_ms: i64) -> bool {
    file.modified.is_none_or(|modified| {
        DateTime::<Utc>::from(modified).timestamp_millis() >= now_ms - metering::RETENTION_MS
    })
}

/// Records a Claude Code log's calls under the route its session's hook
/// said they go through (`pricing::claude_route`), a sub-agent's log under its
/// session's; a local server's reported window becomes the calls' window.
pub(crate) fn route_claude_calls(
    db: &rusqlite::Connection,
    log: &Log,
    events: &mut [agent_hud_agents::transcript::metering::Event],
) -> rusqlite::Result<()> {
    let mut endpoint = crate::database::endpoints::find(db, "claude", &log.id)?;
    if endpoint.is_none() {
        if let Some(parent) = log.live.parent_id.as_deref() {
            endpoint = crate::database::endpoints::find(db, "claude", parent)?;
        }
    }
    let Some(route) = endpoint.as_ref().and_then(crate::pricing::claude_route) else {
        return Ok(());
    };
    let window = endpoint
        .as_ref()
        .and_then(|endpoint| endpoint["contextWindow"].as_u64())
        .filter(|_| crate::pricing::is_local_route(&route));
    for event in events.iter_mut().filter(|event| !event.model.contains('#')) {
        event.context_window = event.context_window.or(window);
        event.model = format!("{}#{route}", event.model);
    }
    Ok(())
}

pub(crate) fn needs_read(log: Option<&LogHead>, file: &FileStamp) -> bool {
    log.is_none_or(|log| {
        log.gone
            || log.parser_version != PARSER_VERSION
            || log.reader_version != source_db::reader_version(&log.path, &log.source)
            || log.modified != file.modified
            || log.length != file.length
            || if compressed::is_archive(&log.path) {
                !log.decoded_complete
            } else {
                log.offset < log.length && !log.waiting_for_more
            }
    })
}

impl AgentHudCore {
    pub(crate) fn next_timestamp_ms(&mut self) -> i64 {
        Utc::now()
            .timestamp_millis()
            .max(self.last_synced_at_ms + 1)
    }

    /// Reads the logs a hook reminded of, without rendering.
    pub(crate) fn read_reminded_logs(&mut self) {
        let paths = self.reads.forced.clone();
        if !paths.is_empty() {
            if let Err(error) = self.scan(Some(paths)) {
                self.source_notice = Some(error);
            }
            if self.usage_projection.is_none() {
                self.read_projection_sources();
            }
        }
    }

    /// The provider indexes the session projection joins: Claude Desktop's
    /// archive and navigation index, Claude Code's background jobs, and
    /// WorkBuddy's metering database. Only a collection pass reads them;
    /// queries and settings changes reuse them.
    pub(crate) fn read_projection_sources(&mut self) {
        self.projection_sources = self.projection_sources.wrapping_add(1);
        self.claude_desktop.index.refresh(&self.provider_roots);
        // A hook can bring this pass on without a collection: the jobs are
        // placed by the processes as they are now.
        self.claude_desktop
            .processes
            .refresh(&self.provider_roots, crate::snapshot::wall_clock_ms());
        self.claude_desktop.jobs = crate::claude_jobs::ClaudeJobs::read(
            &self.provider_roots,
            &self.claude_desktop.processes,
        );
        for head in self.logs.values_mut() {
            // The demo's logs are archived or not as the sample has them.
            if head.source == "claude" && !crate::demo::is_demo(&head.path) {
                head.archived = self.claude_desktop.index.archived.contains(&head.id);
            }
        }
        let metering = {
            let logs = self.selected();
            crate::clients::workbuddy::read_workbuddy_metering(&logs, &self.provider_roots)
        };
        self.workbuddy_metering = metering;
    }

    /// The logs a client most likely holds open, whose writes can reach the
    /// watcher late: the 64 written to most recently in the last day, and
    /// Codex's index. Read from memory; stat-checked by passes and looks.
    pub(crate) fn active_logs(&self) -> Vec<PathBuf> {
        let cutoff = Utc::now().timestamp_millis() - 86_400_000;
        let mut recent: Vec<(&PathBuf, i64)> = self
            .logs
            .values()
            .filter(|log| !log.gone)
            .filter_map(|log| {
                // A session that just began may have been read before its
                // first line was whole, so it has no event yet. Its client can
                // hold the log open, and macOS then reports the writes tens of
                // seconds late (a VS Code Codex rollout, 2026-10-26), so its
                // last write stands in for an event until one is read.
                let begun = (log.waiting_for_more || !log.observed)
                    .then_some(log.modified)
                    .flatten()
                    .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
                    .and_then(|at| i64::try_from(at.as_millis()).ok());
                log.live
                    .last_event()
                    .or(begun)
                    .filter(|at| *at >= cutoff)
                    .map(|at| (&log.path, at))
            })
            .collect();
        recent.sort_unstable_by_key(|(_, at)| std::cmp::Reverse(*at));
        let mut paths = recent
            .into_iter()
            .take(64)
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        paths.extend(
            self.provider_roots
                .iter()
                .filter(|(_, source)| *source == "codex-index")
                .map(|(path, _)| path.clone()),
        );
        paths
    }

    /// Works out the list cost of up to `limit` logs saved before it was kept.
    fn price_unpriced_logs(&mut self, limit: usize) {
        let Some(database) = &self.database else {
            return;
        };
        let unpriced = self
            .logs
            .iter()
            .filter(|(_, head)| !head.list_cost_priced || head.unpriced_tokens.is_none())
            .map(|(path, _)| path.clone())
            .take(limit)
            .collect::<Vec<_>>();
        let now = Utc::now().timestamp_millis();
        for path in unpriced {
            let key = path.to_string_lossy();
            let Some(head) = self.logs.get_mut(&path) else {
                continue;
            };
            if head.price(database, &key).is_err() {
                continue;
            }
            if let Ok(encoded) = serde_json::to_string(&*head) {
                let _ = checkpoints::update_head(database, &key, &encoded, now);
            }
        }
    }

    /// Up to when a pending log's usage is known: its last update when only
    /// the parser changed, else its read's target.
    pub(crate) fn pending_meter_bound(&self, path: &Path, file: &FileStamp) -> Option<i64> {
        self.logs.get(path).and_then(|log| {
            if log.parser_version != PARSER_VERSION
                && log.length == file.length
                && log.modified == file.modified
            {
                metering::date_ms(&log.updated)
            } else {
                log.meter_target_at
            }
        })
    }

    pub(crate) fn pending_meter_bounds(&self) -> HashMap<&str, Option<i64>> {
        let mut bounds = HashMap::<&str, Option<i64>>::new();
        for path in &self.reads.pending {
            let Some(file) = self.reads.listing.files.get(path) else {
                continue;
            };
            let latest = self.pending_meter_bound(path, file);
            bounds
                .entry(file.source)
                .and_modify(|current| {
                    *current = match (*current, latest) {
                        (Some(a), Some(b)) => Some(a.max(b)),
                        _ => None,
                    };
                })
                .or_insert(latest);
        }
        bounds
    }
}
