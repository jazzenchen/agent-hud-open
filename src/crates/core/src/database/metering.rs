//! Persistent quota observations and complete per-call usage facts.
//! All SQLite queries and mutations on `usage_meter_event`, `usage_meter_bucket`,
//! and `quota_sample` live here.

use crate::l10n;
use crate::{iso, pricing, Usage};
pub(crate) use agent_hud_agents::transcript::metering::{date_ms, event_key, Event, Mark};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

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

fn bucket_delta(
    db: &Connection,
    path: &str,
    source: &str,
    model: &str,
    at: i64,
    value: Usage,
    sign: i64,
    updated_at_ms: i64,
) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO usage_meter_bucket(contribution,source,model,at_ms,input,output,cache_read,cache_write,reasoning,updated_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(contribution,model,at_ms) DO UPDATE SET
           input=input+excluded.input,
           output=output+excluded.output,
           cache_read=cache_read+excluded.cache_read,
           cache_write=cache_write+excluded.cache_write,
           reasoning=reasoning+excluded.reasoning,
           updated_at_ms=excluded.updated_at_ms",
        params![
            path,
            source,
            model,
            at.div_euclid(900_000) * 900_000,
            integer(value.input)? * sign,
            integer(value.output)? * sign,
            integer(value.read)? * sign,
            integer(value.write)? * sign,
            integer(value.reasoning)? * sign,
            updated_at_ms
        ],
    )?;
    Ok(())
}

fn repair_gap(db: &Connection, path: &str, key: &str) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE usage_meter_event AS current SET gap_ms=(
            SELECT current.at_ms-previous.at_ms FROM usage_meter_event previous
            WHERE previous.contribution=current.contribution
              AND (previous.at_ms,previous.event_key)<(current.at_ms,current.event_key)
            ORDER BY previous.at_ms DESC,previous.event_key DESC LIMIT 1
         ) WHERE current.contribution=?1 AND current.event_key=?2",
        params![path, key],
    )?;
    Ok(())
}

fn successor(db: &Connection, path: &str, at: i64, key: &str) -> rusqlite::Result<Option<String>> {
    let mut s = db.prepare_cached(
        "SELECT event_key FROM usage_meter_event WHERE contribution=?1 AND (at_ms,event_key)>(?2,?3) ORDER BY at_ms,event_key LIMIT 1",
    )?;
    let mut rows = s.query(params![path, at, key])?;
    rows.next()?.map(|r| r.get(0)).transpose()
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

/// Token history is kept 30 days; rows outlive that by the day that crosses
/// it (agent-hud-native-open `UsageLedger.retention`).
pub(crate) const RETENTION_MS: i64 = 31 * 86_400_000;

/// The oldest time a row may have at `now`, aligned to a bucket so no bucket
/// keeps half its events.
pub(crate) fn retention_cutoff(now: i64) -> i64 {
    (now - RETENTION_MS).div_euclid(900_000) * 900_000
}

/// Deletes the usage rows that left the retention window; the number deleted.
pub(crate) fn expire(db: &Connection, now: i64) -> rusqlite::Result<usize> {
    let cutoff = retention_cutoff(now);
    let events = db.execute(
        "DELETE FROM usage_meter_event WHERE at_ms < ?1",
        params![cutoff],
    )?;
    let buckets = db.execute(
        "DELETE FROM usage_meter_bucket WHERE at_ms < ?1",
        params![cutoff],
    )?;
    let marks = db.execute(
        "DELETE FROM usage_meter_mark WHERE at_ms < ?1",
        params![cutoff],
    )?;
    Ok(events + buckets + marks)
}

pub(crate) fn write(
    db: &Connection,
    path: &str,
    source: &str,
    events: &[Event],
) -> rusqlite::Result<bool> {
    let now_ms = Utc::now().timestamp_millis();
    // Events the next expiry would delete are not written.
    let cutoff = retention_cutoff(now_ms);
    let mut changed = false;
    for event in events.iter().filter(|event| event.at >= cutoff) {
        let old = {
            let mut s = db.prepare_cached(
                "SELECT model,at_ms,input,output,cache_read,cache_write,reasoning,context_window,tools,duration_ms FROM usage_meter_event WHERE contribution=?1 AND event_key=?2",
            )?;
            let mut rows = s.query(params![path, event.key])?;
            rows.next()?
                .map(|r| {
                    Ok::<_, rusqlite::Error>((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        usage(r, 2)?,
                        r.get::<_, Option<i64>>(7)?.map(|n| n as u64),
                        tools_from(r.get::<_, Option<String>>(8)?),
                        r.get::<_, Option<i64>>(9)?.map(|n| n as u64),
                    ))
                })
                .transpose()?
        };
        let model = if event.model.is_empty() {
            old.as_ref().map(|(model, ..)| model.as_str()).unwrap_or("")
        } else {
            &event.model
        }
        .to_owned();
        let same_usage = old
            .as_ref()
            .is_some_and(|(old_model, at, usage, window, tools, _)| {
                *old_model == model
                    && *at == event.at
                    && *usage == event.usage
                    && *window == event.context_window
                    && *tools == event.tools
            });
        if same_usage {
            // A call whose later lines only time it (a Claude response's
            // last block, read a pass after its first) changes no bucket
            // and no gap.
            if old.as_ref().is_some_and(|old| old.5 != event.duration_ms) {
                db.execute(
                    "UPDATE usage_meter_event SET duration_ms=?3,updated_at_ms=?4 WHERE contribution=?1 AND event_key=?2",
                    params![
                        path,
                        event.key,
                        event.duration_ms.map(integer).transpose()?,
                        now_ms
                    ],
                )?;
                changed = true;
            }
            continue;
        }
        changed = true;
        let mut old_next = None;
        if let Some((model, at, old_usage, ..)) = old {
            bucket_delta(db, path, source, &model, at, old_usage, -1, now_ms)?;
            old_next = successor(db, path, at, &event.key)?;
        }
        db.execute(
            "INSERT INTO usage_meter_event(contribution,event_key,source,model,at_ms,input,output,cache_read,cache_write,reasoning,updated_at_ms,context_window,tools,duration_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
             ON CONFLICT(contribution,event_key) DO UPDATE SET
               model=excluded.model,
               at_ms=excluded.at_ms,
               input=excluded.input,
               output=excluded.output,
               cache_read=excluded.cache_read,
               cache_write=excluded.cache_write,
               reasoning=excluded.reasoning,
               updated_at_ms=excluded.updated_at_ms,
               context_window=excluded.context_window,
               tools=excluded.tools,
               duration_ms=excluded.duration_ms",
            params![
                path,
                event.key,
                source,
                model,
                event.at,
                integer(event.usage.input)?,
                integer(event.usage.output)?,
                integer(event.usage.read)?,
                integer(event.usage.write)?,
                integer(event.usage.reasoning)?,
                now_ms,
                event.context_window.map(integer).transpose()?,
                tools_text(&event.tools),
                event.duration_ms.map(integer).transpose()?,
            ],
        )?;
        bucket_delta(db, path, source, &model, event.at, event.usage, 1, now_ms)?;
        note_context(
            db,
            source,
            &model,
            event.usage.input + event.usage.write + event.usage.read,
        )?;
        repair_gap(db, path, &event.key)?;
        if let Some(next) = old_next {
            repair_gap(db, path, &next)?;
        }
        if let Some(next) = successor(db, path, event.at, &event.key)? {
            repair_gap(db, path, &next)?;
        }
    }
    Ok(changed)
}

/// What a log's calls would cost at the vendors' API list prices: what the
/// calls with a list price cost, and how many tokens the others used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct ListCost {
    /// The priced calls' cost in pico US dollars; None when no call has a price.
    pub priced: Option<i128>,
    /// The tokens of the calls with no list price.
    pub unpriced_tokens: u64,
}

impl ListCost {
    /// The whole cost; None when a call has no list price or there was none.
    pub fn complete(&self) -> Option<i128> {
        self.priced.filter(|_| self.unpriced_tokens == 0)
    }
}

/// What a log's calls would cost at the vendors' API list prices: the
/// international list at off-peak rates, in pico US dollars, each call on the
/// tier its prompt reaches (agent-hud-native-open `ListPricing`). A call with
/// no list price leaves the others priced; None when the log has no calls.
pub(crate) fn list_cost(db: &Connection, path: &str) -> rusqlite::Result<Option<ListCost>> {
    let models = {
        let mut statement = db.prepare_cached(
            "SELECT DISTINCT source, model FROM usage_meter_event WHERE contribution=?1",
        )?;
        let rows = statement.query_map(params![path], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if models.is_empty() {
        return Ok(None);
    }
    let mut cost = ListCost::default();
    for (source, model) in models {
        let Some(starts) = pricing::tier_starts(&source, &model) else {
            let tokens: i64 = db
                .prepare_cached(
                    "SELECT COALESCE(SUM(input+output+cache_read+cache_write+reasoning),0)
                     FROM usage_meter_event WHERE contribution=?1 AND source=?2 AND model=?3",
                )?
                .query_row(params![path, source, model], |row| row.get(0))?;
            cost.unpriced_tokens += tokens as u64;
            continue;
        };
        // The tier a call is on: the last start its prompt reaches, -1 for the base rates.
        let tier = starts
            .iter()
            .enumerate()
            .rev()
            .map(|(index, from)| format!("WHEN input+cache_write+cache_read>={from} THEN {index} "))
            .collect::<String>();
        let tier = if tier.is_empty() {
            "-1".to_owned()
        } else {
            format!("CASE {tier}ELSE -1 END")
        };
        let mut statement = db.prepare(&format!(
            "SELECT {tier} AS tier, SUM(input), SUM(output), SUM(cache_read), SUM(cache_write), SUM(reasoning)
             FROM usage_meter_event WHERE contribution=?1 AND source=?2 AND model=?3 GROUP BY tier"
        ))?;
        let mut rows = statement.query(params![path, source, model])?;
        while let Some(row) = rows.next()? {
            let tier = usize::try_from(row.get::<_, i64>(0)?).ok();
            let used = usage(row, 1)?;
            let tokens = price_tokens(used);
            // Session costs are US dollars; a relay that prices in another
            // currency leaves its calls unpriced there.
            match pricing::cost_in_tier(&source, &model, &tokens, tier)
                .filter(|priced| priced.currency == "USD")
            {
                Some(priced) => {
                    cost.priced = Some(cost.priced.unwrap_or(0) + priced.amount_pico);
                }
                None => cost.unpriced_tokens += used.total(),
            }
        }
    }
    Ok(Some(cost))
}

/// Keeps the largest prompt each model was seen with, which tells its context
/// window where no log states it (agent-hud-native-open `model_context`). It is kept
/// past the usage retention.
fn note_context(db: &Connection, source: &str, model: &str, prompt: u64) -> rusqlite::Result<()> {
    if prompt == 0 || model.is_empty() {
        return Ok(());
    }
    db.prepare_cached(
        "INSERT INTO usage_meter_model_context(source,model,largest) VALUES(?1,?2,?3)
         ON CONFLICT(source,model) DO UPDATE SET largest=MAX(largest,excluded.largest)",
    )?
    .execute(params![source, model, integer(prompt)?])?;
    Ok(())
}

/// The largest prompt `model`, used through `source`, was seen with.
pub(crate) fn largest_context(
    db: &Connection,
    source: &str,
    model: &str,
) -> rusqlite::Result<Option<u64>> {
    let mut statement = db.prepare_cached(
        "SELECT largest FROM usage_meter_model_context WHERE source=?1 AND model=?2",
    )?;
    let mut rows = statement.query(params![source, model])?;
    rows.next()?.map(|row| unsigned(row, 0)).transpose()
}

/// Records where a log's turns start and where its client compacted the
/// conversation; marks already recorded stay as they are, so a line read
/// again adds nothing. Marks the next expiry would delete are not written.
pub(crate) fn write_marks(db: &Connection, path: &str, marks: &[Mark]) -> rusqlite::Result<bool> {
    let cutoff = retention_cutoff(Utc::now().timestamp_millis());
    let mut statement = db.prepare_cached(
        "INSERT OR IGNORE INTO usage_meter_mark(contribution,at_ms,kind) VALUES(?1,?2,?3)",
    )?;
    let mut changed = false;
    for mark in marks.iter().filter(|mark| mark.at >= cutoff) {
        changed |= statement.execute(params![path, mark.at, mark.kind as i64])? > 0;
    }
    Ok(changed)
}

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

/// What one call would cost at the international list price, off-peak, on
/// the tier its prompt reaches, in pico US dollars (agent-hud-native-open
/// `ModelCatalog.cost(agentId:kinds:)`).
pub(crate) fn call_list_cost(source: &str, model: &str, used: Usage) -> Option<i128> {
    let starts = pricing::tier_starts(source, model)?;
    let tier = pricing::tier_of(&starts, used.input + used.write + used.read);
    pricing::cost_in_tier(source, model, &price_tokens(used), tier)
        .filter(|cost| cost.currency == "USD")
        .map(|cost| cost.amount_pico)
}

/// A ledger row's tokens as the price list counts them: input with cache
/// writes, output with reasoning.
pub(crate) fn price_tokens(used: Usage) -> pricing::PriceTokens {
    pricing::PriceTokens {
        tokens_in: used.input + used.write,
        tokens_out: used.output + used.reasoning,
        cache_read: used.read,
        cache_write: used.write,
        reasoning: used.reasoning,
    }
}

/// What a DeepSeek log's calls cost at the account's own prices
/// (agent-hud-native-open `UsageLedger.contributionCosts`): each call at its tier and
/// at DeepSeek's peak rates when made in Beijing working hours, in pico US
/// dollars on the international list and pico yuan on the China list. A
/// currency is None when a call has no price in it; None for a log of
/// another client or without calls.
pub(crate) fn account_cost(
    db: &Connection,
    path: &str,
    source: &str,
) -> rusqlite::Result<Option<[Option<i128>; 2]>> {
    if source != "deepseek" {
        return Ok(None);
    }
    let mut statement = db.prepare_cached(
        "SELECT model,at_ms,input,output,cache_read,cache_write,reasoning FROM usage_meter_event WHERE contribution=?1",
    )?;
    let mut rows = statement.query(params![path])?;
    let mut totals = [Some(0i128), Some(0i128)];
    let mut any = false;
    while let Some(row) = rows.next()? {
        any = true;
        let model: String = row.get(0)?;
        let at: i64 = row.get(1)?;
        let tokens = price_tokens(usage(row, 2)?);
        for (total, region) in totals
            .iter_mut()
            .zip([pricing::Region::International, pricing::Region::China])
        {
            let cost = pricing::cost_at(source, &model, &tokens, true, region, Some(at))
                .filter(|cost| cost.currency == region.currency());
            *total = total
                .zip(cost)
                .map(|(total, cost)| total + cost.amount_pico);
        }
    }
    Ok(any.then_some(totals))
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

/// A window's reading: what remained when, and the plan it was read under
/// (`plan_value::plan_key`), where the account named one.
pub(crate) fn insert_quota_sample(
    db: &Connection,
    window_id: &str,
    observed_at_ms: i64,
    remaining: f64,
    plan: Option<&str>,
) -> rusqlite::Result<usize> {
    db.execute(
        "INSERT OR IGNORE INTO quota_sample(window_id,observed_at_ms,remaining,plan) VALUES(?1,?2,?3,?4)",
        params![window_id, observed_at_ms, remaining, plan],
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

impl Reply {
    /// A call read as at_ms, duration_ms, gap_ms, output + reasoning from
    /// column `at`; None when it does not count.
    fn read(key: u32, r: &rusqlite::Row<'_>, at: usize) -> rusqlite::Result<Option<Self>> {
        let call_at: i64 = r.get(at)?;
        let tokens = unsigned(r, at + 3)?;
        Ok(
            reply_span(tokens, r.get(at + 1)?, r.get(at + 2)?).map(|(span, timed)| Self {
                key,
                start: call_at - span as i64,
                at: call_at,
                tokens,
                span,
                timed,
            }),
        )
    }
}

const WEEK_MS: i64 = 7 * 86_400_000;
/// How many points a window's use may stray from its budget, either way,
/// and still be on track (`View::metrics`).
const BUDGET_SLACK: f64 = 2.0;
/// The share of a cycle that has to go by before its use so far, on a
/// straight line, makes a forecast while recent readings are too few.
const WINDOW_BASIS_FROM: f64 = 0.03;
/// The fewest calls, and the least time they took in all, a reply rate is
/// worked out from (`reply_rate`): fewer is noise, so no rate.
const REPLY_RATE_CALLS: u64 = 3;
const REPLY_RATE_SPAN_MS: u64 = 5_000;
/// How far back an agent's recent usage reaches (`View::recent`), as the
/// hover panel's token chart reaches back 24 hours.
const RECENT_MS: i64 = 86_400_000;

/// What one client's calls in the last 24 hours used and cost at list
/// prices (`View::recent`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Recent {
    /// Every kind, cache reads included, as the token chart counts them.
    pub tokens: u64,
    /// The calls with a list price, in pico units by currency.
    pub costs: BTreeMap<String, i128>,
    /// The tokens of the calls with no list price.
    pub unpriced_tokens: u64,
}

impl Recent {
    pub fn json(&self) -> Value {
        json!({
            "tokens": self.tokens,
            "costs": self.costs.iter()
                .map(|(currency, pico)| (currency.clone(), *pico as f64 / 1e12))
                .collect::<BTreeMap<_, _>>(),
            "unpricedTokens": self.unpriced_tokens,
        })
    }
}

/// The calls the reply rate reads, as SQL over `usage_meter_event`: what
/// `reply_span` keeps, before its rate check.
const REPLY_ROWS: &str = "output+reasoning>=10 AND (duration_ms BETWEEN 100 AND 600000
     OR (duration_ms IS NULL AND gap_ms BETWEEN 100 AND 120000))";

/// How a call counts toward the reply rate: its span and whether its log
/// timed it. A call the log timed counts its own time, up to 10 minutes; one
/// it did not counts the gap since its log's previous call, up to 120 s,
/// since a longer gap is the person's or a tool's. None for a call of under
/// 10 output tokens, a span under 100 ms, or over 10,000 tokens per second.
pub(crate) fn reply_span(
    tokens: u64,
    duration: Option<i64>,
    gap: Option<i64>,
) -> Option<(u64, bool)> {
    let (span, timed, cap) = match duration {
        Some(duration) => (duration, true, 600_000),
        None => (gap?, false, 120_000),
    };
    let span = u64::try_from(span)
        .ok()
        .filter(|span| (100..=cap).contains(span))?;
    (tokens >= 10 && tokens <= span.saturating_mul(10)).then_some((span, timed))
}

/// The reply rate of the calls in `sums` ([gap-timed, log-timed] output,
/// span and calls): the calls their logs timed when there are enough of
/// them, else the gap-timed ones, whose spans hold tool runs and the
/// person's time, so the rate is a lower bound. Enough is at least
/// `REPLY_RATE_CALLS` calls taking `REPLY_RATE_SPAN_MS` in all; with too few
/// either way there is no rate, which is unknown, not zero.
pub(crate) fn reply_rate(sums: [(u64, u64, u64); 2]) -> Option<Value> {
    let enough =
        |(_, span, calls): (u64, u64, u64)| calls >= REPLY_RATE_CALLS && span >= REPLY_RATE_SPAN_MS;
    let (lower_bound, (output, span, calls)) = if enough(sums[1]) {
        (false, sums[1])
    } else if enough(sums[0]) {
        (true, sums[0])
    } else {
        return None;
    };
    Some(
        json!({"tokensPerSecond":output as f64/(span as f64/1000.0),"calls":calls,"tokensOut":output,"lowerBound":lower_bound}),
    )
}

impl View {
    pub fn load(
        db: &Connection,
        paths: &[String],
        now: i64,
        regions: &crate::pricing::PriceRegions,
    ) -> rusqlite::Result<Self> {
        {
            let transaction = db.unchecked_transaction()?;
            transaction.execute("DELETE FROM meter_selected", [])?;
            {
                let mut statement = transaction
                    .prepare_cached("INSERT INTO meter_selected(contribution) VALUES(?1)")?;
                for path in paths {
                    statement.execute([path])?;
                }
            }
            transaction.commit()?;
        }
        let mut selection = paths.to_vec();
        selection.sort_unstable();
        let mut view = Self {
            loaded_at: now,
            selection,
            ..Self::default()
        };
        {
            let mut s = db.prepare(
                "SELECT contribution,source,model,at_ms,input,output,cache_read,cache_write,reasoning
                 FROM usage_meter_bucket JOIN meter_selected USING(contribution)",
            )?;
            let mut rows = s.query([])?;
            while let Some(r) = rows.next()? {
                let key = view.key(r.get(1)?, r.get(2)?);
                let row = (key, r.get(3)?, usage(r, 4)?);
                view.parts.entry(r.get(0)?).or_default().buckets.push(row);
            }
        }
        {
            let mut s = db.prepare(&format!(
                "SELECT contribution,source,model,at_ms,duration_ms,gap_ms,output+reasoning
                 FROM meter_selected CROSS JOIN usage_meter_event USING(contribution)
                 WHERE at_ms>=?1 AND {REPLY_ROWS}"
            ))?;
            let mut rows = s.query([now - WEEK_MS])?;
            while let Some(r) = rows.next()? {
                let key = view.key(r.get(1)?, r.get(2)?);
                if let Some(reply) = Reply::read(key, r, 3)? {
                    view.parts.entry(r.get(0)?).or_default().replies.push(reply);
                }
            }
        }
        {
            let mut s = db.prepare(
                "SELECT contribution,at_ms FROM usage_meter_event JOIN meter_selected USING(contribution) WHERE at_ms>?1",
            )?;
            let mut rows = s.query([now])?;
            while let Some(r) = rows.next()? {
                view.parts
                    .entry(r.get(0)?)
                    .or_default()
                    .later
                    .push(r.get(1)?);
            }
        }
        view.aggregate(regions);
        view.refresh_clock(now);
        Ok(view)
    }

    /// Reads the changed logs again; another selection reads every log.
    pub fn update(
        &mut self,
        db: &Connection,
        paths: &[String],
        changed: &HashSet<String>,
        now: i64,
        regions: &crate::pricing::PriceRegions,
    ) -> rusqlite::Result<()> {
        // The selection comes ordered by activity; the view holds it sorted.
        let mut sorted = paths.iter().collect::<Vec<_>>();
        sorted.sort_unstable();
        if !sorted.iter().copied().eq(self.selection.iter()) {
            *self = Self::load(db, paths, now, regions)?;
            return Ok(());
        }
        for path in changed {
            if self.selection.binary_search(path).is_err() {
                continue;
            }
            let mut part = Part::default();
            {
                let mut s = db.prepare_cached(
                    "SELECT source,model,at_ms,input,output,cache_read,cache_write,reasoning
                     FROM usage_meter_bucket WHERE contribution=?1",
                )?;
                let mut rows = s.query([path])?;
                while let Some(r) = rows.next()? {
                    let key = self.key(r.get(0)?, r.get(1)?);
                    part.buckets.push((key, r.get(2)?, usage(r, 3)?));
                }
            }
            {
                let mut s = db.prepare_cached(&format!(
                    "SELECT source,model,at_ms,duration_ms,gap_ms,output+reasoning FROM usage_meter_event
                     WHERE contribution=?1 AND at_ms>=?2 AND {REPLY_ROWS}"
                ))?;
                let mut rows = s.query(params![path, now - WEEK_MS])?;
                while let Some(r) = rows.next()? {
                    let key = self.key(r.get(0)?, r.get(1)?);
                    if let Some(reply) = Reply::read(key, r, 2)? {
                        part.replies.push(reply);
                    }
                }
            }
            {
                let mut s = db.prepare_cached(
                    "SELECT at_ms FROM usage_meter_event WHERE contribution=?1 AND at_ms>?2",
                )?;
                let mut rows = s.query(params![path, now])?;
                while let Some(r) = rows.next()? {
                    part.later.push(r.get(0)?);
                }
            }
            self.parts.insert(path.clone(), part);
        }
        self.aggregate(regions);
        self.refresh_clock(now);
        Ok(())
    }

    fn key(&mut self, source: String, model: String) -> u32 {
        let pair = (source, model);
        if let Some(index) = self.key_index.get(&pair) {
            return *index;
        }
        let index = self.keys.len() as u32;
        self.keys.push(pair.clone());
        self.key_index.insert(pair, index);
        index
    }

    /// The selected logs' buckets summed by source, model and start, as
    /// `SUM … GROUP BY source,model,at_ms HAVING total>0 ORDER BY at_ms`, priced.
    fn aggregate(&mut self, regions: &crate::pricing::PriceRegions) {
        let mut sums = BTreeMap::<(i64, u32), Usage>::new();
        for part in self.parts.values() {
            for (key, at, u) in &part.buckets {
                sums.entry((*at, *key)).or_default().add(*u);
            }
        }
        self.buckets = sums
            .into_iter()
            .filter(|(_, u)| u.total() > 0)
            .map(|((at, key), u)| {
                let (source, model) = &self.keys[key as usize];
                let tokens = price_tokens(u);
                let cost = pricing::cost_at(
                    source,
                    model,
                    &tokens,
                    false,
                    regions.region(source, model),
                    Some(at),
                );
                Bucket {
                    source: source.clone(),
                    model: model.clone(),
                    at,
                    usage: u,
                    cost,
                }
            })
            .collect();
    }

    /// Reply spans of the last hour and week at `now`, and the next moment
    /// either sum changes: when a span leaves the hour or the week, or a call
    /// stamped ahead of the clock comes due.
    pub fn refresh_clock(&mut self, now: i64) {
        let hour = now - 3_600_000;
        let week = now - WEEK_MS;
        let mut sums = HashMap::<(u32, bool, bool), (u64, u64, u64)>::new();
        let mut valid_until: Option<i64> = None;
        let mut bound =
            |at: i64| valid_until = Some(valid_until.map_or(at, |old: i64| old.min(at)));
        for part in self.parts.values_mut() {
            // A span that has left the week never counts again.
            part.replies
                .retain(|reply| reply.start >= week && reply.at >= week);
            for reply in &part.replies {
                if reply.at > now {
                    continue;
                }
                let recent = reply.start >= hour;
                let sum = sums.entry((reply.key, recent, reply.timed)).or_default();
                sum.0 += reply.tokens;
                sum.1 += reply.span;
                sum.2 += 1;
                if recent {
                    bound(reply.start + 3_600_001);
                }
                bound(reply.start + 604_800_001);
            }
            if let Some(next) = part.later.iter().copied().filter(|at| *at > now).min() {
                bound(next);
            }
        }
        self.replies = sums
            .into_iter()
            .map(|((key, recent, timed), sum)| {
                let (source, model) = &self.keys[key as usize];
                ((source.clone(), model.clone(), recent, timed), sum)
            })
            .collect();
        self.valid_until = valid_until;
        self.loaded_at = now;
    }

    /// The models the selected logs' calls used since `since`, by client
    /// source and model as recorded, with their tokens.
    pub fn models_since(&self, since: i64) -> BTreeMap<(String, String), u64> {
        let mut models = BTreeMap::<(String, String), u64>::new();
        for bucket in self.buckets.iter().filter(|bucket| bucket.at >= since) {
            *models
                .entry((bucket.source.clone(), bucket.model.clone()))
                .or_default() += bucket.usage.total();
        }
        models
    }

    /// The client sources whose calls since `since` went through each of
    /// `routes` (what follows `#` in a recorded model), by route.
    pub fn sources_by_route(
        &self,
        since: i64,
        routes: &BTreeSet<&str>,
    ) -> BTreeMap<String, BTreeSet<String>> {
        let mut found = BTreeMap::<String, BTreeSet<String>>::new();
        if routes.is_empty() {
            return found;
        }
        for bucket in self.buckets.iter().filter(|bucket| bucket.at >= since) {
            if let Some((_, route)) = bucket.model.split_once('#') {
                if routes.contains(route)
                    && !found
                        .get(route)
                        .is_some_and(|known| known.contains(&bucket.source))
                {
                    found
                        .entry(route.to_owned())
                        .or_default()
                        .insert(bucket.source.clone());
                }
            }
        }
        found
    }

    /// Each client's calls in the 24 hours before `now`, by the client's
    /// source id: their tokens and what they cost at list prices, from the
    /// selected logs' buckets already in memory.
    pub fn recent(&self, now: i64) -> HashMap<String, Recent> {
        let mut recent = HashMap::<String, Recent>::new();
        for bucket in self
            .buckets
            .iter()
            .filter(|bucket| bucket.at > now - RECENT_MS && bucket.at <= now)
        {
            let tokens = bucket.usage.total();
            let sums = recent.entry(bucket.source.clone()).or_default();
            sums.tokens += tokens;
            match &bucket.cost {
                Some(cost) => {
                    *sums.costs.entry(cost.currency.clone()).or_default() += cost.amount_pico;
                }
                None => sums.unpriced_tokens += tokens,
            }
        }
        recent
    }

    /// What the calls of `sources` from `since` to `now` used and cost at
    /// list prices, leaving out the calls recorded through a route (a relay
    /// or a coding plan, which is paid for apart from any subscription), and
    /// when the selected logs' buckets begin.
    pub fn spent(&self, sources: &[&str], since: i64, now: i64) -> (Recent, Option<i64>) {
        let mut spent = Recent::default();
        for bucket in self.buckets.iter().filter(|bucket| {
            bucket.at >= since
                && bucket.at <= now
                && sources.contains(&bucket.source.as_str())
                && !bucket.model.contains('#')
        }) {
            let tokens = bucket.usage.total();
            spent.tokens += tokens;
            match &bucket.cost {
                Some(cost) => {
                    *spent.costs.entry(cost.currency.clone()).or_default() += cost.amount_pico;
                }
                None => spent.unpriced_tokens += tokens,
            }
        }
        (spent, self.buckets.first().map(|bucket| bucket.at))
    }

    pub fn metrics(
        &self,
        row: &Value,
        now: i64,
        complete: bool,
        reply_complete: bool,
        samples: &Samples,
    ) -> Value {
        let reset = row["resetAt"].as_str().and_then(date_ms);
        let duration = row["windowDurationSeconds"].as_f64().filter(|d| *d > 0.0);
        let cycle = reset
            .zip(duration)
            .filter(|(reset, duration)| now < *reset && now >= *reset - (*duration * 1000.0) as i64)
            .map(|(reset, duration)| (reset - (duration * 1000.0) as i64, reset, duration));
        let remaining = row["usedPercent"]
            .as_f64()
            .map(|v| (100.0 - v).clamp(0.0, 100.0));
        let mut rate = None;
        let mut sampling = Value::Null;
        if let (Some((start, end, duration)), Some(rem)) = (cycle, remaining) {
            let interval = if duration >= 86_400.0 {
                3600.0
            } else {
                900.0f64.min(duration / 2.0)
            };
            let points = samples
                .get(row["id"].as_str().unwrap_or(""))
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let current = points
                .iter()
                .copied()
                .filter(|(at, _)| *at >= start && *at < end && *at <= now)
                .collect::<Vec<_>>();
            let segment = current
                .windows(2)
                .rposition(|v| v[1].1 - v[0].1 >= 5.0)
                .map_or(0, |i| i + 1);
            let current = &current[segment..];
            if let Some(first) = current.first().copied() {
                let mut sampled = vec![first];
                let mut bins = BTreeMap::new();
                for point in current {
                    bins.insert(
                        ((point.0 - start) as f64 / (interval * 1000.0)).floor() as i64,
                        *point,
                    );
                }
                for point in bins.into_values() {
                    if point.0 > sampled.last().unwrap().0 {
                        sampled.push(point);
                    }
                }
                let last = *sampled.last().unwrap();
                let elapsed = (last.0 - first.0) as f64 / 1000.0;
                if elapsed < interval {
                    if rem > 0.5 {
                        sampling = json!({"elapsed":elapsed,"required":interval,"afterReset":points.iter().any(|p|p.0<first.0)});
                    }
                } else {
                    let pace = if duration >= 28.0 * 86400.0 {
                        7.0 * 86400.0
                    } else if duration >= 7.0 * 86400.0 {
                        86400.0
                    } else {
                        duration / 5.0
                    };
                    let cutoff = last.0 - (pace * 1000.0) as i64;
                    let first = sampled
                        .iter()
                        .rev()
                        .find(|p| p.0 <= cutoff)
                        .copied()
                        .unwrap_or(first);
                    let hours = (last.0 - first.0) as f64 / 3_600_000.0;
                    if hours * 3600.0 >= interval {
                        rate = Some((first.1 - last.1).max(0.0) / hours);
                    }
                }
            }
        }
        let used = remaining.map(|remaining| 100.0 - remaining);
        // How far into its cycle the window is, 0 to 1.
        let elapsed = cycle.map(|(start, _, duration)| (now - start) as f64 / (duration * 1000.0));
        // The budget: what a steady pace from nothing at the cycle's start to
        // the whole window at its reset would have used by now.
        let budget = elapsed.zip(used).map_or(Value::Null, |(elapsed, used)| {
            let expected = elapsed * 100.0;
            let delta = used - expected;
            json!({"expectedUsedPercent":expected,"paceDelta":delta,"status":if delta > BUDGET_SLACK {"ahead"} else if delta < -BUDGET_SLACK {"behind"} else {"onTrack"}})
        });
        // The pace a forecast runs on: this cycle's recent readings, else,
        // until they span enough time, the cycle so far on a straight line,
        // once 3% of it has gone by.
        let pace = rate.map(|rate| (rate, "recent")).or_else(|| {
            let ((start, _, _), used) = cycle.zip(used)?;
            elapsed
                .filter(|elapsed| *elapsed >= WINDOW_BASIS_FROM)
                .map(|_| (used / ((now - start) as f64 / 3_600_000.0), "window"))
        });
        let mut outlook = if reset.is_none() {
            json!({"kind":"untimed"})
        } else if remaining.is_some_and(|r| r <= 0.5) {
            json!({"kind":"exhausted"})
        } else if cycle.is_none() || remaining.is_none() {
            json!({"kind":"no-estimate"})
        } else {
            json!({"kind":"insufficient-data"})
        };
        if let (Some((rate, basis)), Some(remaining), Some(reset), Some(_)) =
            (pace, remaining, reset, cycle)
        {
            if remaining > 0.5 && rate <= 0.0 {
                outlook = json!({"kind":"no-usage","basis":basis,"ratePctPerHour":0.0});
            } else if remaining > 0.5 {
                // From the time the metrics are worked out at, as native's Mac
                // rows have it: a reading with no newer one never forecasts a
                // run-out that has already passed.
                let interval = remaining / rate * 3_600_000.0;
                outlook = json!({"kind":"exhausts","basis":basis,"ratePctPerHour":rate,"exhaustsAt":now.checked_add(interval as i64).map(iso),"beforeReset":interval<(reset-now)as f64,"projectedUsedPercent":(100.0-remaining+rate*(reset-now)as f64/3_600_000.0).min(100.0)});
            }
        }
        let source = row["consumerSource"].as_str();
        let families = row["modelFamilies"].as_array();
        let matches = |source_value: &str, model: &str| {
            source == Some(source_value)
                && (row["allModels"] == true
                    || families.is_some_and(|values| {
                        values.iter().any(|family| {
                            family
                                .as_str()
                                .is_some_and(|family| model_family(model) == Some(family))
                        })
                    }))
        };
        let consumer = self.buckets.iter().any(|b| matches(&b.source, &b.model));
        let mut token_rate = None;
        let mut consumed = Value::Null;
        let mut cost = Value::Null;
        let mut reply = Value::Null;
        let mut observed = Value::Null;
        let mut unavailable = serde_json::Map::new();
        let reason = if !complete {
            l10n::text(
                "本地用量账本仍在索引",
                "The local usage ledger is still indexing.",
            )
        } else if source.is_none() {
            l10n::text(
                "此服务还没有本地用量账本",
                "This provider has no migrated local usage ledger.",
            )
        } else if !consumer {
            l10n::text(
                "日志中没有与此额度窗口关联的模型",
                "No logged models are linked to this account window.",
            )
        } else {
            l10n::text(
                "此额度窗口的当前周期未知",
                "This quota window has no known current cycle.",
            )
        };
        if reply_complete && consumer {
            for recent in [true, false] {
                let mut sums = [(0, 0, 0); 2];
                for ((s, m, hour, timed), (o, g, c)) in &self.replies {
                    if matches(s, m) && (!recent || *hour) {
                        let sum = &mut sums[usize::from(*timed)];
                        sum.0 += o;
                        sum.1 += g;
                        sum.2 += c;
                    }
                }
                if let Some(rate) = reply_rate(sums) {
                    reply = rate;
                    break;
                }
            }
        }
        if complete && consumer {
            if let (Some((start, _, _)), Some(oldest)) = (cycle, self.buckets.first().map(|b| b.at))
            {
                let start = start.max(oldest);
                let hours = (now - start) as f64 / 3_600_000.0;
                if hours > 0.0 {
                    let mut tokens = 0u64;
                    let mut total = 0u64;
                    let mut amounts = BTreeMap::<String, i128>::new();
                    for b in &self.buckets {
                        if b.at < start || b.at >= now || !matches(&b.source, &b.model) {
                            continue;
                        }
                        let u = b.usage;
                        tokens += u.input + u.write + u.output + u.reasoning;
                        total += u.total();
                        if let Some(price) = &b.cost {
                            *amounts.entry(price.currency.clone()).or_default() +=
                                price.amount_pico;
                        }
                    }
                    token_rate = Some((tokens as f64 / hours).round());
                    consumed = json!({"used":total,"capacity":null,"unit":"tokens"});
                    observed = iso(start);
                    if !amounts.is_empty() {
                        cost = json!({"amounts":amounts.into_iter().map(|(currency,pico)|(currency,pico as f64/1e12)).collect::<BTreeMap<_,_>>(),"hours":hours});
                    }
                }
            }
        }
        if row["nativeUsage"]["used"].as_f64().is_some() {
            consumed = row["nativeUsage"].clone();
        }
        if reply.is_null() {
            unavailable.insert(
                "tokens".into(),
                json!(if !reply_complete {
                    l10n::text(
                        "本地用量账本仍在索引",
                        "The local usage ledger is still indexing.",
                    )
                } else if consumer {
                    l10n::text(
                        "最近七天可计时的回复调用不足 3 次或合计不足 5 秒",
                        "Fewer than 3 reply calls that could be timed, or under 5 seconds of them, were observed in the last seven days.",
                    )
                } else {
                    reason.clone()
                }),
            );
        }
        if consumed.is_null() {
            unavailable.insert("consumed".into(), json!(reason.clone()));
        }
        if cost.is_null() {
            unavailable.insert(
                "cost".into(),
                json!(if complete && consumer && cycle.is_some() {
                    l10n::text(
                        "本周期日志中的模型没有已知的公开价格",
                        "No list price is known for the logged models in this cycle.",
                    )
                } else {
                    reason
                }),
            );
        }
        if pace.is_none() && sampling.is_null() {
            unavailable.insert(
                "burn".into(),
                json!(if cycle.is_none() {
                    l10n::text(
                        "此额度窗口的当前周期未知",
                        "This quota window has no known current cycle.",
                    )
                } else {
                    l10n::text(
                        "本周期还没有额度读数",
                        "No quota observations are available for this cycle.",
                    )
                }),
            );
        }
        json!({"burnRatePctPerHour":rate,"tokensPerHour":token_rate,"replyRate":reply,"consumed":consumed,"cost":cost,"sampling":sampling,"outlook":outlook,"budget":budget,"unavailable":unavailable,"observedFrom":observed})
    }
}

fn model_family(model: &str) -> Option<&str> {
    let model = model.strip_prefix("claude-").unwrap_or(model);
    ["opus", "sonnet", "haiku", "fable", "gemini", "gpt"]
        .into_iter()
        .find(|family| model.split(['-', '_', '.']).any(|part| part == *family))
}

#[cfg(test)]
mod retention_tests {
    use super::*;
    use agent_hud_agents::transcript::metering::MarkKind;

    fn event(key: &str, at: i64) -> Event {
        Event {
            key: key.into(),
            model: "claude-sonnet-4-5".into(),
            at,
            usage: Usage {
                input: 10,
                output: 5,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn count(db: &Connection, table: &str) -> i64 {
        db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
    }

    #[test]
    fn a_forecast_runs_from_the_time_it_is_worked_out_at() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::schema(&db).unwrap();
        let observed = Utc::now().timestamp_millis() - 600_000;
        let view = View::load(&db, &[], observed, &pricing::PriceRegions::default()).unwrap();
        // 10% of the window a hour, 60% left, three hours to its reset.
        let row = json!({"id":"q","usedPercent":40.0,"resetAt":iso(observed + 3 * 3_600_000),
            "windowDurationSeconds":18_000.0,"observedAt":iso(observed)});
        let samples = Samples::from([(
            "q".to_string(),
            vec![(observed - 3_600_000, 70.0), (observed, 60.0)],
        )]);
        // Worked out again long after the reading, the six hours left still
        // run from then, not from the reading.
        for at in [observed + 10_000, observed + 50 * 60_000] {
            let metrics = view.metrics(&row, at, false, false, &samples);
            assert_eq!(
                metrics["outlook"]["exhaustsAt"],
                iso(at + 6 * 3_600_000),
                "{metrics}"
            );
            assert_eq!(metrics["outlook"]["basis"], "recent");
        }
    }

    #[test]
    fn until_readings_give_a_pace_the_cycle_so_far_makes_the_forecast() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::schema(&db).unwrap();
        let now = Utc::now().timestamp_millis();
        let view = View::load(&db, &[], now, &pricing::PriceRegions::default()).unwrap();
        // A five-hour window an hour into its cycle, read once just now.
        let row = |used: f64, reset: i64| {
            json!({"id":"q","usedPercent":used,"resetAt":iso(reset),
                "windowDurationSeconds":18_000.0,"observedAt":iso(now)})
        };
        let samples = Samples::from([("q".to_string(), vec![(now, 75.0)])]);
        let metrics = view.metrics(&row(25.0, now + 4 * 3_600_000), now, false, false, &samples);
        let outlook = &metrics["outlook"];
        // 25% in the hour so far: the other 75% lasts three hours.
        assert_eq!(outlook["kind"], "exhausts", "{metrics}");
        assert_eq!(outlook["basis"], "window");
        assert_eq!(outlook["ratePctPerHour"], 25.0);
        assert_eq!(outlook["exhaustsAt"], iso(now + 3 * 3_600_000));
        assert_eq!(outlook["beforeReset"], true);
        assert!(
            metrics["burnRatePctPerHour"].is_null(),
            "no recent pace yet"
        );
        assert!(metrics["sampling"].is_object(), "still sampling");
        assert!(metrics["unavailable"]["burn"].is_null());
        // A fifth of the cycle gone: 20% is the budget, 25% five points over.
        assert_eq!(
            metrics["budget"],
            json!({"expectedUsedPercent":20.0,"paceDelta":5.0,"status":"ahead"})
        );
        let budget = |used| {
            view.metrics(&row(used, now + 4 * 3_600_000), now, false, false, &samples)["budget"]
                ["status"]
                .clone()
        };
        assert_eq!(budget(21.0), "onTrack");
        assert_eq!(budget(10.0), "behind");
        // Nothing used yet is no usage, from the same basis.
        let idle = view.metrics(&row(0.0, now + 4 * 3_600_000), now, false, false, &samples);
        assert_eq!(
            idle["outlook"],
            json!({"kind":"no-usage","basis":"window","ratePctPerHour":0.0})
        );
        // Under 3% of the cycle gone, too little to go on.
        let early = view.metrics(
            &row(5.0, now + 18_000_000 - 300_000),
            now,
            false,
            false,
            &samples,
        );
        assert_eq!(early["outlook"]["kind"], "insufficient-data");
        assert_eq!(early["budget"]["status"], "ahead");
    }

    #[test]
    fn only_a_forecast_from_recent_readings_alerts() {
        let now = Utc::now().timestamp_millis();
        let quota = |at: i64, outlook: Value| {
            json!({"id":"q","vendor":"v","name":"Current session","usedPercent":25.0,
                "resetAt":iso(now + 4 * 3_600_000),"observedAt":iso(at),"status":"normal",
                "metrics":{"outlook":outlook}})
        };
        for (basis, alerts) in [("window", 0), ("recent", 1)] {
            let mut tracker = crate::island_events::Tracker::new();
            tracker.update(
                &[],
                &[quota(now - 60_000, json!({"kind":"insufficient-data"}))],
                &[],
                &[],
                now,
            );
            let events = tracker.update(
                &[],
                &[quota(
                    now,
                    json!({"kind":"exhausts","basis":basis,"beforeReset":true,
                        "exhaustsAt":iso(now + 3 * 3_600_000)}),
                )],
                &[],
                &[],
                now,
            );
            assert_eq!(events["items"].as_array().unwrap().len(), alerts, "{basis}");
        }
    }

    #[test]
    fn a_logs_list_cost_prices_each_call_on_its_tier() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::schema(&db).unwrap();
        let now = Utc::now().timestamp_millis();
        let call = |key: &str, input: u64, read: u64| Event {
            key: key.into(),
            model: "gpt-6-sol".into(),
            at: now - 60_000,
            usage: Usage {
                input,
                output: 2_000,
                read,
                write: 0,
                reasoning: 500,
            },
            ..Default::default()
        };
        // One call below the 272,001-token tier, two above it.
        let calls = [
            call("a", 10_000, 50_000),
            call("b", 200_000, 100_000),
            call("c", 300_000, 0),
        ];
        write(&db, "/logs/s.jsonl", "codex", &calls).unwrap();
        let one_by_one: i128 = calls
            .iter()
            .map(|event| {
                let tokens = pricing::PriceTokens {
                    tokens_in: event.usage.input + event.usage.write,
                    tokens_out: event.usage.output + event.usage.reasoning,
                    cache_read: event.usage.read,
                    cache_write: event.usage.write,
                    reasoning: event.usage.reasoning,
                };
                pricing::cost_at(
                    "codex",
                    "gpt-6-sol",
                    &tokens,
                    true,
                    pricing::Region::International,
                    None,
                )
                .unwrap()
                .amount_pico
            })
            .sum();
        assert_eq!(
            list_cost(&db, "/logs/s.jsonl").unwrap(),
            Some(ListCost {
                priced: Some(one_by_one),
                unpriced_tokens: 0
            })
        );

        write(
            &db,
            "/logs/s.jsonl",
            "codex",
            &[Event {
                model: "no-such-model".into(),
                ..call("d", 1, 0)
            }],
        )
        .unwrap();
        let partial = list_cost(&db, "/logs/s.jsonl").unwrap().unwrap();
        assert_eq!(
            partial.priced,
            Some(one_by_one),
            "one unpriced call leaves the other calls priced"
        );
        assert_eq!(partial.unpriced_tokens, call("d", 1, 0).usage.total());
        assert_eq!(partial.complete(), None, "and the whole cost unknown");
        assert_eq!(
            list_cost(&db, "/logs/none.jsonl").unwrap(),
            None,
            "a log without calls has no cost"
        );
    }

    #[test]
    fn usage_older_than_the_retention_is_not_written_and_expires() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::schema(&db).unwrap();
        let now = Utc::now().timestamp_millis();
        let old = now - 40 * 86_400_000;
        write(
            &db,
            "/logs/a.jsonl",
            "claude",
            &[event("old", old), event("new", now - 60_000)],
        )
        .unwrap();
        assert_eq!(
            count(&db, "usage_meter_event"),
            1,
            "an event past the retention is not written"
        );

        // A row written while it was still inside the window leaves at expiry.
        db.execute(
            "INSERT INTO usage_meter_event(contribution,event_key,source,model,at_ms,input,output,cache_read,cache_write,reasoning) VALUES('/logs/a.jsonl','kept','claude','m',?1,1,1,0,0,0)",
            params![old],
        )
        .unwrap();
        db.execute(
            "INSERT INTO usage_meter_bucket(contribution,source,model,at_ms,input,output,cache_read,cache_write,reasoning) VALUES('/logs/a.jsonl','claude','m',?1,1,1,0,0,0)",
            params![old.div_euclid(900_000) * 900_000],
        )
        .unwrap();
        assert_eq!(expire(&db, now).unwrap(), 2);
        assert_eq!(count(&db, "usage_meter_event"), 1);
        assert_eq!(
            count(&db, "usage_meter_bucket"),
            1,
            "the recent event's bucket stays"
        );
        assert_eq!(expire(&db, now).unwrap(), 0);
    }

    #[test]
    fn marks_are_kept_once_and_leave_with_their_log_or_their_time() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::schema(&db).unwrap();
        let now = Utc::now().timestamp_millis();
        let written = [
            Mark {
                at: now - 3_000,
                kind: MarkKind::Prompt,
            },
            Mark {
                at: now - 2_000,
                kind: MarkKind::Compaction,
            },
            Mark {
                at: now - 40 * 86_400_000,
                kind: MarkKind::Prompt,
            },
        ];
        assert!(write_marks(&db, "/logs/a.jsonl", &written).unwrap());
        assert!(
            !write_marks(&db, "/logs/a.jsonl", &written).unwrap(),
            "a line read again adds nothing"
        );
        assert_eq!(
            count(&db, "usage_meter_mark"),
            2,
            "past the retention nothing is written"
        );
        assert_eq!(
            marks(&db, &["/logs/a.jsonl".into()]).unwrap(),
            (vec![now - 3_000], vec![now - 2_000])
        );
        remove_between(&db, "/logs/a.jsonl", now - 2_500, now).unwrap();
        assert_eq!(count(&db, "usage_meter_mark"), 1);
        move_contribution(&db, "/logs/a.jsonl", "/logs/b.jsonl", now).unwrap();
        assert_eq!(
            marks(&db, &["/logs/b.jsonl".into()]).unwrap().0,
            vec![now - 3_000]
        );
        reset(&db, "/logs/b.jsonl").unwrap();
        assert_eq!(count(&db, "usage_meter_mark"), 0);
        write_marks(&db, "/logs/c.jsonl", &written[..1]).unwrap();
        expire(&db, now + 32 * 86_400_000).unwrap();
        assert_eq!(count(&db, "usage_meter_mark"), 0);
    }

    #[test]
    fn calls_keep_their_tools_and_window_and_each_model_its_largest_prompt() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::schema(&db).unwrap();
        let now = Utc::now().timestamp_millis();
        let mut call = Event {
            tools: vec!["Bash".into()],
            context_window: Some(258_400),
            ..event("a", now - 1_000)
        };
        call.usage.read = 90;
        write(&db, "/logs/s.jsonl", "claude", &[call.clone()]).unwrap();
        call.tools.push("Read".into());
        assert!(
            write(&db, "/logs/s.jsonl", "claude", &[call.clone()]).unwrap(),
            "new tools rewrite the call"
        );
        assert!(!write(&db, "/logs/s.jsonl", "claude", &[call]).unwrap());
        let rows = calls_of(&db, &["/logs/s.jsonl".into()], None, None).unwrap();
        assert_eq!(rows[0].tools, vec!["Bash".to_owned(), "Read".to_owned()]);
        assert_eq!(rows[0].context_window, Some(258_400));
        write(
            &db,
            "/logs/t.jsonl",
            "claude",
            &[Event {
                usage: Usage {
                    input: 5,
                    ..Default::default()
                },
                ..event("b", now - 500)
            }],
        )
        .unwrap();
        assert_eq!(
            largest_context(&db, "claude", "claude-sonnet-4-5").unwrap(),
            Some(100)
        );
        assert_eq!(
            prompt_before(&db, &["/logs/s.jsonl".into()], now).unwrap(),
            Some(100)
        );
    }

    #[test]
    fn a_deepseek_log_costs_its_calls_at_the_accounts_prices() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::schema(&db).unwrap();
        let now = Utc::now().timestamp_millis();
        let call = Event {
            model: "deepseek-v4-pro#deepseek-official".into(),
            usage: Usage {
                input: 1_000_000,
                output: 100_000,
                ..Default::default()
            },
            ..event("a", now - 1_000)
        };
        write(
            &db,
            "/logs/d.jsonl",
            "deepseek",
            std::slice::from_ref(&call),
        )
        .unwrap();
        let [usd, cny] = account_cost(&db, "/logs/d.jsonl", "deepseek")
            .unwrap()
            .unwrap();
        let tokens = price_tokens(call.usage);
        let at = |region| {
            pricing::cost_at(
                "deepseek",
                &call.model,
                &tokens,
                true,
                region,
                Some(call.at),
            )
            .map(|cost| cost.amount_pico)
        };
        assert_eq!(usd, at(pricing::Region::International));
        assert_eq!(cny, at(pricing::Region::China));
        assert!(cny.is_some());
        assert_eq!(
            account_cost(&db, "/logs/d.jsonl", "claude").unwrap(),
            None,
            "only DeepSeek bills an account"
        );
        write(
            &db,
            "/logs/d.jsonl",
            "deepseek",
            &[Event {
                model: "deepseek-v4-pro#openrouter".into(),
                ..event("b", now - 500)
            }],
        )
        .unwrap();
        assert_eq!(
            account_cost(&db, "/logs/d.jsonl", "deepseek").unwrap(),
            Some([None, None]),
            "a call routed elsewhere has no account price"
        );
    }

    #[test]
    fn re_cached_prompts_count_in_logs_that_mark_their_prompts() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::schema(&db).unwrap();
        let now = Utc::now().timestamp_millis();
        let call = |key: &str, at: i64, input: u64, write: u64, read: u64| Event {
            usage: Usage {
                input,
                write,
                read,
                output: 10,
                reasoning: 0,
            },
            ..event(key, at)
        };
        let calls = [
            call("a", now - 3_000, 10, 1_000, 0),
            call("b", now - 2_000, 10, 10, 1_000),
            call("c", now - 1_000, 10, 1_100, 0),
        ];
        write(&db, "/logs/s.jsonl", "claude", &calls).unwrap();
        write(&db, "/logs/u.jsonl", "claude", &calls).unwrap();
        write_marks(
            &db,
            "/logs/s.jsonl",
            &[Mark {
                at: now - 4_000,
                kind: MarkKind::Prompt,
            }],
        )
        .unwrap();
        let rows = recached(
            &db,
            &["/logs/s.jsonl".into(), "/logs/u.jsonl".into()],
            now - 86_400_000,
        )
        .unwrap();
        assert_eq!(
            rows.len(),
            1,
            "the log without prompt marks does not count: {rows:?}"
        );
        assert_eq!(
            (rows[0].3, rows[0].4),
            (1_110, 1_110),
            "sent again, as cache writes"
        );
    }

    /// The queries `View` replaced: replies of the hour and the week at
    /// `now`, and every bucket summed by source, model and start.
    fn queried(
        db: &Connection,
        paths: &[String],
        now: i64,
    ) -> (
        HashMap<(String, String, bool, bool), (u64, u64, u64)>,
        Vec<(String, String, i64, Usage)>,
    ) {
        db.execute("DELETE FROM meter_selected", []).unwrap();
        for path in paths {
            db.execute(
                "INSERT INTO meter_selected(contribution) VALUES(?1)",
                [path],
            )
            .unwrap();
        }
        let mut replies = HashMap::new();
        // A call its log timed counts its own time (100 ms to 10 min), one
        // it did not the gap since the log's previous call (100 ms to 120 s);
        // at least 10 output tokens, at most 10,000 a second.
        let mut s = db
            .prepare(
                "SELECT source,model,at_ms-span>=?1,timed,sum(tokens),sum(span),count(*) FROM (
                   SELECT source,model,at_ms,output+reasoning AS tokens,
                          coalesce(duration_ms,gap_ms) AS span,duration_ms IS NOT NULL AS timed
                   FROM meter_selected CROSS JOIN usage_meter_event USING(contribution)
                   WHERE at_ms>=?2 AND at_ms<=?3
                     AND (duration_ms BETWEEN 100 AND 600000
                          OR (duration_ms IS NULL AND gap_ms BETWEEN 100 AND 120000)))
                 WHERE at_ms-span>=?2 AND tokens>=10 AND tokens<=10*span
                 GROUP BY source,model,at_ms-span>=?1,timed",
            )
            .unwrap();
        let mut rows = s
            .query(params![now - 3_600_000, now - 7 * 86_400_000, now])
            .unwrap();
        while let Some(r) = rows.next().unwrap() {
            replies.insert(
                (
                    r.get(0).unwrap(),
                    r.get(1).unwrap(),
                    r.get(2).unwrap(),
                    r.get(3).unwrap(),
                ),
                (
                    unsigned(r, 4).unwrap(),
                    unsigned(r, 5).unwrap(),
                    unsigned(r, 6).unwrap(),
                ),
            );
        }
        let mut s = db.prepare(
            "SELECT source,model,at_ms,sum(input),sum(output),sum(cache_read),sum(cache_write),sum(reasoning)
             FROM usage_meter_bucket JOIN meter_selected USING(contribution)
             GROUP BY source,model,at_ms
             HAVING sum(input+output+cache_read+cache_write+reasoning)>0
             ORDER BY at_ms,source,model",
        ).unwrap();
        let buckets = s
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, usage(r, 3)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        (replies, buckets)
    }

    fn kept(
        view: &View,
    ) -> (
        HashMap<(String, String, bool, bool), (u64, u64, u64)>,
        Vec<(String, String, i64, Usage)>,
    ) {
        let mut buckets = view
            .buckets
            .iter()
            .map(|b| (b.source.clone(), b.model.clone(), b.at, b.usage))
            .collect::<Vec<_>>();
        buckets.sort_by(|a, b| (a.2, &a.0, &a.1).cmp(&(b.2, &b.0, &b.1)));
        (view.replies.clone(), buckets)
    }

    #[test]
    fn the_sources_through_a_route_are_the_ones_whose_calls_name_it() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::schema(&db).unwrap();
        let now = Utc::now().timestamp_millis();
        let call = |key: &str, model: &str, at: i64| Event {
            key: key.into(),
            model: model.into(),
            at,
            usage: Usage {
                input: 10,
                ..Default::default()
            },
            ..Default::default()
        };
        write(
            &db,
            "/logs/a.jsonl",
            "claude",
            &[
                call("a1", "claude-sonnet-4-5#pool.example.net", now - 3_600_000),
                call(
                    "a2",
                    "claude-sonnet-4-5#old.example.net",
                    now - 40 * 86_400_000,
                ),
            ],
        )
        .unwrap();
        write(
            &db,
            "/logs/b.jsonl",
            "pi",
            &[call("b1", "gpt-6#pool.example.net", now - 600_000)],
        )
        .unwrap();
        let paths = vec!["/logs/a.jsonl".to_string(), "/logs/b.jsonl".to_string()];
        let view = View::load(&db, &paths, now, &pricing::PriceRegions::default()).unwrap();
        let routes = BTreeSet::from(["pool.example.net", "old.example.net", "quiet.example.net"]);
        let found = view.sources_by_route(now - 30 * 86_400_000, &routes);
        assert_eq!(
            found,
            BTreeMap::from([(
                "pool.example.net".to_string(),
                BTreeSet::from(["claude".to_string(), "pi".to_string()])
            )])
        );
    }

    #[test]
    fn a_plans_spend_leaves_out_calls_through_a_route_and_other_sources() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::schema(&db).unwrap();
        let now = Utc::now().timestamp_millis();
        let used = Usage {
            input: 1_000,
            output: 100,
            ..Default::default()
        };
        let call = |key: &str, model: &str, at: i64| Event {
            key: key.into(),
            model: model.into(),
            at,
            usage: used,
            ..Default::default()
        };
        write(
            &db,
            "logs/a.jsonl",
            "claude",
            &[
                call(
                    "a1",
                    "claude-sonnet-4-5",
                    now - 30 * 86_400_000 - 21_600_000,
                ),
                call("a2", "claude-sonnet-4-5", now - 3_600_000),
                call("a3", "claude-sonnet-4-5#relay.example.com", now - 1_800_000),
                call("a4", "qwen3-coder:30b", now - 1_800_000),
            ],
        )
        .unwrap();
        write(
            &db,
            "logs/b.jsonl",
            "codex",
            &[call("b1", "gpt-6-sol", now - 600_000)],
        )
        .unwrap();
        let paths = vec!["logs/a.jsonl".to_string(), "logs/b.jsonl".to_string()];
        let view = View::load(&db, &paths, now, &pricing::PriceRegions::default()).unwrap();
        let (spent, oldest) = view.spent(&["claude"], now - 30 * 86_400_000, now);
        assert_eq!(
            (spent.tokens, spent.unpriced_tokens),
            (2 * used.total(), used.total()),
            "the old call, the relayed one and Codex's are left out"
        );
        let price = pricing::cost_at(
            "claude",
            "claude-sonnet-4-5",
            &price_tokens(used),
            false,
            pricing::Region::International,
            Some(now),
        )
        .unwrap()
        .amount_pico;
        assert_eq!(spent.costs, BTreeMap::from([("USD".to_string(), price)]));
        assert!(
            oldest.is_some_and(|at| at <= now - 30 * 86_400_000 - 21_600_000),
            "the buckets begin with the older call"
        );
    }

    #[test]
    fn recent_usage_sums_each_clients_last_day_with_its_unpriced_part() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::schema(&db).unwrap();
        let now = Utc::now().timestamp_millis();
        let used = Usage {
            input: 1_000,
            output: 100,
            ..Default::default()
        };
        let call = |key: &str, model: &str, at: i64| Event {
            key: key.into(),
            model: model.into(),
            at,
            usage: used,
            ..Default::default()
        };
        write(
            &db,
            "/logs/a.jsonl",
            "claude",
            &[
                call("a1", "claude-sonnet-4-5", now - 2 * 86_400_000),
                call("a2", "claude-sonnet-4-5", now - 3_600_000),
                call("a3", "qwen3-coder:30b", now - 1_800_000),
            ],
        )
        .unwrap();
        write(
            &db,
            "/logs/b.jsonl",
            "codex",
            &[call("b1", "gpt-6-sol", now - 600_000)],
        )
        .unwrap();
        write(
            &db,
            "/logs/c.jsonl",
            "codex",
            &[call("c1", "gpt-6-sol", now - 600_000)],
        )
        .unwrap();
        let paths = vec!["/logs/a.jsonl".to_string(), "/logs/b.jsonl".to_string()];
        let view = View::load(&db, &paths, now, &pricing::PriceRegions::default()).unwrap();
        let recent = view.recent(now);
        let price = |source: &str, model: &str| {
            pricing::cost_at(
                source,
                model,
                &price_tokens(used),
                false,
                pricing::Region::International,
                Some(now),
            )
            .unwrap()
            .amount_pico
        };
        let claude = &recent["claude"];
        assert_eq!(
            claude.tokens,
            2 * used.total(),
            "the day-old call is left out"
        );
        assert_eq!(
            claude.unpriced_tokens,
            used.total(),
            "the local model has no price"
        );
        assert_eq!(
            claude.costs,
            BTreeMap::from([("USD".to_string(), price("claude", "claude-sonnet-4-5"))])
        );
        let codex = &recent["codex"];
        assert_eq!(codex.tokens, used.total(), "the unselected log is left out");
        assert_eq!(codex.unpriced_tokens, 0);
        assert_eq!(codex.costs["USD"], price("codex", "gpt-6-sol"));
        assert_eq!(codex.json()["unpricedTokens"], 0);
        assert!(
            view.recent(now + 3 * 86_400_000).is_empty(),
            "a day later nothing is recent"
        );
    }

    #[test]
    fn the_meter_view_matches_its_queries_and_reads_a_changed_log_alone() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::schema(&db).unwrap();
        let now = Utc::now().timestamp_millis();
        let call = |key: &str, model: &str, at: i64, output: u64| Event {
            key: key.into(),
            model: model.into(),
            at,
            usage: Usage {
                input: 100,
                output,
                reasoning: 7,
                ..Default::default()
            },
            ..Default::default()
        };
        let timed = |key: &str, model: &str, at: i64, output: u64, took: u64| Event {
            duration_ms: Some(took),
            ..call(key, model, at, output)
        };
        // Spans inside and outside the hour, one longer than 120 s, one a
        // week back, one stamped ahead of the clock, and a log not selected;
        // calls their logs timed, one over 10 minutes, one too fast to be
        // a reply.
        write(
            &db,
            "/logs/a.jsonl",
            "claude",
            &[
                call("a1", "claude-sonnet-4-5", now - 8 * 86_400_000, 10),
                call("a2", "claude-sonnet-4-5", now - 8 * 86_400_000 + 30_000, 20),
                call("a3", "claude-sonnet-4-5", now - 2 * 3_600_000, 30),
                call("a4", "claude-sonnet-4-5", now - 2 * 3_600_000 + 60_000, 40),
                call("a5", "claude-sonnet-4-5", now - 600_000, 50),
                call("a6", "claude-sonnet-4-5", now - 590_000, 60),
                call("a7", "claude-opus-4-5", now - 400_000, 70),
                timed("a8", "claude-sonnet-4-5", now - 300_000, 80, 4_000),
                timed("a9", "claude-sonnet-4-5", now - 200_000, 90, 700_000),
                timed(
                    "a10",
                    "claude-sonnet-4-5",
                    now - 2 * 3_600_000 + 90_000,
                    900,
                    50,
                ),
            ],
        )
        .unwrap();
        write(
            &db,
            "/logs/b.jsonl",
            "codex",
            &[
                call("b1", "gpt-6-sol", now - 3 * 86_400_000, 80),
                call("b2", "gpt-6-sol", now - 3 * 86_400_000 + 200_000, 90),
                call("b3", "gpt-6-sol", now - 1_000_000, 100),
                call("b4", "gpt-6-sol", now - 990_000, 110),
                timed("b4a", "gpt-6-sol", now - 900_000, 115, 3_000),
                call("b5", "gpt-6-sol", now + 300_000, 120),
            ],
        )
        .unwrap();
        write(
            &db,
            "/logs/c.jsonl",
            "codex",
            &[
                call("c1", "gpt-6-sol", now - 500_000, 130),
                call("c2", "gpt-6-sol", now - 490_000, 140),
            ],
        )
        .unwrap();
        let paths = vec!["/logs/a.jsonl".to_string(), "/logs/b.jsonl".to_string()];
        let regions = pricing::PriceRegions::default();
        let mut view = View::load(&db, &paths, now, &regions).unwrap();
        assert_eq!(kept(&view), queried(&db, &paths, now));
        assert_eq!(
            view.replies
                .get(&("claude".into(), "claude-sonnet-4-5".into(), true, true)),
            Some(&(87, 4_000, 1)),
            "of the timed calls only a8 counts"
        );
        assert!(
            view.valid_until.is_some_and(|until| until <= now + 300_000),
            "the call ahead of the clock bounds the view"
        );

        // b gains a call and moves first, as the most active: the view reads b again alone.
        write(
            &db,
            "/logs/b.jsonl",
            "codex",
            &[call("b6", "gpt-6-sol", now + 20_000, 150)],
        )
        .unwrap();
        let later = now + 30_000;
        let reordered = vec!["/logs/b.jsonl".to_string(), "/logs/a.jsonl".to_string()];
        view.update(
            &db,
            &reordered,
            &HashSet::from(["/logs/b.jsonl".to_string()]),
            later,
            &regions,
        )
        .unwrap();
        assert_eq!(view.selection, paths, "a new order is not a new selection");
        assert_eq!(kept(&view), queried(&db, &paths, later));

        // The clock alone moves spans out of the hour, and the call ahead comes due.
        for then in [now + 400_000, now + 2 * 3_600_000, now + 6 * 86_400_000] {
            view.refresh_clock(then);
            assert_eq!(
                kept(&view).0,
                queried(&db, &paths, then).0,
                "at +{} ms",
                then - now
            );
        }

        // Another selection reads every log.
        let all = vec![
            "/logs/a.jsonl".to_string(),
            "/logs/b.jsonl".to_string(),
            "/logs/c.jsonl".to_string(),
        ];
        let then = now + 6 * 86_400_000;
        view.update(&db, &all, &HashSet::new(), then, &regions)
            .unwrap();
        assert_eq!(kept(&view), queried(&db, &all, then));
    }

    #[test]
    fn a_call_timed_by_a_later_line_changes_its_duration_alone() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::schema(&db).unwrap();
        let now = Utc::now().timestamp_millis();
        let call = |key: &str, at: i64, took: Option<u64>| Event {
            key: key.into(),
            model: "claude-opus-4-5".into(),
            at,
            usage: Usage {
                input: 10,
                output: 200,
                ..Default::default()
            },
            duration_ms: took,
            ..Default::default()
        };
        assert!(write(
            &db,
            "/l",
            "claude",
            &[
                call("a", now - 60_000, None),
                call("b", now - 30_000, Some(2_000))
            ]
        )
        .unwrap());
        let stored = |db: &Connection| {
            db.query_row(
                "SELECT duration_ms,gap_ms,(SELECT group_concat(updated_at_ms||':'||output) FROM usage_meter_bucket) FROM usage_meter_event WHERE event_key='b'",
                [],
                |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, String>(2)?)),
            )
            .unwrap()
        };
        let before = stored(&db);
        assert_eq!((before.0, before.1), (Some(2_000), Some(30_000)));
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert!(
            write(&db, "/l", "claude", &[call("b", now - 30_000, Some(5_000))]).unwrap(),
            "a new duration is a change the meter view reads"
        );
        let after = stored(&db);
        assert_eq!((after.0, after.1), (Some(5_000), Some(30_000)));
        assert_eq!(after.2, before.2, "no bucket moves");
        assert!(!write(&db, "/l", "claude", &[call("b", now - 30_000, Some(5_000))]).unwrap());
    }

    #[test]
    fn the_reply_rate_reads_timed_calls_first_and_gaps_as_a_floor() {
        assert_eq!(
            reply_span(200, Some(5_000), Some(90_000)),
            Some((5_000, true))
        );
        assert_eq!(reply_span(200, None, Some(90_000)), Some((90_000, false)));
        assert_eq!(
            reply_span(200, Some(700_000), Some(1_000)),
            None,
            "a stall, gap or not"
        );
        assert_eq!(reply_span(200, None, Some(150_000)), None, "an idle gap");
        assert_eq!(reply_span(9, Some(5_000), None), None, "too little output");
        assert_eq!(reply_span(2_000, Some(150), None), None, "a burst");
        assert_eq!(reply_span(200, Some(50), None), None);
        let timed = reply_rate([(400, 20_000, 3), (600, 6_000, 3)]).unwrap();
        assert_eq!(
            (
                timed["tokensPerSecond"].as_f64(),
                timed["lowerBound"].as_bool()
            ),
            (Some(100.0), Some(false)),
            "gap-timed calls do not dilute timed ones"
        );
        let floor = reply_rate([(400, 20_000, 3), (0, 0, 0)]).unwrap();
        assert_eq!(
            (
                floor["tokensPerSecond"].as_f64(),
                floor["lowerBound"].as_bool()
            ),
            (Some(20.0), Some(true))
        );
        assert!(reply_rate([(0, 0, 0); 2]).is_none());
    }

    #[test]
    fn a_reply_rate_needs_three_calls_and_five_seconds() {
        assert!(
            reply_rate([(0, 0, 0), (300, 3_000, 1)]).is_none(),
            "one call"
        );
        assert!(
            reply_rate([(0, 0, 0), (300, 4_999, 3)]).is_none(),
            "three calls under five seconds"
        );
        assert!(reply_rate([(0, 0, 0), (300, 5_000, 3)]).is_some());
        // Too few timed calls leave the gap-timed ones, when there are
        // enough of those.
        let floor = reply_rate([(900, 30_000, 4), (300, 3_000, 2)]).unwrap();
        assert_eq!(
            (floor["calls"].as_u64(), floor["lowerBound"].as_bool()),
            (Some(4), Some(true))
        );
        assert!(reply_rate([(900, 30_000, 2), (300, 3_000, 2)]).is_none());
    }
}
