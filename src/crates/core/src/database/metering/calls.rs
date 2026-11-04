//! A session's recorded calls and marks, read back.

use super::{call_list_cost, tools_from, unsigned, usage};
use crate::Usage;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

/// The prompt and compaction marks of these logs, each oldest first.
pub(crate) fn marks(db: &Connection, paths: &[String]) -> rusqlite::Result<(Vec<i64>, Vec<i64>)> {
    let selected = json!(paths).to_string();
    let mut statement = db.prepare_cached(
        "SELECT at_ms,kind FROM usage_meter_mark WHERE contribution IN (SELECT value FROM json_each(?1)) ORDER BY at_ms",
    )?;
    let mut rows = statement.query(params![selected])?;
    let (mut prompts, mut compactions) = (Vec::new(), Vec::new());
    while let Some(row) = rows.next()? {
        let at: i64 = row.get(0)?;
        match row.get::<_, i64>(1)? {
            0 => prompts.push(at),
            1 => compactions.push(at),
            _ => {}
        }
    }
    Ok((prompts, compactions))
}

/// One recorded model call of a session's logs.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CallRow {
    pub contribution: String,
    pub key: String,
    pub source: String,
    pub model: String,
    pub at: i64,
    pub usage: Usage,
    pub context_window: Option<u64>,
    pub tools: Vec<String>,
}

/// The calls of these logs from `from` through `through` (all of them for
/// None), oldest first.
pub(crate) fn calls_of(
    db: &Connection,
    paths: &[String],
    from: Option<i64>,
    through: Option<i64>,
) -> rusqlite::Result<Vec<CallRow>> {
    let selected = json!(paths).to_string();
    let mut statement = db.prepare_cached(
        "SELECT contribution,event_key,source,model,at_ms,input,output,cache_read,cache_write,reasoning,context_window,tools
         FROM usage_meter_event WHERE contribution IN (SELECT value FROM json_each(?1))
           AND at_ms BETWEEN ?2 AND ?3
         ORDER BY at_ms,contribution,event_key",
    )?;
    let mut rows = statement.query(params![
        selected,
        from.unwrap_or(i64::MIN),
        through.unwrap_or(i64::MAX)
    ])?;
    let mut calls = Vec::new();
    while let Some(row) = rows.next()? {
        calls.push(CallRow {
            contribution: row.get(0)?,
            key: row.get(1)?,
            source: row.get(2)?,
            model: row.get(3)?,
            at: row.get(4)?,
            usage: usage(row, 5)?,
            context_window: row.get::<_, Option<i64>>(10)?.map(|n| n as u64),
            tools: tools_from(row.get(11)?),
        });
    }
    Ok(calls)
}

/// The prompt (fresh input, cache writes and cache reads) of the latest call
/// in these logs before `before`.
pub(crate) fn prompt_before(
    db: &Connection,
    paths: &[String],
    before: i64,
) -> rusqlite::Result<Option<u64>> {
    let selected = json!(paths).to_string();
    let mut statement = db.prepare_cached(
        "SELECT input+cache_write+cache_read FROM usage_meter_event
         WHERE contribution IN (SELECT value FROM json_each(?1)) AND at_ms<?2
         ORDER BY at_ms DESC,event_key DESC LIMIT 1",
    )?;
    let mut rows = statement.query(params![selected, before])?;
    rows.next()?.map(|row| unsigned(row, 0)).transpose()
}

/// Prompt tokens sent again because the cache no longer held them
/// (agent-hud-native-open `UsageLedger.recachedBuckets`): each call in a log that
/// marks its prompts, against the call before it in that log, by the rule a
/// session's turns count them. By 15-minute bucket, source and model:
/// (bucket, source, model, sent, sent as cache writes).
pub(crate) fn recached(
    db: &Connection,
    paths: &[String],
    since: i64,
) -> rusqlite::Result<Vec<(i64, String, String, u64, u64)>> {
    let selected = json!(paths).to_string();
    let mut statement = db.prepare_cached(
        "SELECT r.bucket,r.source,r.model,SUM(r.sent),SUM(CASE WHEN r.wrote THEN r.sent ELSE 0 END) FROM (
            SELECT (e.at_ms/900000)*900000 AS bucket,e.source,e.model,e.input+e.cache_write AS sent,
                   e.cache_read,e.cache_write>0 AS wrote,e.at_ms,
                   LAG(e.input+e.cache_write+e.cache_read) OVER (PARTITION BY e.contribution ORDER BY e.at_ms,e.event_key) AS previous
            FROM usage_meter_event e
            WHERE e.contribution IN (SELECT value FROM json_each(?1))
              AND EXISTS (SELECT 1 FROM usage_meter_mark m WHERE m.contribution=e.contribution AND m.kind=0)
         ) r
         WHERE r.previous IS NOT NULL AND r.cache_read*2<r.previous AND r.sent*2>=r.previous AND r.at_ms>=?2
         GROUP BY r.bucket,r.source,r.model",
    )?;
    let rows = statement.query_map(
        params![selected, since.div_euclid(900_000) * 900_000],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                unsigned(row, 3)?,
                unsigned(row, 4)?,
            ))
        },
    )?;
    rows.collect()
}

pub(crate) fn contribution_usage(db: &Connection, path: &str) -> rusqlite::Result<Usage> {
    db.query_row(
        "SELECT coalesce(sum(input),0),coalesce(sum(output),0),coalesce(sum(cache_read),0),coalesce(sum(cache_write),0),coalesce(sum(reasoning),0)
         FROM usage_meter_event WHERE contribution=?1",
        [path],
        |row| usage(row, 0),
    )
}

pub(crate) fn session_calls(
    db: &Connection,
    path: &str,
    source: &str,
    offset: u64,
    limit: u64,
) -> Result<(u64, Vec<Value>), String> {
    let total: u64 = db
        .query_row(
            "SELECT COUNT(*) FROM usage_meter_event WHERE contribution=?1",
            [path],
            |row| row.get::<_, i64>(0).map(|n| n as u64),
        )
        .map_err(|_| "Could not count session calls")?;
    let mut statement = db
        .prepare(
            "SELECT event_key,model,at_ms,input,output,cache_read,cache_write,reasoning,tools
             FROM usage_meter_event WHERE contribution=?1
             ORDER BY at_ms DESC,event_key DESC LIMIT ?2 OFFSET ?3",
        )
        .map_err(|_| "Could not read session calls")?;
    let calls = statement
        .query_map(params![path, limit as i64, offset as i64], |row| {
            let used = usage(row, 3)?;
            let Usage { input, output, read, write, reasoning } = used;
            let at: i64 = row.get(2)?;
            let model = row.get::<_, String>(1)?;
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "modelName": (!model.is_empty()).then(|| crate::naming::model_names::consumer_name(&format!("{source}-model:{model}"))),
                "model": model,
                "timestamp": DateTime::<Utc>::from_timestamp_millis(at).map(|at| at.to_rfc3339()),
                "input": input,
                "output": output,
                "cacheRead": read,
                "cacheWrite": write,
                "reasoning": reasoning,
                "total": input + output + read + write + reasoning,
                "own": true,
                "source": source,
                "context": input + write + read,
                "listCost": call_list_cost(source, &model, used).map(|pico| pico as f64 / 1e12),
                "tools": tools_from(row.get(8)?)
            }))
        })
        .map_err(|_| "Could not read session calls")?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Could not decode session calls")?;
    Ok((total, calls))
}
