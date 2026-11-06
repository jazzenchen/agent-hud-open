//! Legacy database and usage/quota migration helpers.
use super::*;
pub(crate) use agent_hud_agents::sqlite as legacy_database;
pub(crate) use agent_hud_agents::transcript::carried_usage;
pub(crate) mod legacy_quota;
pub(crate) mod legacy_usage;

impl crate::AgentHudCore {
    pub fn set_legacy_quota_database(&mut self, path: Option<PathBuf>) {
        // The Swift app's ledger holds only its last 30 days of calls. It
        // stands in for reading a log again only in a ledger from before data
        // versions were recorded; a later one reads the log, keeping all of it.
        if let Some(path) = path.as_ref().filter(|_| self.data_version.is_none()) {
            let candidates = self
                .logs
                .values()
                .filter(|log| {
                    log.parser_version != PARSER_VERSION
                        && log.offset == log.length
                        && matches!(log.source.as_str(), "claude" | "codex")
                })
                .filter_map(|log| {
                    Some(legacy_usage::Candidate {
                        path: log.path.clone(),
                        source: log.source.clone(),
                        offset: log.offset,
                        length: log.length,
                        modified: log.modified?,
                    })
                })
                .collect::<Vec<_>>();
            let now_ms = self.next_timestamp_ms();
            if let Some(database) = &mut self.database {
                let imported = (|| -> rusqlite::Result<Vec<(PathBuf, LogHead)>> {
                    let transaction = database.transaction()?;
                    let report = legacy_usage::import(&transaction, path, &candidates)?;
                    let mut updates = Vec::new();
                    for item in report.imported {
                        let Some(original) = self.logs.get(&item.path) else {
                            continue;
                        };
                        if original.source == "codex" {
                            let known = metering::contribution_usage(
                                &transaction,
                                item.path.to_string_lossy().as_ref(),
                            )?;
                            if known != original.usage {
                                continue;
                            }
                        }
                        let (mut log, _) = checkpoints::read(&transaction, &item.path)
                            .map_err(rusqlite::Error::InvalidParameterName)?;
                        log.parser_version = PARSER_VERSION;
                        log.meter_target_at = None;
                        log.meter_codex = item.codex_baseline.map(|b| Usage {
                            input: b.input,
                            output: b.output,
                            read: b.read,
                            write: b.write,
                            reasoning: b.reasoning,
                        });
                        let payload = serde_json::to_string(&log)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                        let head = LogHead::from_log(&log);
                        let encoded_head = serde_json::to_string(&head)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                        checkpoints::upsert_log(
                            &transaction,
                            item.path.to_string_lossy().as_ref(),
                            &payload,
                            &encoded_head,
                            now_ms,
                        )?;
                        updates.push((item.path, head));
                    }
                    transaction.commit()?;
                    Ok(updates)
                })();
                match imported {
                    Ok(updates) => {
                        for (path, log) in updates {
                            self.parser_cache.remove(&path);
                            self.logs.insert(path, log);
                        }
                        let _ = self.sync_from_database();
                        self.meters.view = None;
                        self.usage_projection = None;
                    }
                    Err(_) => {
                        self.storage_issue = Some(l10n::text("无法沿用之前的用量记录，将重新逐步索引会话日志", "Previous usage history could not be reused; source logs will be indexed incrementally"));
                    }
                }
            }
        }
        self.legacy_quota_database = path;
    }
}
