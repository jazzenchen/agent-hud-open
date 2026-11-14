//! Bringing the in-memory logs up to date with the database, adopting a
//! session whose mapping moved to another log, and loading a log's parser
//! from its checkpoint.
use crate::l10n::tr;
use crate::{checkpoints, source_batches, AgentHudCore, Log};
use serde_json::json;
use std::path::{Path, PathBuf};

impl AgentHudCore {
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

    pub(super) fn adopt_mapped_session(
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
        if log.cwd.is_none() {
            log.cwd = old_log.cwd;
        }
        if log.live.parent_id.is_none() && old_log.live.parent_id.is_some() {
            log.live.parent_id = old_log.live.parent_id;
            log.live.is_subagent = old_log.live.is_subagent;
        }
        log.archived |= old_log.archived;
        if log.client.is_none() {
            log.client = old_log.client;
        }
        if let (Ok(mut cur), Ok(old)) = (
            serde_json::to_value(&log.naming),
            serde_json::to_value(&old_log.naming),
        ) {
            for key in ["named", "prompt", "agent", "fallback"] {
                if cur[key].is_null() && !old[key].is_null() {
                    cur[key] = old[key].clone();
                }
            }
            if let Ok(merged) = serde_json::from_value(cur) {
                log.naming = merged;
            }
        }
        if old_log.source_state["hasInference"] == true {
            log.messages = old_log.messages;
            log.usage = old_log.usage;
            log.turns = old_log.turns;
            log.buckets = old_log.buckets;
            log.meter_events.clear();
            log.source_state["hasInference"] = json!(true);
            for flag in ["hasOtel", "hasStore"] {
                if old_log.source_state[flag] == true {
                    log.source_state[flag] = json!(true);
                }
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

    pub(crate) fn load_parser(&mut self, path: &Path) -> Result<(Log, usize), String> {
        if let Some(log) = self.parser_cache.take(path) {
            return Ok(log);
        }
        let (mut log, bytes) = checkpoints::read(
            self.database
                .as_ref()
                .ok_or_else(|| tr!("Parser checkpoint storage is unavailable"))?,
            path,
        )?;
        // A checkpoint keeps no turn text; this run's words come from its head.
        if let Some(head) = self.logs.get(path) {
            log.live.keep_words(&head.live);
        }
        Ok((log, bytes))
    }
}
