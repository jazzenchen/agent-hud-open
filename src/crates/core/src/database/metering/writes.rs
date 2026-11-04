//! Writing a log's calls and marks into the ledger.

use super::{integer, retention_cutoff, tools_from, tools_text, usage, Event, Mark};
use crate::Usage;
use chrono::Utc;
use rusqlite::{params, Connection};

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
