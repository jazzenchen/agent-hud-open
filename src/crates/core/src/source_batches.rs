//! Multi-session JSONL acquisition: only append bytes are decoded. Compact source
//! facts and trace/model contexts are staged in the existing ingestion database;
//! projections are paged into the same canonical per-event ledger.
use super::*;
use agent_hud_agents::transcript::records::{Page, Record};
use std::io::{Seek, SeekFrom};

mod copilot;
mod grok;

use copilot::copilot;
use grok::grok;
pub(super) use grok::grok_models;

pub(super) fn accepts(path: &Path, source: &str) -> bool {
    match source {
        "grok-cli" => path.file_name().is_some_and(|s| s == "unified.jsonl"),
        "copilot" => {
            path.extension().is_some_and(|s| s == "jsonl")
                && path.file_name().is_none_or(|s| s != "events.jsonl")
        }
        _ => false,
    }
}
fn text(v: &Value) -> Option<&str> {
    v.as_str().map(str::trim).filter(|s| !s.is_empty())
}
fn identifier(v: &Value) -> Option<&str> {
    text(v).filter(|s| !s.chars().all(|c| c == '0'))
}
fn number(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.parse().ok())
}
fn stamp(v: &Value) -> Option<i64> {
    text(v)
        .and_then(metering::date_ms)
        .or_else(|| v.as_i64().filter(|n| *n > 0))
}
fn otel_time(v: &Value) -> Option<i64> {
    let a = v.as_array()?;
    if a.len() != 2 {
        return None;
    }
    let s = a[0].as_f64()?;
    let n = a[1].as_f64()?;
    (s > 0. && (0.0..1_000_000_000.0).contains(&n)).then_some((s * 1000. + n / 1_000_000.) as i64)
}
pub(super) fn resume(db: &Connection, path: &Path, g: &str) -> rusqlite::Result<Value> {
    let saved = staging::get_log_context(
        db,
        &path.to_string_lossy(),
        g,
        "resume",
        "",
        u64::MAX / 2,
        false,
    )?;
    let at = saved
        .as_ref()
        .and_then(|(v, _)| v.as_deref()?.parse::<u64>().ok())
        .unwrap_or(0);
    let emitted = saved
        .and_then(|(_, aux)| aux?.parse::<i64>().ok())
        .unwrap_or(-1);
    Ok(json!({"stage":0,"after":at,"emitted":emitted}))
}
pub(super) fn read(
    path: &Path,
    db: &Connection,
    g: &str,
    cursor: &Value,
    source: &str,
) -> rusqlite::Result<Page> {
    let path_text = path.to_string_lossy();
    let stage = cursor["stage"].as_i64().unwrap_or(0);
    let after = cursor["after"].as_u64().unwrap_or(0);
    let emitted = cursor["emitted"].as_i64().unwrap_or(-1);
    if cursor.is_null() {
        staging::clear_stale_log_staging(db, &path_text, g)?;
    }
    let (after, byte_offset) = if stage == 0 {
        let file = fs::File::open(path).map_err(|_| rusqlite::Error::InvalidQuery)?;
        let mut reader = std::io::BufReader::new(file);
        reader
            .seek(SeekFrom::Start(after))
            .map_err(|_| rusqlite::Error::InvalidQuery)?;
        let mut offset = after;
        let mut bytes = 0;
        let mut eof = false;
        while bytes < 2 * 1024 * 1024 {
            let (line, n, complete) =
                super::line(&mut reader).map_err(|_| rusqlite::Error::InvalidQuery)?;
            if !complete {
                eof = true;
                break;
            }
            let at = offset;
            offset += n;
            bytes += n as usize;
            if line.len() > MAX_LINE {
                continue;
            }
            let Ok(v) = serde_json::from_slice::<Value>(&line) else {
                continue;
            };
            if source == "grok-cli" {
                grok(db, &path_text, g, at, &v)?
            } else {
                copilot(db, &path_text, g, at, &v)?
            }
        }
        staging::put_log_context(
            db,
            &path_text,
            g,
            "resume",
            "",
            0,
            Some(&offset.to_string()),
            Some(&emitted.to_string()),
        )?;
        if !eof {
            return Ok(Page {
                records: vec![],
                cursor: Some(json!({"stage":0,"after":offset,"emitted":emitted})),
                watermark: None,
            });
        }
        (emitted, offset)
    } else {
        (
            cursor["after"].as_i64().unwrap_or(emitted),
            cursor["byte_offset"].as_u64().unwrap_or(0),
        )
    };
    let staged_rows = staging::query_log_records_page(db, &path_text, g, after)?;
    let mut records = vec![];
    let mut last = after;
    let n = staged_rows.len();
    for r in staged_rows {
        last = r.offset;
        let mut id = r.id;
        let mut key = r.event_key;
        let mut model = r.model;
        let context = r.context;
        if source == "copilot" {
            if id.is_empty() {
                id = if let Some(trace) = context.as_deref() {
                    staging::get_log_context(
                        db,
                        &path_text,
                        g,
                        "trace-session",
                        trace,
                        u64::MAX / 2,
                        true,
                    )?
                    .and_then(|(v, _)| v)
                    .unwrap_or_default()
                } else {
                    String::new()
                }
            }
            if id.is_empty() {
                continue;
            }
            id = format!("copilot:{id}");
            key = format!("{id}:otel:{key}");
            if model.is_none() {
                if let Some(trace) = context.as_deref() {
                    model = staging::get_log_context(
                        db,
                        &path_text,
                        g,
                        "trace-model",
                        trace,
                        u64::MAX / 2,
                        true,
                    )?
                    .and_then(|(v, _)| v)
                }
            }
        } else if model.is_none() {
            if let Some(process) = context.as_deref() {
                if staging::count_log_sessions_for_process(db, &path_text, g, process)? == 1 {
                    model = r.candidate;
                }
            }
        }
        records.push(Record {
            id,
            title: None,
            title_fallback: None,
            cwd: None,
            model: model.unwrap_or_else(|| "Unknown".into()),
            key,
            at: r.at_ms,
            usage: Usage {
                input: r.input.max(0) as u64,
                output: r.output.max(0) as u64,
                read: r.cache_read.max(0) as u64,
                ..Usage::default()
            },
            parent: None,
            archived: Some(false),
            updated: None,
            duration_ms: r.duration_ms.and_then(|ms| u64::try_from(ms).ok()),
        });
    }
    let next_emitted = last.max(emitted);
    if n < 500 {
        staging::put_log_context(
            db,
            &path_text,
            g,
            "resume",
            "",
            0,
            Some(&byte_offset.to_string()),
            Some(&next_emitted.to_string()),
        )?;
    }
    Ok(Page {
        records,
        cursor: (n == 500).then(
            || json!({"stage":1,"after":last,"byte_offset":byte_offset,"emitted":next_emitted}),
        ),
        watermark: (n < 500).then(|| json!({"stage":0,"after":byte_offset,"emitted":next_emitted})),
    })
}
