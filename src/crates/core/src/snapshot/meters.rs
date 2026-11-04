//! The meter view, the quota samples and the balance metrics, brought up to
//! date with the ledger before a snapshot is read from them.
use super::*;

impl crate::AgentHudCore {
    /// Brings the meter view, the quota samples and the balance metrics up to
    /// date with the ledger, and returns the time the snapshot is read at.
    pub(crate) fn prepare_meters(&mut self) -> Result<i64, String> {
        if !self.accounts.billing.is_empty() {
            if let Some(db) = &self.database {
                // Billed events are DeepSeek's (metering writes each event with
                // its log's source), so only DeepSeek logs are looked up.
                let deepseek = self
                    .usage_projection
                    .as_ref()
                    .unwrap()
                    .selected
                    .iter()
                    .filter(|path| {
                        self.logs
                            .get(*path)
                            .is_some_and(|log| log.source == "deepseek")
                    })
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                self.meters.billing.prepare(
                    db,
                    self.statistics.revision,
                    &deepseek,
                    wall_clock_ms(),
                )?;
            }
        }
        let now = wall_clock_ms();
        self.statistics.observed_at = Some(now);
        if let Some(database) = &self.database {
            if self.meters.view.is_none() || !self.meters.changes.is_empty() {
                let paths = self
                    .usage_projection
                    .as_ref()
                    .unwrap()
                    .selected
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                let result = if let Some(view) = &mut self.meters.view {
                    view.update(
                        database,
                        &paths,
                        &self.meters.changes,
                        now,
                        &self.price_regions,
                    )
                } else {
                    metering::View::load(database, &paths, now, &self.price_regions)
                        .map(|view| self.meters.view = Some(view))
                };
                if result.is_err() {
                    self.meters.view = None;
                    self.storage_issue = Some(l10n::text(
                        "无法从本地数据库读取用量指标",
                        "Usage metrics could not be read from SQLite",
                    ))
                }
                self.meters.changes.clear();
            }
        }
        if let Some(view) = &mut self.meters.view {
            if view.valid_until.is_some_and(|until| now >= until) || now < view.loaded_at {
                view.refresh_clock(now);
            }
        }
        if self.meters.samples.is_none() {
            if let Some(database) = &self.database {
                match metering::samples(database, now) {
                    Ok(samples) => self.meters.samples = Some(samples),
                    Err(_) => {
                        self.storage_issue = Some(l10n::text(
                            "无法从本地数据库读取额度记录",
                            "Quota history could not be read from SQLite",
                        ))
                    }
                }
            }
        }
        Ok(now)
    }
}
