//! Multi-session JSONL acquisition: only append bytes are decoded. Compact source
//! facts and trace/model contexts are staged in the existing ingestion database;
//! projections are paged into the same canonical per-event ledger.
use super::*;
use agent_hud_agents::transcript::records::{Page, Record};
use std::io::{Seek, SeekFrom};
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
pub(super) fn grok_models(
    db: &Connection,
    session_id: &str,
) -> rusqlite::Result<HashMap<Option<i64>, String>> {
    let rows = staging::query_log_records_by_session(db, session_id)?;
    let mut by_time: HashMap<Option<i64>, Option<String>> = HashMap::new();
    for (path, g, at, mut model, context, candidate) in rows {
        if model.is_none() {
            if let Some(process) = context.as_deref() {
                if staging::count_log_sessions_for_process(db, &path, &g, process)? == 1 {
                    model = candidate;
                }
            }
        }
        let Some(model) = model.filter(|m| !m.is_empty() && m != "Unknown") else {
            continue;
        };
        for bucket in [Some(at), None] {
            by_time
                .entry(bucket)
                .and_modify(|prev| {
                    if prev.as_deref() != Some(model.as_str()) {
                        *prev = None;
                    }
                })
                .or_insert_with(|| Some(model.clone()));
        }
    }
    Ok(by_time
        .into_iter()
        .filter_map(|(k, v)| Some((k, v?)))
        .collect())
}
fn grok(db: &Connection, path: &str, g: &str, offset: u64, v: &Value) -> rusqlite::Result<()> {
    let msg = text(&v["msg"]).unwrap_or("");
    let pid = number(&v["pid"]);
    if msg == "AuthManager::new" {
        if let Some(pid) = pid {
            staging::put_log_context(
                db,
                path,
                g,
                "process",
                &pid.to_string(),
                offset,
                Some(&offset.to_string()),
                None,
            )?;
        }
        return Ok(());
    }
    let process = if let Some(pid) = pid {
        let generation =
            staging::get_log_context(db, path, g, "process", &pid.to_string(), offset, false)?
                .and_then(|(v, _)| v)
                .unwrap_or_else(|| "0".into());
        Some(format!("{pid}:{generation}"))
    } else {
        None
    };
    let ctx = &v["ctx"];
    let changed = match msg {
        "model changed" => text(&ctx["model"]),
        "model catalog: notifying clients" => text(&ctx["current_model_id"]),
        "backend_search: model switch" => text(&ctx["new_model"]),
        _ => None,
    };
    let sid = text(&v["sid"]);
    if let Some(model) = changed {
        let key = if let Some(sid) = sid {
            format!("{sid}:{}", process.as_deref().unwrap_or(""))
        } else {
            format!("process:{}", process.as_deref().unwrap_or(""))
        };
        staging::put_log_context(db, path, g, "model", &key, offset, Some(model), None)?;
        return Ok(());
    }
    if msg != "shell.turn.inference_done" {
        return Ok(());
    }
    let (Some(sid), Some(at), Some(input), Some(output)) = (
        sid,
        stamp(&v["ts"]),
        number(&ctx["prompt_tokens"]),
        number(&ctx["completion_tokens"]),
    ) else {
        return Ok(());
    };
    let read = number(&ctx["cached_prompt_tokens"]).unwrap_or(0);
    if read > input {
        return Ok(());
    }
    let key = text(&v["event_id"])
        .or_else(|| text(&v["eventId"]))
        .or_else(|| text(&ctx["event_id"]))
        .map(str::to_owned)
        .unwrap_or_else(|| metering::event_key(&serde_json::to_string(v).unwrap_or_default()));
    let scope = format!("{sid}:{}", process.as_deref().unwrap_or(""));
    let model =
        staging::get_log_context(db, path, g, "model", &scope, offset, false)?.and_then(|(v, _)| v);
    let fallback = if model.is_none() {
        if let Some(process) = process.as_deref() {
            staging::get_log_context(
                db,
                path,
                g,
                "model",
                &format!("process:{process}"),
                offset,
                false,
            )?
            .and_then(|(v, _)| v)
        } else {
            None
        }
    } else {
        None
    };
    if let Some(process) = process.as_deref() {
        staging::put_log_context(
            db,
            path,
            g,
            "session",
            &format!("{process}\0{sid}"),
            0,
            Some(sid),
            Some(process),
        )?;
    }
    staging::insert_log_record(
        db,
        path,
        g,
        offset,
        &format!("grok:{sid}"),
        &format!("grok:{sid}:unified:{key}"),
        at,
        (input - read) as i64,
        output as i64,
        read as i64,
        model.as_deref(),
        process.as_deref(),
        fallback.as_deref(),
        // The model call's own time; the record's elapsed_since_turn_start_ms
        // also holds the turn's tool runs.
        number(&ctx["model_elapsed_ms"]).and_then(|ms| i64::try_from(ms).ok()),
    )?;
    Ok(())
}
fn copilot(db: &Connection, path: &str, g: &str, offset: u64, v: &Value) -> rusqlite::Result<()> {
    let a = &v["attributes"];
    if !a.is_object() {
        return Ok(());
    }
    let trace = identifier(&v["traceId"]).or_else(|| identifier(&v["spanContext"]["traceId"]));
    let session = text(&a["gen_ai.conversation.id"]);
    let model = text(&a["gen_ai.response.model"]).or_else(|| text(&a["gen_ai.request.model"]));
    if let Some(trace) = trace {
        if let Some(session) = session {
            staging::put_log_context(
                db,
                path,
                g,
                "trace-session",
                trace,
                offset,
                Some(session),
                None,
            )?;
            staging::update_log_record_trace_session(db, path, g, trace, session, offset)?;
        }
        if let Some(model) = model {
            staging::put_log_context(db, path, g, "trace-model", trace, offset, Some(model), None)?;
            staging::update_log_record_trace_model(db, path, g, trace, model, offset)?;
        }
    }
    let name = text(&v["name"]);
    if v["type"].as_str().is_some_and(|s| s != "span")
        || !(a["gen_ai.operation.name"] == "chat" || name.is_some_and(|s| s.starts_with("chat ")))
    {
        return Ok(());
    }
    let input = number(&a["gen_ai.usage.input_tokens"]);
    let output = number(&a["gen_ai.usage.output_tokens"]);
    if input.is_none() && output.is_none() {
        return Ok(());
    }
    let (start, end) = (otel_time(&v["startTime"]), otel_time(&v["endTime"]));
    let Some(at) = start.or(end) else {
        return Ok(());
    };
    // A chat span is one model call: the turn's tool runs and hooks are its
    // sibling spans, so its own start to end is the call's time.
    let took = start
        .zip(end)
        .map(|(start, end)| end - start)
        .filter(|ms| *ms > 0);
    let read = number(&a["gen_ai.usage.cache_read.input_tokens"])
        .or_else(|| number(&a["gen_ai.usage.cache_read_input_tokens"]))
        .unwrap_or(0);
    let span = identifier(&v["spanId"]).or_else(|| identifier(&v["spanContext"]["spanId"]));
    let key = if let (Some(trace), Some(span)) = (trace, span) {
        format!("{trace}:{span}")
    } else if let Some(response) = text(&a["gen_ai.response.id"]) {
        format!("response:{response}")
    } else {
        metering::event_key(&serde_json::to_string(v).unwrap_or_default())
    };
    staging::insert_log_record(
        db,
        path,
        g,
        offset,
        session.unwrap_or(""),
        &key,
        at,
        input.unwrap_or(0).saturating_sub(read) as i64,
        output.unwrap_or(0) as i64,
        read as i64,
        model,
        trace,
        None,
        took,
    )?;
    Ok(())
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
