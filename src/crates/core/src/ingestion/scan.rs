//! A pass over the logs: listing them, reading the changed ones and writing
//! what they hold to the ledger.
use super::{line, needs_read, read_lines, route_claude_calls};
use crate::l10n;
use crate::{
    carried_usage,
    checkpoints::{self, LogHead},
    compressed, log_files, metering, source_db, AgentHudCore, Log, PARSER_VERSION,
};
use chrono::{DateTime, Utc};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

impl AgentHudCore {
    pub(crate) fn scan(&mut self, only: Option<HashSet<PathBuf>>) -> Result<(), String> {
        let reminder_only = only.is_some();
        let mut errors = self.storage_issue.iter().cloned().collect::<Vec<_>>();
        let mut budget = self.reads.budget();
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
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(3))
            {
                self.reads.active_checked_at = Some(Instant::now());
                self.active_logs()
            } else {
                Vec::new()
            };
            errors.extend(self.reads.listing.refresh(&self.provider_roots, &active));
            let handovers = self.archive_handovers(&mut errors);
            for path in &self.reads.listing.changed {
                if self
                    .reads
                    .listing
                    .files
                    .get(path)
                    .is_some_and(|file| needs_read(self.logs.get(path), file))
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
        let removed: HashSet<_> = removed
            .into_iter()
            .map(|path| log_files::native_path(&path))
            .collect();
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
            let path = log_files::native_path(&path);
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
                    // A Codex rollout read before its provider was kept says
                    // it on its first line; resuming past it, look there once.
                    if source == "codex" && log.offset > 0 && !log.codex_provider_known() {
                        let (first, _, _) = line(&mut BufReader::new(&mut file))?;
                        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&first) {
                            log.note_codex_provider(&value);
                        }
                    }
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
            let whole = !needs_read(Some(&LogHead::from_log(&log)), &metadata);
            if whole {
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
                    let mut events = log.meter_events.values().cloned().collect::<Vec<_>>();
                    if source == "claude" {
                        route_claude_calls(&transaction, &log, &mut events)
                            .map_err(|e| e.to_string())?;
                    }
                    let written = metering::write(&transaction, &path_str, source, &events)
                        .map_err(|e| e.to_string())?;
                    let marked = metering::write_marks(&transaction, &path_str, &log.meter_marks)
                        .map_err(|e| e.to_string())?;
                    // The session's costs follow its calls as they are now.
                    let mut priced = head.clone();
                    priced
                        .price(&transaction, &path_str)
                        .map_err(|e| e.to_string())?;
                    costs = (priced.list_costs(), priced.account_cost);
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
            if whole {
                self.reads.pending.remove(&path);
            }
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
}
