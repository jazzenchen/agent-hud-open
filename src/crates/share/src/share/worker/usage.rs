//! The usage records: the ledger's usage by day, read at most every 30
//! seconds once it has moved, over every day kept now and then.
use super::{Worker, RETENTION_MS};
use crate::{keys, projection, SyncRecord};
use serde_json::{json, Value};

const USAGE_EVERY_MS: i64 = 30_000;
/// How often a changed ledger is read again over every day kept, for the
/// changes that land before yesterday: a new client's history, a rescan, a
/// new price. Records that come out the same are not sent again.
const USAGE_FULL_EVERY_MS: i64 = 30 * 60_000;

impl Worker {
    pub(super) fn project_usage(&mut self, snapshot: &Value, now: i64) -> Result<(), String> {
        let revision = snapshot["statisticsRevision"].as_u64();
        let full_due = self.usage_seeded
            && revision != self.usage_full_revision
            && now - self.usage_full_at >= USAGE_FULL_EVERY_MS;
        if revision.is_some() && revision == self.usage_revision && !full_due {
            return Ok(());
        }
        if self.usage_tried_at != 0 && now - self.usage_tried_at < USAGE_EVERY_MS {
            return Ok(());
        }
        self.usage_tried_at = now;
        // The first read covers every day kept; later ones yesterday and today,
        // and every day again now and then while the ledger keeps changing.
        let full = !self.usage_seeded || full_due;
        let retention_start = now - RETENTION_MS;
        let window_start = if full {
            retention_start
        } else {
            (keys::day_start(now) - keys::DAY_MS).max(retention_start)
        };
        let facts = (self.query)(
            "usage_buckets",
            json!({"from":rfc3339(window_start),"to":rfc3339(now),"bucketMinutes":15}),
        )?;
        // Until indexing can replace whole days, the days already sent stay.
        if facts["coverage"]["complete"] != true {
            return Ok(());
        }
        let records = projection::usage(&self.settings.device_id, &facts, now)?;
        let window_day = keys::day_start(window_start);
        self.records.retain(|_, r| match r.kind.as_str() {
            "usage" => !full && usage_day(r) < window_day,
            // Names stay for the days not read again.
            "consumer" => !full,
            _ => true,
        });
        for record in records {
            self.records.insert(record.id(), record);
        }
        self.usage_seeded = true;
        self.usage_revision = revision;
        if full {
            self.usage_full_revision = revision;
            self.usage_full_at = now;
        }
        Ok(())
    }
}
/// The day a usage record covers: it expires 31 days after that day begins.
pub(super) fn usage_day(record: &SyncRecord) -> i64 {
    record
        .expires_at_ms
        .map(|end| end - 31 * keys::DAY_MS)
        .unwrap_or(0)
}
fn rfc3339(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .expect("a clock time")
        .to_rfc3339()
}
