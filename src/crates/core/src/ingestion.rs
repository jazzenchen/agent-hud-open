//! Incremental transcript streaming, `session_mapping` resolution, and
//! timestamp-driven (`updated_at_ms`) SQLite synchronization.
use crate::l10n;
use crate::{
    carried_usage,
    checkpoints::{self, LogHead},
    compressed, log_files,
    log_files::FileStamp,
    metering, requests, source_batches, source_db, AgentHudCore, Log, TimelineBucket, Usage,
    UsageProjection, MAX_LINE, PARSER_VERSION,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, File},
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

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

pub(crate) fn read_lines(
    log: &mut Log,
    reader: &mut dyn BufRead,
    budget: &mut u64,
    requests: &requests::Requests,
    errors: &mut Vec<String>,
    archive: bool,
    mut conversation: Option<&mut crate::conversation_collect::Gathered>,
) -> std::io::Result<()> {
    let mut page = 2 * 1024 * 1024u64;
    log.waiting_for_more = false;
    while *budget > 0 && page > 0 {
        let (bytes, skeleton, consumed, complete) = line_with_skeleton(reader)?;
        if consumed == 0 {
            log.decoded_complete = archive;
            break;
        }
        if !complete {
            log.waiting_for_more = true;
            log.decoded_complete = archive;
            break;
        }
        log.meter_line_start = log.offset;
        log.offset += consumed;
        *budget = budget.saturating_sub(consumed);
        page = page.saturating_sub(consumed);
        let bytes = if bytes.len() > MAX_LINE {
            match skeleton {
                Some(skeleton) => skeleton,
                None => {
                    errors.push(format!(
                        "{} contains an oversized record; usage may be incomplete",
                        log.source
                    ));
                    continue;
                }
            }
        } else {
            bytes
        };
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => {
                let before = conversation
                    .is_some()
                    .then(|| crate::conversation_collect::before(log));
                log.parse(&value);
                if let (Some(facts), Some(before)) = (conversation.as_deref_mut(), &before) {
                    crate::conversation_collect::line(
                        log,
                        &value,
                        log.meter_line_start,
                        before,
                        facts,
                    );
                }
                if log.source == "codex" && value["type"] == "turn_context" {
                    if let Some(turn) = value["payload"]["turn_id"].as_str() {
                        let now = Utc::now().timestamp_millis();
                        let at = value["timestamp"]
                            .as_str()
                            .and_then(metering::date_ms)
                            .unwrap_or(now);
                        requests.note_reviewer(
                            "codex",
                            &log.id,
                            turn,
                            value["payload"]["approvals_reviewer"]
                                .as_str()
                                .map(str::to_owned),
                            at,
                            now,
                        );
                    }
                }
            }
            Err(_) => errors.push(format!(
                "{} contains an invalid JSON record; usage may be incomplete",
                log.source
            )),
        }
    }
    Ok(())
}

/// Whether a listed log is read now: one last written within the ledger's
/// 31-day retention that changed since it was read, was never read, or was
/// read by another reader version or only partly. An older log holds no usage
/// the ledger keeps and is not read, even when an earlier version read it;
/// what it counted stays (agent-hud-native-open TailLogStore.index: logs modified
/// since the history cutoff, and older ones outdated or unfinished within the
/// retention). Open also leaves unread a log 7 to 31 days old it never read;
/// Next reads it, so a first launch fills the 30-day token history.
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

pub(crate) fn line(reader: &mut (impl BufRead + ?Sized)) -> std::io::Result<(Vec<u8>, u64, bool)> {
    let (bytes, _, consumed, complete) = line_with_skeleton(reader)?;
    Ok((bytes, consumed, complete))
}

/// One line, its first `MAX_LINE + 1` bytes kept; for a longer line also its
/// skeleton: the same JSON with every string over 4 KiB left empty, so a
/// record whose tool output, image or file content runs to megabytes still
/// counts its usage and closes its call (agent-hud-native-open parses such lines by
/// byte search and has no line limit). None when the line is not longer, or
/// its skeleton would itself pass 8 MiB.
pub(crate) fn line_with_skeleton(
    reader: &mut (impl BufRead + ?Sized),
) -> std::io::Result<(Vec<u8>, Option<Vec<u8>>, u64, bool)> {
    let mut out = Vec::new();
    let mut skeleton: Option<Skeleton> = None;
    let mut consumed = 0;
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return Ok((out, None, consumed, false));
        }
        let count = chunk
            .iter()
            .position(|&byte| byte == b'\n')
            .map(|index| index + 1)
            .unwrap_or(chunk.len());
        let complete = chunk[count - 1] == b'\n';
        let piece = &chunk[..count];
        if let Some(skeleton) = &mut skeleton {
            skeleton.feed(piece);
        } else if out.len() + count > MAX_LINE {
            let mut started = Skeleton::default();
            started.feed(&out);
            started.feed(piece);
            skeleton = Some(started);
        }
        if out.len() <= MAX_LINE {
            let remaining = (MAX_LINE + 1 - out.len()).min(count);
            out.extend_from_slice(&chunk[..remaining]);
        }
        consumed += count as u64;
        reader.consume(count);
        if complete {
            let skeleton = skeleton
                .filter(|_| out.len() > MAX_LINE)
                .and_then(Skeleton::finish);
            return Ok((out, skeleton, consumed, true));
        }
    }
}

/// A JSON line copied with its long strings left empty (`line_with_skeleton`).
#[derive(Default)]
struct Skeleton {
    out: Vec<u8>,
    in_string: bool,
    escape: bool,
    /// Where the open string's content starts in `out`.
    start: usize,
    dropping: bool,
    overflow: bool,
}

impl Skeleton {
    const STRING_LIMIT: usize = 4 * 1024;
    const LIMIT: usize = 8 * 1024 * 1024;

    fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.overflow {
                return;
            }
            if self.in_string {
                if self.escape {
                    self.escape = false;
                } else if byte == b'\\' {
                    self.escape = true;
                } else if byte == b'"' {
                    self.in_string = false;
                    self.out.push(b'"');
                    continue;
                }
                if !self.dropping {
                    if self.out.len() - self.start < Self::STRING_LIMIT {
                        self.out.push(byte);
                    } else {
                        self.out.truncate(self.start);
                        self.dropping = true;
                    }
                }
            } else if byte == b'"' {
                self.in_string = true;
                self.dropping = false;
                self.out.push(b'"');
                self.start = self.out.len();
            } else if !byte.is_ascii_whitespace() {
                self.out.push(byte);
            }
            if self.out.len() > Self::LIMIT {
                self.overflow = true;
            }
        }
    }

    fn finish(self) -> Option<Vec<u8>> {
        (!self.overflow && !self.in_string).then_some(self.out)
    }
}

impl AgentHudCore {
    pub(crate) fn next_timestamp_ms(&mut self) -> i64 {
        Utc::now()
            .timestamp_millis()
            .max(self.last_synced_at_ms + 1)
    }

    /// Synchronize in-memory state from SQLite using `updated_at_ms > last_synced_at_ms`.
    pub(crate) fn sync_from_database(&mut self) -> Result<(), String> {
        self.sync_holding(&[])
    }

    /// `sync_from_database` after this run wrote the logs in `held`: their
    /// heads are in memory and the meter view reads them again alone, so their
    /// rows are not read back and keep the views.
    pub(crate) fn sync_holding(&mut self, held: &[&str]) -> Result<(), String> {
        let Some(db) = &self.database else {
            return Ok(());
        };
        let changes = checkpoints::sync_since(db, self.last_synced_at_ms, held)?;
        for (path, mut head) in changes.heads {
            // Turn text lives only in memory: a head read back takes it from
            // the one this run holds.
            if let Some(earlier) = self.logs.get(&path) {
                head.keep_words(earlier);
            }
            self.logs.insert(path, head);
        }
        for (key, canonical_path) in changes.mappings {
            self.session_map.insert(key, canonical_path);
        }
        self.last_synced_at_ms = changes.max_updated_at_ms;
        if changes.changed {
            self.usage_projection = None;
            self.meters.view = None;
        }
        Ok(())
    }

    fn adopt_mapped_session(
        &mut self,
        path: &Path,
        log: &mut Log,
    ) -> Result<Option<PathBuf>, String> {
        if log.source == "grok-cli" && log.model == "Unknown" {
            if let Some(db) = &self.database {
                if let Ok(models) = source_batches::grok_models(db, &log.id) {
                    if let Some(model) = models.values().next() {
                        log.model = model.clone();
                        for event in log.meter_events.values_mut() {
                            if event.model.is_empty() || event.model == "Unknown" {
                                event.model = model.clone();
                            }
                        }
                    }
                }
            }
        }
        let map_key = (log.source.clone(), log.id.clone());
        let Some(old_path) = self.session_map.get(&map_key).cloned() else {
            return Ok(None);
        };
        if old_path == path || !old_path.to_string_lossy().contains("#hud-session-") {
            return Ok(None);
        }
        if !self.logs.contains_key(&old_path) {
            return Ok(None);
        }
        let (old_log, _) = self.load_parser(&old_path)?;
        if old_log.source_state["hasInference"] == true {
            log.messages = old_log.messages;
            log.usage = old_log.usage;
            log.turns = old_log.turns;
            log.buckets = old_log.buckets;
            log.meter_events.clear();
            log.source_state["hasInference"] = json!(true);
            if old_log.source_state["hasOtel"] == true {
                log.source_state["hasOtel"] = json!(true);
            }
            if log.model == "Unknown" && old_log.model != "Unknown" {
                log.model = old_log.model;
            }
            if old_log.updated > log.updated {
                log.updated = old_log.updated;
            }
        }
        Ok(Some(old_path))
    }

    pub(crate) fn scan(&mut self) -> Result<(), String> {
        self.scan_impl(None)
    }

    pub fn read_reminded(&mut self) -> Result<Value, String> {
        self.read_reminded_logs();
        self.render_snapshot()
    }

    /// Reads the logs a hook reminded of, without rendering.
    pub(crate) fn read_reminded_logs(&mut self) {
        let paths = self.reads.forced.clone();
        if !paths.is_empty() {
            if let Err(error) = self.scan_impl(Some(paths)) {
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
        self.claude_desktop.jobs = crate::claude_jobs::ClaudeJobs::read(&self.provider_roots);
        for head in self.logs.values_mut() {
            if head.source == "claude" {
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
                log.live
                    .last_event()
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

    pub(crate) fn scan_impl(&mut self, only: Option<HashSet<PathBuf>>) -> Result<(), String> {
        let reminder_only = only.is_some();
        let mut errors = self.storage_issue.iter().cloned().collect::<Vec<_>>();
        let mut budget: u64 = 16 * 1024 * 1024;
        let handovers = if let Some(paths) = &only {
            for path in paths {
                let Some((_, source)) = self
                    .provider_roots
                    .iter()
                    .find(|(root, _)| path.starts_with(root))
                else {
                    continue;
                };
                match fs::metadata(path) {
                    Ok(metadata) if metadata.is_file() => {
                        self.reads
                            .listing
                            .files
                            .insert(path.clone(), log_files::stamp(path, source, &metadata));
                    }
                    Ok(_) => {}
                    Err(_) => {
                        self.reads.forced.remove(path);
                        errors.push(format!("{source}: the callback transcript is unavailable"));
                    }
                }
            }
            HashSet::new()
        } else {
            let active = if self
                .reads
                .active_checked_at
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(5))
            {
                self.reads.active_checked_at = Some(Instant::now());
                self.active_logs()
            } else {
                Vec::new()
            };
            errors.extend(self.reads.listing.refresh(&self.provider_roots, &active));
            let handovers = self.archive_handovers(&mut errors);
            let now_ms = Utc::now().timestamp_millis();
            for path in &self.reads.listing.changed {
                if self
                    .reads
                    .listing
                    .files
                    .get(path)
                    .is_some_and(|file| due(self.logs.get(path), file, now_ms))
                {
                    self.reads.pending.insert(path.clone());
                }
            }
            handovers
        };
        let mut found = self
            .reads
            .pending
            .iter()
            .filter(|path| only.as_ref().is_none_or(|only| only.contains(*path)))
            .filter_map(|path| {
                self.reads
                    .listing
                    .files
                    .get(path)
                    .map(|file| (path.clone(), *file))
            })
            .collect::<Vec<_>>();
        found.sort_unstable_by_key(|(_, file)| std::cmp::Reverse(file.modified));
        // A log that could not be read waits out its pause; its failure is
        // still reported meanwhile.
        let started = Utc::now().timestamp_millis();
        if !reminder_only {
            self.reads.failed.keep(&self.reads.listing.files);
        }
        found.retain(|(path, _)| {
            if self.reads.failed.pausing(path, started) {
                errors.extend(self.reads.failed.reason(path).map(str::to_owned));
                false
            } else {
                true
            }
        });
        let mut removed = if reminder_only {
            HashSet::new()
        } else {
            self.reads.listing.removed.clone()
        };
        if !reminder_only && self.reads.listing.reconciled {
            removed.extend(
                self.logs
                    .values()
                    .filter(|head| {
                        let origin = head.origin_path.as_ref().unwrap_or(&head.path);
                        origin != Path::new("/account/cursor")
                            && !crate::demo::is_demo(origin)
                            && !self.reads.listing.files.contains_key(origin)
                    })
                    .map(|head| head.path.clone()),
            );
        }
        let removed_origins = removed.clone();
        removed.extend(
            self.logs
                .values()
                .filter(|head| {
                    head.origin_path
                        .as_ref()
                        .is_some_and(|path| removed_origins.contains(path))
                })
                .map(|head| head.path.clone()),
        );
        removed.retain(|path| !handovers.contains(path));
        for path in &removed {
            let Some(mut head) = self.logs.get(path).filter(|head| !head.gone).cloned() else {
                continue;
            };
            head.gone = true;
            let now_ms = self.next_timestamp_ms();
            if let Some(database) = &self.database {
                let result = serde_json::to_string(&head).ok().and_then(|encoded| {
                    checkpoints::update_head(
                        database,
                        path.to_string_lossy().as_ref(),
                        &encoded,
                        now_ms,
                    )
                    .ok()
                });
                if result.is_none() {
                    errors.push("Removed transcript checkpoint update failed".into());
                    self.reads.listing.note_changes(None);
                    continue;
                }
            }
            self.reads.pending.remove(path);
            self.reads.forced.remove(path);
            self.parser_cache.remove(path);
            self.archives.forget(path);
            self.logs.insert(path.clone(), head);
            self.usage_projection = None;
            self.meters.view = None;
        }
        for (path, metadata) in found {
            if budget == 0 {
                break;
            }
            let source = metadata.source;
            if source_db::accepts(&path, source) {
                match self.scan_database(&path, metadata, &mut budget) {
                    Ok(_) => self.reads.failed.succeeded(&path),
                    Err(error) => {
                        self.reads.failed.failed(
                            &path,
                            error.clone(),
                            Utc::now().timestamp_millis(),
                        );
                        errors.push(error);
                    }
                }
                continue;
            }
            let modified = metadata.modified;
            let original = self.logs.get(&path).cloned();
            let reset = original.as_ref().is_none_or(|log| {
                log.parser_version != PARSER_VERSION
                    || metadata.length < log.length
                    || (metadata.length == log.length && modified != log.modified)
            });
            let mut log = if reset {
                let mut draft = Log::new(path.clone(), source);
                if let Some(previous) = original.as_ref().filter(|previous| {
                    previous.length == metadata.length && previous.modified == modified
                }) {
                    draft.meter_target_at = metering::date_ms(&previous.updated);
                    if previous.parser_version != PARSER_VERSION {
                        draft.carried = Some(carried_usage::CarriedUsage::default());
                    }
                }
                draft
            } else {
                match self.load_parser(&path) {
                    Ok((log, _)) => log,
                    Err(error) => {
                        self.reads.failed.failed(
                            &path,
                            error.clone(),
                            Utc::now().timestamp_millis(),
                        );
                        errors.push(error);
                        continue;
                    }
                }
            };
            if !reset
                && !self.reads.forced.contains(&path)
                && !original.as_ref().is_some_and(|head| head.gone)
                && log.length == metadata.length
                && log.modified == modified
                && (if compressed::is_archive(&path) {
                    log.decoded_complete
                } else {
                    log.offset >= log.length || log.waiting_for_more
                })
            {
                self.reads.pending.remove(&path);
                continue;
            }
            if !reset && (log.length != metadata.length || log.modified != modified) {
                log.meter_target_at = None;
            }
            let archive = compressed::is_archive(&path);
            if archive {
                log.decoded_complete = false;
            }
            if archive && self.archives.active().is_some_and(|active| active != path) {
                continue;
            }
            self.reads.forced.remove(&path);
            if log.offset == 0 {
                log.read_kept_name();
            }
            // What this read holds of the session's conversation, for the
            // store that keeps it; none is gathered without one.
            let mut conversation = (self.conversation_sink.is_some() && source != "codex-index")
                .then(|| crate::conversation_collect::Gathered {
                    facts: Vec::new(),
                    awaiting: self.conversation_awaiting.get(&path).cloned(),
                });
            let line_start = log.offset;
            let result = (|| -> std::io::Result<()> {
                if archive {
                    if !self
                        .archives
                        .prepare(&path, metadata, log.offset, &mut budget)?
                    {
                        return Ok(());
                    }
                    read_lines(
                        &mut log,
                        self.archives.reader(),
                        &mut budget,
                        &self.requests,
                        &mut errors,
                        true,
                        conversation.as_mut(),
                    )?;
                    self.archives.position(log.offset);
                } else {
                    let mut file = File::open(&path)?;
                    file.seek(SeekFrom::Start(log.offset))?;
                    read_lines(
                        &mut log,
                        &mut BufReader::new(file),
                        &mut budget,
                        &self.requests,
                        &mut errors,
                        false,
                        conversation.as_mut(),
                    )?;
                }
                Ok(())
            })();
            if let Err(error) = result {
                let reason = format!("{source}: {error}");
                self.reads
                    .failed
                    .failed(&path, reason.clone(), Utc::now().timestamp_millis());
                errors.push(reason);
                self.archives.forget(&path);
            } else {
                self.reads.failed.succeeded(&path);
            }
            if log.decoded_complete {
                self.archives.forget(&path);
            }
            log.live.settle();
            log.length = metadata.length;
            log.modified = modified;
            let adopted_vpath = match self.adopt_mapped_session(&path, &mut log) {
                Ok(vpath) => vpath,
                Err(error) => {
                    errors.push(error);
                    None
                }
            };
            if !needs_read(Some(&LogHead::from_log(&log)), &metadata) {
                log.meter_target_at = None;
            }
            let projection_changed = original.as_ref().is_none_or(|previous| {
                previous.gone
                    || previous.source != log.source
                    || previous.modified != log.modified
                    || previous.id != log.id
                    || previous.observed != log.observed
                    || previous.live.is_internal != log.live.is_internal
                    || previous.usage != log.usage
                    || previous.buckets != log.buckets
                    || previous.titles != log.titles
                    || (previous.updated != log.updated
                        && self.logs.values().any(|other| {
                            other.path != path
                                && other.source == log.source
                                && other.id == log.id
                                && other.usage.total() == log.usage.total()
                        }))
            });
            let coverage_changed = original.as_ref().is_some_and(|previous| {
                previous.parser_version != log.parser_version
                    || previous.meter_target_at != log.meter_target_at
            });
            if log.updated.is_empty() {
                if let Some(modified) = modified {
                    log.updated = DateTime::<Utc>::from(modified).to_rfc3339();
                }
            }
            let carried_before = log.carried.is_some();
            let replaced = if let Some(carried) = &log.carried {
                let whole = !needs_read(Some(&LogHead::from_log(&log)), &metadata);
                // Marks are covered source time too: a prompt read before its
                // first call must outlive the handover's final sweep.
                let (replaced, next) = carried.writing(
                    log.meter_events
                        .values()
                        .map(|e| e.at)
                        .chain(log.meter_marks.iter().map(|mark| mark.at)),
                    whole,
                );
                log.carried = next;
                replaced
            } else {
                Vec::new()
            };
            let mut head = LogHead::from_log(&log);
            let mut payload_bytes = 0;
            let mut costs = (None, None);
            let now_ms = self.next_timestamp_ms();
            let committed = if let Some(database) = &mut self.database {
                let result = (|| -> Result<bool, String> {
                    let payload = serde_json::to_string(&log)
                        .map_err(|_| "Checkpoint encoding failed".to_owned())?;
                    payload_bytes = payload.len();
                    let transaction = database.transaction().map_err(|e| e.to_string())?;
                    let path_str = path.to_string_lossy();
                    let mut removed = if reset && !carried_before {
                        metering::reset(&transaction, &path_str).map_err(|e| e.to_string())?
                    } else {
                        false
                    };
                    if let Some(old_vpath) = &adopted_vpath {
                        let old_str = old_vpath.to_string_lossy();
                        if log.source_state["hasInference"] == true {
                            metering::move_contribution(
                                &transaction,
                                old_str.as_ref(),
                                path_str.as_ref(),
                                now_ms,
                            )
                            .map_err(|e| e.to_string())?;
                        } else {
                            metering::reset(&transaction, &old_str).map_err(|e| e.to_string())?;
                        }
                        checkpoints::delete_log(&transaction, old_str.as_ref())
                            .map_err(|e| e.to_string())?;
                        removed = true;
                    }
                    let mut carried_removed = false;
                    for (from, to) in &replaced {
                        carried_removed |=
                            metering::remove_between(&transaction, &path_str, *from, *to)
                                .map_err(|e| e.to_string())?;
                    }
                    let written = metering::write(
                        &transaction,
                        &path_str,
                        source,
                        &log.meter_events.values().cloned().collect::<Vec<_>>(),
                    )
                    .map_err(|e| e.to_string())?;
                    let marked = metering::write_marks(&transaction, &path_str, &log.meter_marks)
                        .map_err(|e| e.to_string())?;
                    // The session's costs follow its calls as they are now.
                    let mut priced = head.clone();
                    priced
                        .price(&transaction, &path_str)
                        .map_err(|e| e.to_string())?;
                    costs = (priced.list_cost, priced.account_cost);
                    let encoded_head = serde_json::to_string(&priced)
                        .map_err(|_| "Checkpoint head encoding failed".to_owned())?;
                    checkpoints::upsert_log(
                        &transaction,
                        path_str.as_ref(),
                        &payload,
                        &encoded_head,
                        now_ms,
                    )
                    .map_err(|e| e.to_string())?;
                    let should_map = log.observed
                        && source != "codex-index"
                        && !log.live.is_internal
                        && self
                            .session_map
                            .get(&(log.source.clone(), log.id.clone()))
                            .and_then(|existing| self.logs.get(existing))
                            .filter(|existing| {
                                !existing.gone
                                    && existing.path != path
                                    && !existing.path.to_string_lossy().contains("#hud-session-")
                            })
                            .is_none_or(|existing| log.usage.total() >= existing.usage.total());
                    if should_map {
                        checkpoints::upsert_session_mapping(
                            &transaction,
                            source,
                            &log.id,
                            path_str.as_ref(),
                            log.live.parent_id.as_deref(),
                            now_ms,
                        )
                        .map_err(|e| e.to_string())?;
                    }
                    transaction.commit().map_err(|e| e.to_string())?;
                    Ok(removed || carried_removed || written || marked)
                })();
                match result {
                    Ok(changed) => Some(changed),
                    Err(error) => {
                        errors.push(format!("Usage ledger checkpoint write failed: {error}"));
                        None
                    }
                }
            } else {
                Some(!log.meter_events.is_empty() || reset)
            };
            let Some(facts_changed) = committed else {
                continue;
            };
            if let (Some(sink), Some(gathered)) = (self.conversation_sink.clone(), conversation) {
                match gathered.awaiting {
                    Some(turn) => self.conversation_awaiting.insert(path.clone(), turn),
                    None => self.conversation_awaiting.remove(&path),
                };
                if let Err(error) =
                    self.deliver_conversation(sink, &path, &log, line_start, gathered.facts)
                {
                    errors.push(error);
                }
            }
            if let Some(old_vpath) = adopted_vpath {
                self.logs.remove(&old_vpath);
                self.parser_cache.remove(&old_vpath);
            }
            if facts_changed || projection_changed || coverage_changed {
                self.usage_projection = None;
                // The meter view reads this log again alone.
                self.meters
                    .changes
                    .insert(path.to_string_lossy().into_owned());
            }
            log.meter_events.clear();
            log.meter_marks.clear();
            if log.updated.is_empty() {
                if let Some(modified) = modified {
                    log.updated = DateTime::<Utc>::from(modified).to_rfc3339();
                }
            }
            if !needs_read(Some(&LogHead::from_log(&log)), &metadata) {
                self.reads.pending.remove(&path);
                log.meter_target_at = None;
            }
            let should_map = log.observed
                && source != "codex-index"
                && !log.live.is_internal
                && self
                    .session_map
                    .get(&(log.source.clone(), log.id.clone()))
                    .and_then(|existing| self.logs.get(existing))
                    .filter(|existing| {
                        !existing.gone
                            && existing.path != path
                            && !existing.path.to_string_lossy().contains("#hud-session-")
                    })
                    .is_none_or(|existing| log.usage.total() >= existing.usage.total());
            if should_map
                && self
                    .session_map
                    .insert((log.source.clone(), log.id.clone()), path.clone())
                    .as_ref()
                    != Some(&path)
            {
                // Another copy now speaks for the session.
                self.usage_projection = None;
            }
            if self.database.is_some() {
                head.account_cost = costs.1;
                head.set_list_cost(costs.0);
            }
            let held = path.to_string_lossy().into_owned();
            self.logs.insert(path, head);
            self.parser_cache.insert(log, payload_bytes);
            let _ = self.sync_holding(&[&held]);
        }
        self.price_unpriced_logs(64);
        // Expired usage leaves in its own small statement about once an hour.
        let now = Utc::now().timestamp_millis();
        if self
            .ledger_expired_at
            .is_none_or(|at| now - at >= 3_600_000)
        {
            self.ledger_expired_at = Some(now);
            if let Some(database) = &self.database {
                if metering::expire(database, now).is_ok_and(|deleted| deleted > 0) {
                    self.usage_projection = None;
                    self.meters.view = None;
                    self.statistics.revision = self.statistics.revision.wrapping_add(1);
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "{} ({}): {}",
                l10n::text(
                    "部分本地会话无法读取",
                    "Some local transcripts could not be read"
                ),
                errors.len(),
                errors[0]
            ))
        }
    }

    /// Works out the list cost of up to `limit` logs saved before it was kept.
    fn price_unpriced_logs(&mut self, limit: usize) {
        let Some(database) = &self.database else {
            return;
        };
        let unpriced = self
            .logs
            .iter()
            .filter(|(_, head)| !head.list_cost_priced)
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

    pub(crate) fn pending_meter_bounds(&self) -> HashMap<&str, Option<i64>> {
        let mut bounds = HashMap::<&str, Option<i64>>::new();
        for path in &self.reads.pending {
            let Some(file) = self.reads.listing.files.get(path) else {
                continue;
            };
            let latest = self.logs.get(path).and_then(|log| {
                if log.parser_version != PARSER_VERSION
                    && log.length == file.length
                    && log.modified == file.modified
                {
                    metering::date_ms(&log.updated)
                } else {
                    log.meter_target_at
                }
            });
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

    /// The logs whose usage counts: one copy per session, internal ones
    /// (Codex's guardian reviews) included, as their usage counts in Open;
    /// `listed_sessions` leaves those out of every list.
    /// The logs whose usage counts (`select`), as the usage projection holds
    /// them: it keeps the selection it was built on until a change to the
    /// logs, the session map or the enabled clients drops it, so the queries a
    /// pass makes do not pick and sort every log again.
    pub(crate) fn selected(&self) -> Vec<&LogHead> {
        match &self.usage_projection {
            Some(projection) => projection
                .selected
                .iter()
                .filter_map(|path| self.logs.get(path))
                .collect(),
            None => self.select(),
        }
    }

    fn select(&self) -> Vec<&LogHead> {
        // A log is its session's mapped copy when the mapping holds exactly
        // (source, id, path); borrowed keys avoid two String clones per log.
        let mapped = self
            .session_map
            .iter()
            .map(|((source, id), path)| (source.as_str(), id.as_str(), path.as_path()))
            .collect::<HashSet<_>>();
        let mapped_copy = |log: &LogHead| {
            mapped.contains(&(log.source.as_str(), log.id.as_str(), log.path.as_path()))
        };
        let mut sessions: HashMap<(&str, &str), &LogHead> = HashMap::new();
        for log in self.logs.values().filter(|log| {
            log.source != "codex-index"
                && self
                    .enabled
                    .get(log.source.as_str())
                    .copied()
                    .unwrap_or(true)
        }) {
            let is_mapped = mapped_copy(log);
            sessions
                .entry((log.source.as_str(), &log.id))
                .and_modify(|current| {
                    let current_mapped = mapped_copy(current);
                    // Of a session's copies, a listed one before a gone one,
                    // then the most recently modified counts (agent-hud-native-open
                    // LedgerCopies); usage and the mapping only break ties.
                    if (
                        !log.gone,
                        log.modified,
                        log.usage.total(),
                        is_mapped,
                        std::cmp::Reverse(&log.path),
                    ) > (
                        !current.gone,
                        current.modified,
                        current.usage.total(),
                        current_mapped,
                        std::cmp::Reverse(&current.path),
                    ) {
                        *current = log;
                    }
                })
                .or_insert(log);
        }
        let mut logs: Vec<_> = sessions.into_values().collect();
        logs.sort_by(|a, b| {
            b.updated
                .cmp(&a.updated)
                .then_with(|| a.source.cmp(&b.source))
                .then_with(|| a.id.cmp(&b.id))
                .then_with(|| a.path.cmp(&b.path))
        });
        logs
    }

    pub(crate) fn listed_sessions(&self) -> Vec<&LogHead> {
        self.selected()
            .into_iter()
            .filter(|head| !head.gone && !head.archived && !head.live.is_internal)
            .collect()
    }

    pub(crate) fn ensure_usage_projection(&mut self) {
        if self.usage_projection.is_some() {
            return;
        }
        self.statistics.revision = self.statistics.revision.wrapping_add(1);
        let logs = self.select();
        let mut totals = Usage::default();
        let mut buckets = BTreeMap::<i64, TimelineBucket>::new();
        let mut counts = HashMap::<String, usize>::new();
        for log in &logs {
            totals.add(log.usage);
            if !log.gone && !log.archived && !log.live.is_internal {
                *counts.entry(log.source.clone()).or_default() += 1;
            }
            for (at, value) in &log.buckets {
                buckets.entry(*at).or_default().add(*value);
            }
        }
        let timeline = buckets.into_iter().rev().take(48).collect::<Vec<_>>().into_iter().rev().map(|(at,value)|json!({"timestamp":DateTime::<Utc>::from_timestamp(at,0).unwrap().to_rfc3339(),"input":value.input,"output":value.output,"cache":value.cache,"tokens":value.input+value.output+value.cache})).collect();
        let wb_metering = &self.workbuddy_metering;
        let mut titles: HashMap<String, String> = self
            .logs
            .values()
            .filter(|log| log.source == "codex-index")
            .flat_map(|log| {
                log.titles
                    .iter()
                    .map(|(id, title)| (id.clone(), title.clone()))
            })
            .collect();
        titles.extend(wb_metering.titles.clone());
        let context_percents: HashMap<String, f64> = wb_metering
            .credits
            .iter()
            .filter(|&(_id, c)| c.size > 0)
            .map(|(id, c)| {
                (
                    id.clone(),
                    (c.used as f64 / c.size as f64 * 100.0).clamp(0.0, 100.0),
                )
            })
            .collect();
        self.usage_projection = Some(UsageProjection {
            selected: logs.iter().map(|log| log.path.clone()).collect(),
            totals,
            timeline,
            counts,
            titles,
            context_percents,
            observed_providers: self
                .logs
                .values()
                .filter(|log| log.observed && !log.live.is_internal)
                .map(|log| log.source.clone())
                .collect(),
        });
    }
}

/// A log as the session list shows it, before its head is written. The
/// session view needs core's projects and demo, so it lives here.
pub(crate) trait LogSession {
    fn session(&self, now: i64, subagents_at: Option<i64>) -> Value;
}

impl LogSession for Log {
    fn session(&self, now: i64, subagents_at: Option<i64>) -> Value {
        let updated = self
            .live
            .activity_at(metering::date_ms(&self.updated), subagents_at)
            .and_then(DateTime::<Utc>::from_timestamp_millis)
            .map(|at| at.to_rfc3339())
            .unwrap_or_else(|| self.updated.clone());
        let mut session = json!({"id":self.id,"title":self.title(),"source":self.source,"model":self.model,"vendorName":crate::naming::model_names::vendor_name(&self.source),"modelName":(!self.model.is_empty()).then(||crate::naming::model_names::consumer_name(&format!("{}-model:{}",self.source,self.model))),"cwd":self.cwd,"updatedAt":updated,"state":self.live.session_state(now,&self.source,subagents_at),"tokens":self.usage.total(),"cost":null,"contextPercent":null,"logPath":self.origin_path.as_ref().unwrap_or(&self.path),"parentId":self.live.parent_id});
        session["project"] = json!(crate::projects::project_id(self.cwd.as_deref()));
        if let Some(client) =
            crate::naming::clients::client_name(&self.source, self.client.as_deref())
        {
            session["client"] = json!(client);
        }
        if self.source == "antigravity" {
            if let Some(id) = self.id.strip_prefix("antigravity:") {
                session["navigationTarget"] =
                    json!({"kind":"antigravity-conversation","id":id,"cwd":self.cwd});
            }
        }
        // A demo session has a way back, so its reply card shows "Return to
        // session" as a real one does; the demo runtime goes nowhere with it.
        if crate::demo::is_demo(&self.path) {
            session["navigationTarget"] = json!({"kind":"demo","id":self.id});
        }
        if let Some(facts) = self.live.facts(&self.id, &self.source, now, subagents_at) {
            session["liveFacts"] = facts;
        }
        session
    }
}
