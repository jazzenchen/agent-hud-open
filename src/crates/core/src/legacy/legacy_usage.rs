//! Reuse proven original transcript facts without rereading their multi-GB logs.
//!
//! The v5 Claude/v6 Codex state records retained-history completion separately
//! from its tail offset. Only an unchanged, fully committed state with no
//! pending/carried history can seed a matching complete checkpoint here.
//! Source event IDs are stable FNV-1a business keys, not SQLite row IDs.
use crate::legacy_database::ReadOnlyDatabase;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) struct Candidate {
    pub path: PathBuf,
    pub source: String,
    pub offset: u64,
    pub length: u64,
    pub modified: SystemTime,
}
/// Raw cumulative Codex counters, before the exclusive-kind conversion.
pub(crate) struct Baseline {
    pub input: u64,
    pub output: u64,
    pub read: u64,
    pub write: u64,
    pub reasoning: u64,
}
pub(crate) struct Imported {
    pub path: PathBuf,
    pub codex_baseline: Option<Baseline>,
}
#[derive(Default)]
pub(crate) struct Report {
    pub imported: Vec<Imported>,
    pub event_count: usize,
}
fn same_millisecond(modified: SystemTime, milliseconds: i64) -> bool {
    let Ok(since_epoch) = modified.duration_since(UNIX_EPOCH) else {
        return false;
    };
    let Ok(nanos) = i128::try_from(since_epoch.as_nanos()) else {
        return false;
    };
    // Swift persists rounded ms; retaining nanos avoids a floor/round mismatch.
    (nanos - i128::from(milliseconds) * 1_000_000).abs() < 1_000_000
}
fn source_signature(text: &str) -> Option<(i64, u64)> {
    let (modified, size) = text.split_once(':')?;
    Some((modified.parse().ok()?, size.parse().ok()?))
}
fn baseline(state: &Value) -> Option<Option<Baseline>> {
    let transcript = state.get("transcript")?.as_object()?;
    if !transcript.get("hasTotals")?.as_bool()? {
        return Some(None);
    }
    Some(Some(Baseline {
        input: transcript.get("totalInput")?.as_u64()?,
        output: transcript.get("totalOutput")?.as_u64()?,
        read: transcript.get("totalCached")?.as_u64()?,
        write: transcript.get("totalWrite")?.as_u64()?,
        reasoning: transcript.get("totalReasoning")?.as_u64()?,
    }))
}
struct Event {
    key: String,
    model: String,
    at: i64,
    input: i64,
    output: i64,
    read: i64,
    write: i64,
    reasoning: i64,
}

/// Caller transaction must cover this import AND the successful checkpoint
/// upgrades from Report, so a crash cannot leave facts with an old checkpoint
/// that will reset/recount them. This never starts or writes the source app.
pub(crate) fn import(
    db: &Connection,
    legacy_path: &Path,
    candidates: &[Candidate],
) -> rusqlite::Result<Report> {
    let mut report = Report::default();
    if candidates.is_empty() || !legacy_path.is_file() {
        return Ok(report);
    }
    if db.is_autocommit() {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let source = ReadOnlyDatabase::open(legacy_path)?;
    let version: i64 = source
        .connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != 4 {
        return Ok(report);
    }
    let mut saved=source.connection.prepare_cached("SELECT signature,state FROM source_file WHERE source=?1 AND path=?2 AND state IS NOT NULL AND length(state)<=8388608")?;
    let mut contribution = source
        .connection
        .prepare_cached("SELECT id,account,counted FROM contribution WHERE source=?1 AND key=?2")?;
    let mut read=source.connection.prepare_cached("SELECT e.event,a.name,e.timestamp_ms,e.tokens_in,e.tokens_out,e.cache_read,e.cache_write,e.reasoning FROM usage_event e JOIN agent a ON a.id=e.agent WHERE e.contribution_id=?1")?;
    let mut insert=db.prepare_cached("INSERT INTO usage_meter_event(contribution,event_key,source,model,at_ms,input,output,cache_read,cache_write,reasoning,gap_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)")?;
    for candidate in candidates {
        let codec = match candidate.source.as_str() {
            "claude" => 5,
            "codex" => 6,
            _ => continue,
        };
        // Exact absolute contribution path, never basename/session-id matching.
        if !candidate.path.is_absolute()
            || candidate.path.extension().and_then(|s| s.to_str()) != Some("jsonl")
            || candidate.offset != candidate.length
        {
            continue;
        }
        let Some(path) = candidate.path.to_str() else {
            continue;
        };
        let previous: Option<(String, Vec<u8>)> = saved
            .query_row(params![candidate.source, path], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()?;
        let Some((signature, bytes)) = previous else {
            continue;
        };
        let Some((modified, size)) = source_signature(&signature) else {
            continue;
        };
        if size != candidate.length || !same_millisecond(candidate.modified, modified) {
            continue;
        }
        let Some(state) = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .filter(Value::is_object)
        else {
            continue;
        };
        if state["version"].as_u64() != Some(codec)
            || state.get("carrying").is_some()
            || state.get("back").is_some_and(|v| !v.is_null())
            || state.get("carried").is_some_and(|v| !v.is_null())
            || state["offset"].as_u64() != Some(size)
            || state["committedSize"].as_u64() != Some(size)
        {
            continue;
        }
        let codex_baseline = if candidate.source == "codex" {
            let Some(baseline) = baseline(&state) else {
                continue;
            };
            baseline
        } else {
            if state["accumulator"]["path"].as_str() != Some(path) {
                continue;
            }
            None
        };
        let scope: Option<(i64, String, i64)> = contribution
            .query_row(params![candidate.source, path], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .optional()?;
        let mut events = Vec::new();
        if let Some((id, account, counted)) = scope {
            // Original native transcript contributions use an empty account.
            // An account-specific/gateway contribution cannot be reassigned.
            if !account.is_empty() || counted != 1 {
                continue;
            }
            let mut rows = read.query([id])?;
            let prefix = format!("{}-model:", candidate.source);
            let mut valid = true;
            while let Some(row) = rows.next()? {
                let key: i64 = row.get(0)?;
                let consumer: String = row.get(1)?;
                let Some(model) = consumer.strip_prefix(&prefix) else {
                    valid = false;
                    break;
                };
                let at: i64 = row.get(2)?;
                let input: i64 = row.get(3)?;
                let output: i64 = row.get(4)?;
                let cache_read: i64 = row.get(5)?;
                let cache_write: i64 = row.get(6)?;
                let reasoning: i64 = row.get(7)?;
                if [at, input, output, cache_read, cache_write, reasoning]
                    .into_iter()
                    .any(|v| v < 0)
                {
                    valid = false;
                    break;
                }
                let write = cache_write.min(input);
                let thought = reasoning.min(output);
                // Already FNV(m:/r:/o:) or FNV(Codex o<start>); never rehash it.
                events.push(Event {
                    key: key.to_string(),
                    model: model.into(),
                    at,
                    input: input - write,
                    output: output - thought,
                    read: cache_read,
                    write,
                    reasoning: thought,
                });
            }
            if !valid {
                continue;
            }
        }
        events.sort_by(|a, b| (a.at, &a.key).cmp(&(b.at, &b.key)));
        // Replace one canonical path, rather than merging old and new keys.
        db.execute_batch("SAVEPOINT legacy_usage_path")?;
        db.execute(
            "DELETE FROM usage_meter_event WHERE contribution=?1",
            [path],
        )?;
        db.execute(
            "DELETE FROM usage_meter_bucket WHERE contribution=?1",
            [path],
        )?;
        let mut previous_at = None;
        for event in &events {
            let gap = previous_at.map(|previous| event.at - previous);
            insert.execute(params![
                path,
                event.key,
                candidate.source,
                event.model,
                event.at,
                event.input,
                event.output,
                event.read,
                event.write,
                event.reasoning,
                gap
            ])?;
            previous_at = Some(event.at);
        }
        db.execute("INSERT INTO usage_meter_bucket(contribution,source,model,at_ms,input,output,cache_read,cache_write,reasoning) SELECT contribution,source,model,(at_ms/900000)*900000,sum(input),sum(output),sum(cache_read),sum(cache_write),sum(reasoning) FROM usage_meter_event WHERE contribution=?1 GROUP BY contribution,source,model,(at_ms/900000)*900000",[path])?;
        db.execute_batch("RELEASE legacy_usage_path")?;
        report.event_count += events.len();
        report.imported.push(Imported {
            path: candidate.path.clone(),
            codex_baseline,
        });
    }
    Ok(report)
}
