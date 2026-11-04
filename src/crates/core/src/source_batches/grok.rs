//! Grok CLI's `unified.jsonl`: its models and its records.
use super::*;

pub(crate) fn grok_models(
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
pub(super) fn grok(
    db: &Connection,
    path: &str,
    g: &str,
    offset: u64,
    v: &Value,
) -> rusqlite::Result<()> {
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
