//! Persistent quota observations and complete per-call usage facts.
//! All SQLite queries and mutations on `usage_meter_event`, `usage_meter_bucket`,
//! and `quota_sample` live here.

use crate::{pricing, Usage};
pub(crate) use agent_hud_agents::transcript::metering::{date_ms, event_key, Event, Mark};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::json;
use std::collections::{HashMap, HashSet};

mod calls;
mod capacity;
mod costs;
mod metrics;
mod queries;
mod replies;
mod spend;
mod view;
mod writes;

pub(crate) use calls::{
    calls_of, contribution_usage, marks, prompt_before, recached, session_calls, CallRow,
};
pub(crate) use costs::{
    account_cost, call_list_cost, largest_context, list_cost, price_tokens, ListCost,
};
pub(crate) use queries::{
    insert_quota_sample, prune_quota_samples, query_deepseek_billing_events, query_events_between,
    query_statistics_data, samples, Samples,
};
pub(crate) use replies::{reply_rate, reply_span};
pub(crate) use spend::Recent;
pub(crate) use writes::{write, write_marks};

fn tools_text(tools: &[String]) -> Option<String> {
    (!tools.is_empty()).then(|| json!(tools).to_string())
}

fn tools_from(text: Option<String>) -> Vec<String> {
    text.and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn unsigned(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    Ok(row.get::<_, i64>(index)? as u64)
}

fn integer(value: u64) -> rusqlite::Result<i64> {
    i64::try_from(value).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

fn usage(row: &rusqlite::Row<'_>, start: usize) -> rusqlite::Result<Usage> {
    Ok(Usage {
        input: unsigned(row, start)?,
        output: unsigned(row, start + 1)?,
        read: unsigned(row, start + 2)?,
        write: unsigned(row, start + 3)?,
        reasoning: unsigned(row, start + 4)?,
    })
}

pub(crate) fn reset(db: &Connection, path: &str) -> rusqlite::Result<bool> {
    let events = db.execute(
        "DELETE FROM usage_meter_event WHERE contribution=?1",
        [path],
    )?;
    let buckets = db.execute(
        "DELETE FROM usage_meter_bucket WHERE contribution=?1",
        [path],
    )?;
    let marks = db.execute("DELETE FROM usage_meter_mark WHERE contribution=?1", [path])?;
    Ok(events > 0 || buckets > 0 || marks > 0)
}

pub(crate) fn move_contribution(
    db: &Connection,
    from_path: &str,
    to_path: &str,
    updated_at_ms: i64,
) -> rusqlite::Result<()> {
    reset(db, to_path)?;
    db.execute(
        "UPDATE usage_meter_event SET contribution=?1, updated_at_ms=?2 WHERE contribution=?3",
        params![to_path, updated_at_ms, from_path],
    )?;
    db.execute(
        "UPDATE usage_meter_bucket SET contribution=?1, updated_at_ms=?2 WHERE contribution=?3",
        params![to_path, updated_at_ms, from_path],
    )?;
    db.execute(
        "UPDATE OR IGNORE usage_meter_mark SET contribution=?1 WHERE contribution=?2",
        params![to_path, from_path],
    )?;
    db.execute(
        "DELETE FROM usage_meter_mark WHERE contribution=?1",
        [from_path],
    )?;
    Ok(())
}

/// Range replacement is used only by a parser-version handover.
pub(crate) fn remove_between(
    db: &Connection,
    path: &str,
    from: i64,
    to: i64,
) -> rusqlite::Result<bool> {
    let removed = db.execute(
        "DELETE FROM usage_meter_event WHERE contribution=?1 AND at_ms BETWEEN ?2 AND ?3",
        params![path, from, to],
    )?;
    db.execute(
        "DELETE FROM usage_meter_mark WHERE contribution=?1 AND at_ms BETWEEN ?2 AND ?3",
        params![path, from, to],
    )?;
    if removed == 0 {
        return Ok(false);
    }
    rebuild(db, path)?;
    Ok(true)
}

pub(crate) fn rebuild(db: &Connection, path: &str) -> rusqlite::Result<()> {
    let now_ms = Utc::now().timestamp_millis();
    db.execute(
        "DELETE FROM usage_meter_bucket WHERE contribution=?1",
        [path],
    )?;
    db.execute(
        "INSERT INTO usage_meter_bucket(contribution,source,model,at_ms,input,output,cache_read,cache_write,reasoning,updated_at_ms)
         SELECT contribution,source,model,(at_ms/900000)*900000,SUM(input),SUM(output),SUM(cache_read),SUM(cache_write),SUM(reasoning),?2
         FROM usage_meter_event WHERE contribution=?1
         GROUP BY contribution,source,model,(at_ms/900000)*900000",
        params![path, now_ms],
    )?;
    db.execute(
        "WITH gaps AS (
            SELECT event_key,at_ms-lag(at_ms) OVER (ORDER BY at_ms,event_key) AS gap
            FROM usage_meter_event WHERE contribution=?1
         )
         UPDATE usage_meter_event SET gap_ms=(SELECT gap FROM gaps WHERE gaps.event_key=usage_meter_event.event_key)
         WHERE contribution=?1",
        [path],
    )?;
    Ok(())
}

/// The Mac keeps every token it records: usage rows never expire, and every
/// log on disk is read, so the Statistics window can reach as far back as
/// the logs do. The phone asks for its own 30 days. The quota and balance
/// metrics need no more than this window, so the meter view reads only it.
pub(crate) const READ_WINDOW_MS: i64 = 31 * 86_400_000;

/// What the snapshot's quota and balance metrics are read from, kept between
/// passes.
#[derive(Default)]
pub(crate) struct Meters {
    pub(crate) view: Option<View>,
    /// Logs whose usage changed since the meter view read them.
    pub(crate) changes: HashSet<String>,
    pub(crate) samples: Option<Samples>,
    pub(crate) billing: crate::billing_metrics::Cache,
}

/// Quota metrics over the selected logs: their 15-minute buckets and their
/// reply spans, kept per log so a log that changes is read again alone
/// (`update`), and replies summed in memory as the clock moves (`refresh_clock`).
#[derive(Default)]
pub(crate) struct View {
    buckets: Vec<Bucket>,
    /// (source, model, within the hour, timed by its log) → (output +
    /// reasoning, span ms, calls).
    replies: HashMap<(String, String, bool, bool), (u64, u64, u64)>,
    pub loaded_at: i64,
    pub valid_until: Option<i64>,
    /// The selected logs the parts were read for, sorted.
    selection: Vec<String>,
    parts: HashMap<String, Part>,
    /// (source, model) pairs the parts' rows name by index.
    keys: Vec<(String, String)>,
    key_index: HashMap<(String, String), u32>,
}

struct Bucket {
    source: String,
    model: String,
    at: i64,
    usage: Usage,
    cost: Option<pricing::ModelCost>,
}

#[derive(Default)]
struct Part {
    /// (key, bucket start, usage)
    buckets: Vec<(u32, i64, Usage)>,
    /// Calls that count toward the reply rate (`reply_span`), from a week
    /// before the part was read on.
    replies: Vec<Reply>,
    /// Times of this log's calls after the part was read (a clock behind).
    later: Vec<i64>,
}

struct Reply {
    key: u32,
    start: i64,
    at: i64,
    /// Output + reasoning.
    tokens: u64,
    span: u64,
    /// Whether the log timed the call, rather than the gap since its
    /// previous call.
    timed: bool,
}

const WEEK_MS: i64 = 7 * 86_400_000;

#[cfg(test)]
mod tests;
