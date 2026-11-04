//! GitHub Copilot's session JSONL records.
use super::*;

pub(super) fn copilot(
    db: &Connection,
    path: &str,
    g: &str,
    offset: u64,
    v: &Value,
) -> rusqlite::Result<()> {
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
