//! Reading a page of a client's database or session file into the sessions
//! its records belong to and moving the file's cursor past it; `read_start`
//! picks where the read begins and `page_write` writes the page to the ledger.
use super::{opencode, opencode_legacy};
use crate::{legacy_database, log_files::FileStamp, source_batches, AgentHudCore, Log};
use agent_hud_agents::copilot::store::Counts;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

impl AgentHudCore {
    pub(crate) fn scan_database(
        &mut self,
        path: &Path,
        stamp: FileStamp,
        budget: &mut u64,
    ) -> Result<(), String> {
        let Some(mut checkpoint) = self.database_checkpoint(path, stamp)? else {
            return Ok(());
        };
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
        let reader = if is_batch || is_bot || is_legacy || is_turn_log || is_document || is_settings
        {
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
        let dirty = checkpoint.source_state["dirty"] == true;
        // A database that changed while it was read goes on from the
        // watermark its reader left, when it leaves one, rather than from the
        // start: a long read of a client that writes as it works (Antigravity)
        // would otherwise restart for as long as it works (agent-hud-open #53).
        let resume = page.cursor.is_none() && dirty && !is_batch && page.watermark.is_some();
        if let Some(watermark) = page.watermark {
            checkpoint.source_state["watermark"] = watermark;
        }
        checkpoint.source_state["cursor"] = if resume {
            checkpoint.source_state["dirty"] = json!(false);
            checkpoint.source_state["watermark"].clone()
        } else {
            page.cursor.clone().unwrap_or(Value::Null)
        };
        let complete = page.cursor.is_none() && !dirty;
        checkpoint.source_state["restart"] = json!(page.cursor.is_none() && !complete && !resume);
        checkpoint.offset = if complete { stamp.length } else { 0 };
        checkpoint.length = stamp.length;
        checkpoint.modified = stamp.modified;

        self.write_database_page(
            logs,
            checkpoint,
            reset_contributions,
            superseded,
            mappings,
            budget,
        )?;
        self.sync_from_database()?;
        if complete {
            self.reads.pending.remove(path);
            self.reads.forced.remove(path);
        }
        Ok(())
    }
}
