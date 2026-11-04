//! The sessions account readers report (Cursor's, Grok Bot's), written to
//! the ledger like the sessions read from a client's files.
use crate::{
    checkpoints::{self, LogHead},
    metering, AgentHudCore, Log, Usage,
};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

impl AgentHudCore {
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
