//! The engine's facts as the records the phone reads: this computer, its
//! quota readings, its token usage by day and the names of what used it.
mod readings;
mod usage;

pub use readings::{accounts_loading, reading_facts, readings};
pub use usage::{consumer, usage};

use crate::{keys, record, SyncRecord};
use serde_json::{json, Value};

/// This computer as the phone lists it. A product adds its own fields.
pub fn device(device: &str, name: &str, now: i64) -> SyncRecord {
    SyncRecord::new(
        "device",
        device,
        device,
        json!({"name":name,"heartbeatAtMs":now}),
        now,
        None,
    )
}
/// The usage records' identifiers for the days from `from_ms` until the day of
/// `until_ms`, whether or not they were ever written.
pub fn usage_ids(device: &str, from_ms: i64, until_ms: i64, version: Option<u32>) -> Vec<String> {
    let mut ids = Vec::new();
    let mut day = keys::day_start(from_ms);
    let end = keys::day_start(until_ms);
    while day < end {
        ids.push(record::record_id("usage", device, &keys::day(day), version));
        day += keys::DAY_MS;
    }
    ids
}
/// A time the engine writes as epoch milliseconds or as RFC 3339 text.
pub fn milliseconds(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| {
        value
            .as_str()
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .map(|v| v.timestamp_millis())
    })
}

#[cfg(test)]
mod tests;
