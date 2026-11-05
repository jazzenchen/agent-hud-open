//! Quota samples, and the rows the statistics and billing metrics read.

use super::{unsigned, usage};
use crate::l10n;
use crate::Usage;
use rusqlite::{params, Connection};
use serde_json::json;
use std::collections::HashMap;

/// A window's reading: what remained when, the plan it was read under
/// (`plan_value::plan_key`) where the account named one, and the reset it
/// reported.
pub(crate) fn insert_quota_sample(
    db: &Connection,
    window_id: &str,
    observed_at_ms: i64,
    remaining: f64,
    plan: Option<&str>,
    reset_at_ms: Option<i64>,
) -> rusqlite::Result<usize> {
    db.execute(
        "INSERT OR IGNORE INTO quota_sample(window_id,observed_at_ms,remaining,plan,reset_at_ms) VALUES(?1,?2,?3,?4,?5)",
        params![window_id, observed_at_ms, remaining, plan, reset_at_ms],
    )
}

pub(crate) fn prune_quota_samples(db: &Connection, cutoff_ms: i64) -> rusqlite::Result<usize> {
    db.execute(
        "DELETE FROM quota_sample WHERE observed_at_ms<?1",
        [cutoff_ms],
    )
}

pub(crate) type Samples = HashMap<String, Vec<(i64, f64)>>;

pub(crate) fn samples(db: &Connection, now: i64) -> rusqlite::Result<Samples> {
    let mut samples = Samples::new();
    {
        let mut s = db.prepare(
            "SELECT window_id,observed_at_ms,remaining FROM quota_sample WHERE observed_at_ms>=?1 AND observed_at_ms<=?2 ORDER BY observed_at_ms",
        )?;
        let rows = s.query_map(params![now - 30 * 86_400_000, now], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, f64>(2)?,
            ))
        })?;
        for row in rows {
            let (id, at, remaining) = row?;
            samples.entry(id).or_default().push((at, remaining));
        }
    }
    Ok(samples)
}

pub(crate) struct EventRow {
    pub source: String,
    pub model: String,
    pub at: i64,
    pub usage: Usage,
    pub gap: Option<i64>,
    /// How long the call itself took, where its log records it.
    pub duration: Option<i64>,
}

pub(crate) fn query_events_between(
    db: &Connection,
    selected_paths: &[String],
    from_ms: i64,
    to_ms: i64,
) -> rusqlite::Result<Vec<EventRow>> {
    let mut statement = db.prepare(
        "WITH selected(contribution) AS (SELECT value FROM json_each(?1))
         SELECT source,model,at_ms,input,output,cache_read,cache_write,reasoning
         FROM usage_meter_event JOIN selected USING(contribution)
         WHERE at_ms>=?2 AND at_ms<?3
         ORDER BY at_ms,event_key",
    )?;
    let mut rows = statement.query(params![json!(selected_paths).to_string(), from_ms, to_ms])?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        result.push(EventRow {
            source: row.get(0)?,
            model: row.get(1)?,
            at: row.get(2)?,
            usage: usage(row, 3)?,
            gap: None,
            duration: None,
        });
    }
    Ok(result)
}

pub(crate) fn query_deepseek_billing_events(
    db: &Connection,
    paths: &[String],
    since_ms: i64,
) -> rusqlite::Result<Vec<EventRow>> {
    let mut s = db.prepare(
        "WITH selected(contribution) AS(SELECT value FROM json_each(?1))
         SELECT model,at_ms,input,output,cache_read,cache_write,reasoning,gap_ms,duration_ms
         FROM usage_meter_event JOIN selected USING(contribution)
         WHERE source='deepseek' AND model LIKE '%#deepseek-official' AND at_ms>=?2
         ORDER BY at_ms,event_key",
    )?;
    let mut rows = s.query(params![json!(paths).to_string(), since_ms])?;
    let mut events = Vec::new();
    while let Some(r) = rows.next()? {
        events.push(EventRow {
            source: "deepseek".into(),
            model: r.get(0)?,
            at: r.get(1)?,
            usage: usage(r, 2)?,
            gap: r.get(7)?,
            duration: r.get(8)?,
        });
    }
    Ok(events)
}

pub(crate) struct StatsBucketRow {
    pub contribution: String,
    pub source: String,
    pub model: String,
    pub at: i64,
    pub tokens: Usage,
}

pub(crate) fn query_statistics_data(
    db: &Connection,
    paths: &[String],
    from: i64,
    to: i64,
) -> Result<
    (
        Vec<(String, String, i64, i64)>,
        Vec<StatsBucketRow>,
        Vec<(String, String, i64, u64)>,
    ),
    String,
> {
    let sql_error = |_: rusqlite::Error| -> String {
        l10n::text(
            "无法从本地用量账本读取统计",
            "Statistics could not be read from the local usage ledger",
        )
    };
    let selected = json!(paths).to_string();
    let mut model_bounds = Vec::new();
    {
        let mut s = db
            .prepare(
                "WITH selected(contribution) AS (SELECT value FROM json_each(?1))
                 SELECT source,model,min(at_ms),max(at_ms)
                 FROM usage_meter_bucket JOIN selected USING(contribution)
                 WHERE input+output+cache_read+cache_write+reasoning>0
                 GROUP BY source,model",
            )
            .map_err(sql_error)?;
        let rows = s
            .query_map([&selected], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            model_bounds.push(row.map_err(sql_error)?);
        }
    }
    let mut bucket_rows = Vec::new();
    {
        let mut s = db
            .prepare(
                "WITH selected(contribution) AS (SELECT value FROM json_each(?1))
                 SELECT contribution,source,model,at_ms,input,output,cache_read,cache_write,reasoning
                 FROM usage_meter_bucket JOIN selected USING(contribution)
                 WHERE at_ms>=?2 AND at_ms<?3 AND input+output+cache_read+cache_write+reasoning>0
                 ORDER BY at_ms",
            )
            .map_err(sql_error)?;
        let rows = s
            .query_map(params![selected, from, to], |r| {
                Ok(StatsBucketRow {
                    contribution: r.get(0)?,
                    source: r.get(1)?,
                    model: r.get(2)?,
                    at: r.get(3)?,
                    tokens: usage(r, 4)?,
                })
            })
            .map_err(sql_error)?;
        for row in rows {
            bucket_rows.push(row.map_err(sql_error)?);
        }
    }
    let mut unknown_rows = Vec::new();
    {
        let mut s = db
            .prepare(
                "WITH selected(contribution) AS (SELECT value FROM json_each(?1))
                 SELECT source,model,(at_ms/900000)*900000,count(*)
                 FROM usage_meter_event JOIN selected USING(contribution)
                 WHERE at_ms>=?2 AND at_ms<?3 AND (model='' OR lower(model)='unknown')
                   AND input+output+cache_read+cache_write+reasoning>0
                 GROUP BY source,model,(at_ms/900000)*900000",
            )
            .map_err(sql_error)?;
        let rows = s
            .query_map(params![selected, from, to], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, unsigned(r, 3)?))
            })
            .map_err(sql_error)?;
        for row in rows {
            unknown_rows.push(row.map_err(sql_error)?);
        }
    }
    Ok((model_bounds, bucket_rows, unknown_rows))
}
