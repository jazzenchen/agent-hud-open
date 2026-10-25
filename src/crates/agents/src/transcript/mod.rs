//! The transcript model every client's parser fills in: a log as it is read
//! (`Log`), its live session facts (`LiveLog`), its usage and meter events,
//! and the records a database source yields. Each client's parser lives in its
//! own folder (`<client>/transcript.rs`).
pub mod carried_usage;
pub mod items;
pub mod kept_names;
pub mod live;
pub mod log;
pub mod metering;
pub mod records;
pub mod turns;
pub mod usage;

#[cfg(test)]
mod call_timing_tests;

pub use live::LiveLog;
pub use log::{Log, RecentCall};
pub use usage::{number, TimelineBucket, Usage};

pub const MAX_TURNS: usize = 128;

/// Each data version and the reading rules it names, oldest first.
///
/// A data version is the release in which the way logs are read last
/// changed: 0.6.5 when 0.6.5 reads them differently, and 0.6.5 still while
/// 0.7.0 reads them the same way. The ledger records it; a log a checkpoint
/// holds under other rules is read once more while it is within the
/// retention, and what older or removed logs counted stays. To change how
/// logs are read, add a row named after the release that ships it (a build
/// before that release uses `-dev.1`, `-dev.2`…), never edit a row.
///
/// The rules before data versions: 5 kept the client that drives a session;
/// 6 kept each log's prompt and compaction marks, the tools each call asked
/// for and Codex's context window; 7 kept Claude Code's thinking tokens as
/// reasoning, counted lines without a message id by request id or offset,
/// and took the session's model from the most output.
pub const DATA_VERSIONS: &[(&str, u32)] = &[("0.6.0", 7)];

/// The data version this build reads logs by.
pub const DATA_VERSION: &str = DATA_VERSIONS[DATA_VERSIONS.len() - 1].0;

/// The reading rules a checkpoint records for each log: the number
/// `DATA_VERSION` names.
pub const PARSER_VERSION: u32 = DATA_VERSIONS[DATA_VERSIONS.len() - 1].1;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::HashSet;

pub fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| !s.trim().is_empty())
}

pub fn n(value: &Value) -> u64 {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0)
}

pub fn at(value: &Value) -> Option<i64> {
    text(value).and_then(metering::date_ms).or_else(|| {
        value
            .as_f64()
            .filter(|v| v.is_finite() && *v > 0.0)
            .map(|v| v as i64)
    })
}

pub fn iso(at: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(at)
        .map(|d| d.to_rfc3339())
        .unwrap_or_default()
}

pub fn first<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .find_map(|key| value.get(*key).filter(|v| !v.is_null()))
}

pub fn positive(value: &Value, keys: &[&str]) -> Option<u64> {
    let values = keys
        .iter()
        .filter_map(|k| value.get(*k))
        .map(n)
        .collect::<Vec<_>>();
    values
        .iter()
        .find(|n| **n > 0)
        .copied()
        .or_else(|| values.first().copied())
}

pub fn parts(value: &Value) -> Option<&str> {
    text(value).or_else(|| value.as_array()?.iter().find_map(|v| text(&v["text"])))
}

pub fn sql_columns(db: &Connection, table: &str) -> rusqlite::Result<HashSet<String>> {
    db.prepare("SELECT name FROM pragma_table_info(?1)")?
        .query_map([table], |r| r.get(0))?
        .collect()
}

pub fn sql_column(set: &HashSet<String>, alias: &str, name: &str, fallback: &str) -> String {
    if set.contains(name) {
        format!("{alias}.{name}")
    } else {
        fallback.into()
    }
}

pub fn sql_tables(db: &Connection) -> rusqlite::Result<HashSet<String>> {
    db.prepare("SELECT name FROM sqlite_master WHERE type='table'")?
        .query_map([], |r| r.get(0))?
        .collect()
}

pub fn sql_text(row: &rusqlite::Row<'_>, index: usize) -> Option<String> {
    match row.get_ref(index).ok()? {
        rusqlite::types::ValueRef::Text(bytes) => std::str::from_utf8(bytes)
            .ok()
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        rusqlite::types::ValueRef::Integer(n) => Some(n.to_string()),
        _ => None,
    }
}

pub fn sql_count(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    match row.get_ref(index)? {
        rusqlite::types::ValueRef::Null => Ok(0),
        rusqlite::types::ValueRef::Integer(n) if n >= 0 => Ok(n as u64),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

pub fn sql_time(row: &rusqlite::Row<'_>, index: usize, seconds: bool) -> Option<i64> {
    let n = match row.get_ref(index).ok()? {
        rusqlite::types::ValueRef::Integer(n) => n as f64,
        rusqlite::types::ValueRef::Real(n) => n,
        _ => return None,
    };
    (n.is_finite() && n > 0.0).then_some((n * if seconds { 1000.0 } else { 1.0 }) as i64)
}

pub fn sql_model(value: Option<String>) -> String {
    value.unwrap_or_else(|| "Unknown".into())
}

pub fn unsigned(value: &Value) -> u64 {
    value.as_u64().unwrap_or(0)
}

impl Log {
    pub fn own_id(&mut self, raw: &str) {
        self.id = format!("{}:{raw}", self.source);
        self.observed = true;
    }

    pub fn clock(&mut self, at: i64) {
        let date = iso(at);
        if date > self.updated {
            self.updated = date;
        }
    }

    /// Replace this contribution with a new authoritative set of usage facts.
    pub fn clear_usage(&mut self) {
        self.messages.clear();
        self.usage = Usage::default();
        self.turns.clear();
        self.buckets.clear();
        self.meter_events.clear();
    }

    /// Corrections replace the same source response, not a second usage event.
    pub fn count_source(&mut self, key: String, model: &str, at: i64, next: Usage, maximum: bool) {
        self.count_timed_source(key, model, at, next, maximum, None);
    }

    /// `count_source` for a call whose log records how long it took.
    pub fn count_timed_source(
        &mut self,
        key: String,
        model: &str,
        at: i64,
        next: Usage,
        maximum: bool,
        duration_ms: Option<u64>,
    ) {
        if next.total() == 0 && !self.messages.contains_key(&key) {
            return;
        }
        let previous = self.messages.get(&key).copied().unwrap_or_default();
        let next = if maximum { previous.max(next) } else { next };
        self.usage.input = self.usage.input.saturating_sub(previous.input) + next.input;
        self.usage.output = self.usage.output.saturating_sub(previous.output) + next.output;
        self.usage.read = self.usage.read.saturating_sub(previous.read) + next.read;
        self.usage.write = self.usage.write.saturating_sub(previous.write) + next.write;
        self.usage.reasoning =
            self.usage.reasoning.saturating_sub(previous.reasoning) + next.reasoning;
        self.messages.insert(key.clone(), next);
        self.observed = true;
        self.model = model.split('#').next().unwrap_or(model).into();
        self.clock(at);
        let hash = metering::event_key(&key);
        self.meter_events.insert(
            hash.clone(),
            metering::Event {
                key: hash,
                model: model.into(),
                at,
                usage: next,
                duration_ms,
                ..Default::default()
            },
        );
        let turn = json!({"id":key,"index":self.messages.len().saturating_sub(1),"timestamp":iso(at),"input":next.input,"output":next.output+next.reasoning,"cache":next.read+next.write,"context":null,"message":null});
        if let Some(index) = self.turns.iter().position(|old| old["id"] == turn["id"]) {
            let old = std::mem::replace(&mut self.turns[index], turn.clone());
            self.note_bucket(&old, false);
            self.note_bucket(&turn, true);
        } else {
            self.push_turn(turn);
        }
    }

    pub fn parse_source(&mut self, v: &Value) {
        match self.source.as_str() {
            "pi" => self.pi(v),
            "qwen" => self.qwen(v),
            "codebuddy" | "workbuddy" => self.buddy(v),
            "kimi" => self.kimi(v),
            "openclaw" => self.openclaw(v),
            "deepseek" => self.deepseek(v),
            "copilot" => self.copilot(v),
            "grok-cli" => self.grok(v),
            _ => {}
        }
    }
}
