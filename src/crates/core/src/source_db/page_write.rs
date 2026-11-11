//! Writing a page read from a client's database to the ledger in one
//! transaction: its sessions, their mappings and the file's cursor.
use crate::{
    checkpoints::{self, LogHead},
    metering, AgentHudCore, Log,
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

impl AgentHudCore {
    /// Writes a page's session logs and the file's cursor log
    /// (`checkpoint`), marks the logs they superseded gone and maps each
    /// session to its log, in one transaction; then caches the logs.
    pub(super) fn write_database_page(
        &mut self,
        logs: HashMap<PathBuf, Log>,
        checkpoint: Log,
        reset_contributions: HashSet<PathBuf>,
        superseded: HashSet<PathBuf>,
        mappings: Vec<(String, String, PathBuf, Option<String>)>,
        budget: &mut u64,
    ) -> Result<(), String> {
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
        Ok(())
    }
}
