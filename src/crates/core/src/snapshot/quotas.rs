//! The quota windows a snapshot shows, with their metrics.
use super::*;
use crate::database::quota_link::QuotaLink;

impl crate::AgentHudCore {
    /// The quota windows of enabled clients with their metrics, and WorkBuddy's
    /// ceilings, each with a short name distinct among its account's windows.
    pub(super) fn quota_rows(
        &self,
        at: i64,
        now: i64,
        pending_meter: &HashMap<&str, Option<i64>>,
    ) -> Vec<Value> {
        // A window linked to some clients waits for their logs; one that a
        // route links to any client's calls waits for every log.
        let metering_complete = |value: &Value, cutoff: i64| {
            let read = |latest: &Option<i64>| latest.is_some_and(|latest| latest < cutoff);
            self.database.is_some()
                && match QuotaLink::of(value).map(|link| link.sources()) {
                    None => true,
                    Some(Some(sources)) => sources
                        .iter()
                        .all(|source| pending_meter.get(source).is_none_or(read)),
                    Some(None) => pending_meter.values().all(read),
                }
        };
        let mut quotas = self
            .accounts
            .quotas
            .iter()
            .filter(|value| self.shown(value, "vendor"))
            .map(|value| {
                let mut row = value.clone();
                if let Some(view) = &self.meters.view {
                    let cycle_start = value["resetAt"]
                        .as_str()
                        .and_then(metering::date_ms)
                        .zip(value["windowDurationSeconds"].as_f64())
                        .and_then(|(reset, duration)| reset.checked_sub((duration * 1000.0) as i64))
                        .unwrap_or(at - 7 * 86_400_000);
                    row["metrics"] = view.metrics(
                        value,
                        at,
                        metering_complete(value, cycle_start),
                        metering_complete(value, at - 7 * 86_400_000),
                        self.meters.samples.as_ref().unwrap_or(&Default::default()),
                        self.database.as_ref().map(|db| (db, &self.price_regions)),
                    );
                }
                // legacyWindowId stays: panel layouts, Settings and get_hud name a
                // row by it (src/packages/hud/src/quota-rows.ts).
                if let Some(object) = row.as_object_mut() {
                    for key in ["consumerSource", "consumerRoutes", "modelFamilies"] {
                        object.remove(key);
                    }
                }
                row
            })
            .collect::<Vec<_>>();
        if self.enabled.get("workbuddy").copied().unwrap_or(true) {
            let cache_path = self
                .settings_path
                .parent()
                .map(|dir| dir.join("workbuddy-ceilings.json"));
            for row in crate::clients::workbuddy::ceiling_quotas(
                self.logs.values(),
                cache_path.as_deref(),
                now,
            ) {
                quotas.push(row);
            }
        }
        crate::naming::window_names::settle_short_names(&mut quotas);
        quotas
    }
}
