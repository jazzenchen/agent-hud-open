//! Incremental database and batch session ingestion.
//! Every file tracks an incremental cursor/watermark; sessions resolve through
//! `session_mapping`, and all SQLite writes stamp `updated_at_ms`.
use crate::{
    checkpoints::{self, LogHead},
    legacy_database,
    log_files::FileStamp,
    metering, source_batches, AgentHudCore, Log, Usage, PARSER_VERSION,
};
use agent_hud_agents::copilot::store::Counts;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::SystemTime,
};

#[cfg(test)]
mod droid_tests;
#[cfg(test)]
mod tests;

pub(crate) fn virtual_path(path: &Path, id: &str) -> PathBuf {
    use std::fmt::Write;
    let mut key = String::new();
    for byte in id.as_bytes() {
        let _ = write!(key, "{byte:02x}");
    }
    PathBuf::from(format!("{}#hud-session-{key}", path.to_string_lossy()))
}

/// The client of OpenCode's database `source` is: OpenCode or a fork.
fn opencode(source: &str) -> Option<&'static agent_hud_agents::opencode::Store> {
    agent_hud_agents::opencode::store(source)
}

/// Whether `path` is one of the JSON message files OpenCode kept before its
/// database.
fn opencode_legacy(path: &Path, source: &str) -> bool {
    opencode(source).is_some_and(|store| store.legacy)
        && agent_hud_agents::opencode::legacy::accepts(path)
}

pub(crate) fn accepts(path: &Path, source: &str) -> bool {
    if opencode_legacy(path, source)
        || agent_hud_agents::cline::transcript::accepts(path, source)
        || source_batches::accepts(path, source)
        || (source == "grok-bot" && agent_hud_agents::grok::bot::accepts(path))
        || (source == "droid" && agent_hud_agents::droid::transcript::accepts(path))
    {
        return true;
    }
    let name = path.file_name().unwrap_or_default();
    match source {
        "hermes" => name == "state.db",
        "antigravity" => {
            path.extension().is_some_and(|e| e == "db") && name != "conversation_summaries.db"
        }
        "zcode" => name == "db.sqlite",
        "devin" | "goose" => name == "sessions.db",
        _ if opencode(source).is_some_and(|store| name == store.database) => true,
        // Cursor's session headers, and the turn log its `stop` hook keeps.
        "cursor" => name == "state.vscdb" || agent_hud_agents::hooks::turn_log::accepts(path),
        "copilot" => agent_hud_agents::copilot::store::accepts(path),
        "qoder-cn" => agent_hud_agents::qoder::legacy::accepts(path),
        _ => false,
    }
}

pub(crate) fn reader_version(path: &Path, source: &str) -> u16 {
    if opencode(source)
        .is_some_and(|store| path.file_name().is_some_and(|name| name == store.database))
    {
        2
    } else if source == "hermes" {
        // 2: calls carry the route their billing provider names.
        2
    } else if source == "antigravity" {
        // 2: trajectory_metadata_blob cwd, parent_id, role, duration_ms, archived.
        2
    } else {
        1
    }
}

impl AgentHudCore {
    fn resolve_session_path(
        &self,
        source: &str,
        session_id: &str,
        origin_path: &Path,
        superseded: &mut HashSet<PathBuf>,
    ) -> Option<PathBuf> {
        let map_key = (source.to_owned(), session_id.to_owned());
        if let Some(store) = opencode(source) {
            let is_db = origin_path.file_name().is_some_and(|n| n == store.database);
            let vpath = virtual_path(origin_path, session_id);
            if let Some(existing) = self.session_map.get(&map_key) {
                if !is_db
                    && existing
                        .to_string_lossy()
                        .contains(&format!("{}#hud-session-", store.database))
                    && self.logs.get(existing).is_some_and(|h| !h.gone)
                {
                    return None;
                }
                if is_db && existing != &vpath {
                    superseded.insert(existing.clone());
                }
            }
            return Some(vpath);
        }
        if let Some(existing) = self.session_map.get(&map_key) {
            if self.logs.contains_key(existing) {
                return Some(existing.clone());
            }
        }
        if let Some(head) = self
            .logs
            .values()
            .find(|h| h.source == source && h.id == session_id && !h.gone)
        {
            return Some(head.path.clone());
        }
        Some(virtual_path(origin_path, session_id))
    }

    pub(crate) fn scan_database(
        &mut self,
        path: &Path,
        stamp: FileStamp,
        budget: &mut u64,
    ) -> Result<(), String> {
        let previous = self.logs.get(path).cloned();
        let reader_version = reader_version(path, stamp.source);
        let signature = format!(
            "{reader_version}:{}:{}",
            stamp
                .modified
                .and_then(|d| d.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            stamp.length
        );
        let stored = if previous.is_some() {
            Some(self.load_parser(path)?.0)
        } else {
            None
        };
        let continuing = stored.as_ref().is_some_and(|log| {
            log.reader_version == reader_version
                && log.offset < log.length
                && !log.source_state["cursor"].is_null()
        });
        let changed = previous.as_ref().is_none_or(|old| {
            old.modified != stamp.modified
                || old.length != stamp.length
                || old.parser_version != PARSER_VERSION
                || old.reader_version != reader_version
                || old.gone
        });
        let can_grow_incrementally = !continuing
            && previous.as_ref().is_some_and(|old| {
                stamp.length >= old.length
                    && old.parser_version == PARSER_VERSION
                    && old.reader_version == reader_version
                    && !old.gone
            });
        let append_batch = source_batches::accepts(path, stamp.source)
            && can_grow_incrementally
            && previous
                .as_ref()
                .is_some_and(|old| old.length < stamp.length);
        let resume_watermark = !source_batches::accepts(path, stamp.source)
            && can_grow_incrementally
            && changed
            && stored
                .as_ref()
                .is_some_and(|log| !log.source_state["watermark"].is_null());
        let restart = stored
            .as_ref()
            .is_some_and(|log| log.source_state["restart"] == true);
        let mut checkpoint = if continuing {
            let mut log = stored.unwrap();
            if changed {
                log.source_state["dirty"] = json!(true);
            }
            log
        } else if append_batch {
            let mut log = stored.unwrap();
            log.source_state["cursor"] = source_batches::resume(
                self.database.as_ref().ok_or("Usage ledger unavailable")?,
                path,
                log.source_state["generation"]
                    .as_str()
                    .ok_or("Missing source generation")?,
            )
            .map_err(|_| "Could not resume source log")?;
            log.source_state["dirty"] = json!(false);
            log
        } else if resume_watermark {
            let mut log = stored.unwrap();
            log.source_state["cursor"] = log.source_state["watermark"].clone();
            log.source_state["dirty"] = json!(false);
            log
        } else if changed || restart {
            let mut log = Log::new(path.into(), stamp.source);
            log.reader_version = reader_version;
            log.live.is_internal = true;
            log.source_state =
                json!({"generation":signature,"cursor":null,"watermark":null,"dirty":false});
            log
        } else {
            stored.unwrap()
        };
        // A Grok Bot roster is read again while a turn works, though it has
        // not changed (`Reads::bot_poll`).
        let polled = stamp.source == "grok-bot" && self.reads.forced.contains(path);
        if !changed && !restart && !continuing && !polled && checkpoint.offset >= stamp.length {
            self.reads.pending.remove(path);
            self.reads.forced.remove(path);
            return Ok(());
        }
        let is_batch = source_batches::accepts(path, stamp.source);
        if is_batch {
            *budget = budget.saturating_sub(2 * 1024 * 1024);
        }
        let is_bot = stamp.source == "grok-bot";
        // A JSON document per session, read whole: Factory Droid's settings.
        let is_settings = stamp.source == "droid";
        let is_turn_log = agent_hud_agents::hooks::turn_log::accepts(path);
        let is_legacy = opencode_legacy(path, stamp.source);
        // A messages file written again whole (Cline's, Roo Code's).
        let is_document = agent_hud_agents::cline::transcript::accepts(path, stamp.source);
        let replaces_sessions = opencode(stamp.source).is_some() && !is_legacy;
        let reader = if is_batch || is_bot || is_legacy || is_turn_log || is_document || is_settings {
            None
        } else {
            Some(legacy_database::ReadOnlyDatabase::open(path).map_err(|_| {
                format!(
                    "{} session database could not be opened read-only",
                    stamp.source
                )
            })?)
        };
        let cursor = &checkpoint.source_state["cursor"];
        let page = if is_legacy {
            agent_hud_agents::opencode::legacy::read(path)
        } else if is_document {
            agent_hud_agents::cline::transcript::read(path, stamp.source)
        } else if is_bot {
            agent_hud_agents::grok::bot::read(path, cursor)
        } else if is_settings {
            agent_hud_agents::droid::transcript::read(path, cursor)
        } else if is_turn_log {
            agent_hud_agents::hooks::turn_log::read(path, cursor)
        } else if is_batch {
            source_batches::read(
                path,
                self.database.as_ref().ok_or("Usage ledger unavailable")?,
                checkpoint.source_state["generation"]
                    .as_str()
                    .ok_or("Missing generation")?,
                cursor,
                stamp.source,
            )
        } else {
            let reader = reader.as_ref().unwrap();
            match stamp.source {
                "zcode" => agent_hud_agents::zcode::transcript::read(&reader.connection, cursor),
                "hermes" => {
                    agent_hud_agents::hermes::transcript::read(&reader.connection, path, cursor)
                }
                "devin" => agent_hud_agents::devin::transcript::read(&reader.connection, cursor),
                "goose" => agent_hud_agents::goose::transcript::read(&reader.connection, cursor),
                "cursor" => agent_hud_agents::cursor::sessions::read(&reader.connection, cursor),
                "copilot" => agent_hud_agents::copilot::store::read(&reader.connection, cursor),
                "qoder-cn" => agent_hud_agents::qoder::legacy::read(&reader.connection, cursor),
                "antigravity" => crate::clients::antigravity::read(
                    &reader.connection,
                    self.database.as_ref().ok_or("Usage ledger unavailable")?,
                    path,
                    checkpoint.source_state["generation"]
                        .as_str()
                        .ok_or("Missing generation")?,
                    cursor,
                ),
                source => match opencode(source) {
                    Some(store) => agent_hud_agents::opencode::transcript::read(
                        &reader.connection,
                        cursor,
                        store,
                    ),
                    None => Err(rusqlite::Error::InvalidQuery),
                },
            }
        }
        .map_err(|_| {
            format!(
                "{} session source has an unsupported or inconsistent schema",
                stamp.source
            )
        })?;

        let mut logs = HashMap::<PathBuf, Log>::new();
        let mut reset_contributions = HashSet::<PathBuf>::new();
        let mut superseded = HashSet::<PathBuf>::new();
        let mut mappings = Vec::<(String, String, PathBuf, Option<String>)>::new();
        let generation = checkpoint.source_state["generation"].clone();

        let mut bot_running = false;
        for record in page.records {
            let Some(key) =
                self.resolve_session_path(stamp.source, &record.id, path, &mut superseded)
            else {
                continue;
            };
            if !logs.contains_key(&key) {
                let log = if self.logs.contains_key(&key) {
                    self.load_parser(&key)?.0
                } else {
                    let mut log = Log::new(key.clone(), stamp.source);
                    log.id = record.id.clone();
                    log
                };
                logs.insert(key.clone(), log);
            }
            let log = logs.get_mut(&key).unwrap();
            let is_virtual = key.to_string_lossy().contains("#hud-session-");
            // A turn log's month adds tokens to a session another source
            // lists, and leaves that source its origin: the month's file is
            // removed after two months while the session goes on.
            if is_virtual && !(is_turn_log && log.origin_path.is_some()) {
                log.origin_path = Some(path.into());
            }
            // OpenCode updates existing messages and may replace the legacy
            // table's replies with different IDs in session_message. Each file
            // generation therefore replaces a session once, across all pages.
            if replaces_sessions && log.source_state["generation"] != generation {
                log.clear_usage();
                reset_contributions.insert(key.clone());
            }
            log.source_state["generation"] = generation.clone();
            if stamp.source == "antigravity" && log.client.is_none() {
                log.client = agent_hud_agents::antigravity::transcript::client_from_path(path);
            }
            if record.parent.is_some() {
                log.live.parent_id = record.parent.clone();
                log.live.is_subagent = true;
            }
            if let Some(archived) = record.archived {
                log.archived = archived;
            }
            log.naming.name(0, record.title.as_deref());
            if let Some(fallback) = record.title_fallback {
                log.naming.fallback(fallback);
            }
            log.cwd = record.cwd.or(log.cwd.take());
            log.observed = true;
            if is_virtual {
                log.modified = stamp.modified;
            }
            log.clock(record.updated.unwrap_or(record.at));
            let clean_model = record
                .model
                .split('#')
                .next()
                .unwrap_or(&record.model)
                .to_owned();
            if clean_model != "Unknown" || log.model == "Unknown" {
                log.model = clean_model;
            }
            // Copilot's session store counts a session's calls once read;
            // the OTel export's and the shutdown totals give way to it.
            if stamp.source == "copilot" && !is_batch {
                if log.copilot_store_counts() == Counts::Replace && !log.messages.is_empty() {
                    log.clear_usage();
                    reset_contributions.insert(key.clone());
                }
            } else if stamp.source == "copilot" && !log.copilot_otel_counts() {
                continue;
            } else if (is_batch || stamp.source == "antigravity")
                && record.usage.total() > 0
                && log.source_state["hasInference"] != true
            {
                if !log.messages.is_empty() {
                    log.clear_usage();
                    reset_contributions.insert(key.clone());
                }
                log.source_state["hasInference"] = json!(true);
                if stamp.source == "copilot" {
                    log.source_state["hasOtel"] = json!(true);
                }
            }
            if is_bot {
                if let (Some(dir), Some(agent)) = (path.parent(), record.id.rsplit(':').next()) {
                    if let Some(account) = agent_hud_agents::grok::cache::account(dir) {
                        if let Some(turn) = agent_hud_agents::grok::bot::turn(dir, &account, agent)
                        {
                            bot_running |= turn.running;
                            log.live.observe_source(
                                &turn.id,
                                turn.running,
                                Some(turn.started),
                                turn.at,
                                (!turn.running).then_some("completed"),
                            );
                        }
                    }
                }
            }
            log.count_timed_source(
                record.key,
                &record.model,
                record.at,
                record.usage,
                false,
                record.duration_ms,
            );
            mappings.push((
                stamp.source.to_owned(),
                record.id,
                key,
                log.live.parent_id.clone(),
            ));
        }

        if is_bot {
            self.reads.bot_poll = bot_running.then(|| {
                (
                    chrono::Utc::now().timestamp_millis() + crate::BOT_POLL_MS,
                    path.to_path_buf(),
                )
            });
        }
        if let Some(watermark) = page.watermark {
            checkpoint.source_state["watermark"] = watermark;
        }
        checkpoint.source_state["cursor"] = page.cursor.clone().unwrap_or(Value::Null);
        let complete = page.cursor.is_none() && checkpoint.source_state["dirty"] != true;
        checkpoint.source_state["restart"] = json!(page.cursor.is_none() && !complete);
        checkpoint.offset = if complete { stamp.length } else { 0 };
        checkpoint.length = stamp.length;
        checkpoint.modified = stamp.modified;

        let mut encoded = Vec::new();
        for (_, log) in logs {
            let head = LogHead::from_log(&log);
            let payload = serde_json::to_string(&log)
                .map_err(|_| "Could not encode a client session checkpoint")?;
            let bytes = payload.len();
            *budget = budget.saturating_sub(bytes as u64);
            encoded.push((log, head, payload, bytes));
        }
        let physical_head = LogHead::from_log(&checkpoint);
        let parent_payload =
            serde_json::to_string(&checkpoint).map_err(|_| "Could not encode a database cursor")?;
        let parent_bytes = parent_payload.len();
        encoded.push((checkpoint, physical_head, parent_payload, parent_bytes));

        let now_ms = self.next_timestamp_ms();
        let db = self
            .database
            .as_mut()
            .ok_or("Usage ledger is unavailable")?;
        let transaction = db
            .transaction()
            .map_err(|_| "Could not begin client database ingestion")?;

        for (log, head, payload, _) in &encoded {
            let log_path = log.path.to_string_lossy();
            if reset_contributions.contains(&log.path) {
                metering::reset(&transaction, &log_path)
                    .map_err(|_| "Could not reset superseded fallback usage")?;
            }
            metering::write(
                &transaction,
                &log_path,
                &log.source,
                &log.meter_events.values().cloned().collect::<Vec<_>>(),
            )
            .map_err(|_| "Could not write client usage ledger")?;
            let head =
                serde_json::to_string(head).map_err(|_| "Could not encode client session head")?;
            checkpoints::upsert_log(&transaction, log_path.as_ref(), payload, &head, now_ms)
                .map_err(|_| "Could not write client source cursor")?;
        }

        for old_path in &superseded {
            if let Some(mut old_head) = self.logs.get(old_path).cloned() {
                old_head.gone = true;
                let encoded_head = serde_json::to_string(&old_head)
                    .map_err(|_| "Could not encode superseded session head")?;
                checkpoints::update_head(
                    &transaction,
                    old_path.to_string_lossy().as_ref(),
                    &encoded_head,
                    now_ms,
                )
                .map_err(|_| "Could not mark superseded session head")?;
                self.parser_cache.remove(old_path);
            }
        }

        for (source, session_id, canonical_path, parent_id) in &mappings {
            checkpoints::upsert_session_mapping(
                &transaction,
                source,
                session_id,
                canonical_path.to_string_lossy().as_ref(),
                parent_id.as_deref(),
                now_ms,
            )
            .map_err(|_| "Could not write session mapping")?;
        }

        transaction
            .commit()
            .map_err(|_| "Could not commit client database page")?;

        for (mut log, _, _, bytes) in encoded {
            log.meter_events.clear();
            self.parser_cache.insert(log, bytes);
        }
        self.sync_from_database()?;
        if complete {
            self.reads.pending.remove(path);
            self.reads.forced.remove(path);
        }
        Ok(())
    }

    pub(crate) fn ingest_account_sessions(&mut self, records: &[Value]) -> Result<(), String> {
        if records.is_empty() || self.database.is_none() {
            return Ok(());
        }
        let origin = Path::new("/account/cursor");
        let mut logs = HashMap::<PathBuf, Log>::new();
        let mut superseded = HashSet::<PathBuf>::new();
        let mut mappings = Vec::<(String, String, PathBuf, Option<String>)>::new();
        for row in records {
            let source: &'static str = match row["source"].as_str() {
                Some("cursor") => "cursor",
                Some("grok-bot") => "grok-bot",
                _ => continue,
            };
            let Some(id) = row["id"].as_str() else {
                continue;
            };
            let Some(event_key) = row["key"].as_str() else {
                continue;
            };
            let Some(at) = row["at"].as_i64() else {
                continue;
            };
            let model = row["model"].as_str().unwrap_or("Unknown");
            let usage = Usage {
                input: row["input"].as_u64().unwrap_or(0),
                output: row["output"].as_u64().unwrap_or(0),
                read: row["read"].as_u64().unwrap_or(0),
                write: row["write"].as_u64().unwrap_or(0),
                reasoning: row["reasoning"].as_u64().unwrap_or(0),
            };
            let Some(key) = self.resolve_session_path(source, id, origin, &mut superseded) else {
                continue;
            };
            if !logs.contains_key(&key) {
                let log = if self.logs.contains_key(&key) {
                    self.load_parser(&key)?.0
                } else {
                    let mut log = Log::new(key.clone(), source);
                    log.id = id.to_owned();
                    log
                };
                logs.insert(key.clone(), log);
            }
            let log = logs.get_mut(&key).unwrap();
            if key.to_string_lossy().contains("#hud-session-") && log.origin_path.is_none() {
                log.origin_path = Some(origin.into());
            }
            if let Some(parent) = row["parent"].as_str() {
                log.live.parent_id = Some(parent.to_owned());
                log.live.is_subagent = true;
            }
            // A record that does not say leaves the session's archive state
            // alone (Grok Bot's daily tokens beside its roster's sessions).
            if let Some(archived) = row["archived"].as_bool() {
                log.archived = archived;
            }
            log.naming.name(0, row["title"].as_str());
            if let Some(fallback) = row["titleFallback"].as_str() {
                log.naming.fallback(fallback);
            }
            log.observed = true;
            log.clock(at);
            let clean_model = model.split('#').next().unwrap_or(model).to_owned();
            if clean_model != "Unknown" || log.model == "Unknown" {
                log.model = clean_model;
            }
            log.count_source(event_key.to_owned(), model, at, usage, false);
            mappings.push((
                source.to_owned(),
                id.to_owned(),
                key,
                log.live.parent_id.clone(),
            ));
        }
        if logs.is_empty() {
            return Ok(());
        }
        let mut encoded = Vec::new();
        for (_, log) in logs {
            let head = LogHead::from_log(&log);
            let payload = serde_json::to_string(&log)
                .map_err(|_| "Could not encode an account session checkpoint")?;
            let bytes = payload.len();
            encoded.push((log, head, payload, bytes));
        }
        let now_ms = self.next_timestamp_ms();
        let db = self
            .database
            .as_mut()
            .ok_or("Usage ledger is unavailable")?;
        let transaction = db
            .transaction()
            .map_err(|_| "Could not begin account session ingestion")?;
        for (log, head, payload, _) in &encoded {
            let log_path = log.path.to_string_lossy();
            metering::write(
                &transaction,
                &log_path,
                &log.source,
                &log.meter_events.values().cloned().collect::<Vec<_>>(),
            )
            .map_err(|_| "Could not write account usage ledger")?;
            let head =
                serde_json::to_string(head).map_err(|_| "Could not encode account session head")?;
            checkpoints::upsert_log(&transaction, log_path.as_ref(), payload, &head, now_ms)
                .map_err(|_| "Could not write account session checkpoint")?;
        }
        for (source, session_id, canonical_path, parent_id) in &mappings {
            checkpoints::upsert_session_mapping(
                &transaction,
                source,
                session_id,
                canonical_path.to_string_lossy().as_ref(),
                parent_id.as_deref(),
                now_ms,
            )
            .map_err(|_| "Could not write account session mapping")?;
        }
        transaction
            .commit()
            .map_err(|_| "Could not commit account sessions")?;
        for (mut log, _, _, bytes) in encoded {
            log.meter_events.clear();
            self.parser_cache.insert(log, bytes);
        }
        self.sync_from_database()?;
        self.usage_projection = None;
        self.meters.view = None;
        Ok(())
    }
}
