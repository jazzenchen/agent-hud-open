//! Staging table queries (`source_log_context`, `source_log_record`, `source_proto_identity`)
//! used by batch JSONL and protobuf source readers.

use rusqlite::{params, Connection};

mod proto_identity;

pub(crate) use proto_identity::{
    clear_stale_proto_identity, proto_first_response_idx, proto_unique_label_model,
    proto_unique_step_time, upsert_proto_identity,
};

pub(crate) fn put_log_context(
    db: &Connection,
    path: &str,
    generation: &str,
    kind: &str,
    key: &str,
    offset: u64,
    value: Option<&str>,
    aux: Option<&str>,
) -> rusqlite::Result<()> {
    db.execute(
        "INSERT OR REPLACE INTO source_log_context VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![path, generation, kind, key, offset as i64, value, aux],
    )?;
    Ok(())
}

pub(crate) fn get_log_context(
    db: &Connection,
    path: &str,
    generation: &str,
    kind: &str,
    key: &str,
    offset: u64,
    first: bool,
) -> rusqlite::Result<Option<(Option<String>, Option<String>)>> {
    let mut s = db.prepare(&format!(
        "SELECT value,aux FROM source_log_context WHERE path=?1 AND generation=?2 AND kind=?3 AND key=?4 AND offset<=?5 ORDER BY offset {} LIMIT 1",
        if first { "ASC" } else { "DESC" }
    ))?;
    let mut rows = s.query(params![path, generation, kind, key, offset as i64])?;
    rows.next()?.map(|r| Ok((r.get(0)?, r.get(1)?))).transpose()
}

pub(crate) fn count_log_sessions_for_process(
    db: &Connection,
    path: &str,
    generation: &str,
    process: &str,
) -> rusqlite::Result<i64> {
    db.query_row(
        "SELECT count(*) FROM source_log_context WHERE path=?1 AND generation=?2 AND kind='session' AND aux=?3",
        params![path, generation, process],
        |r| r.get(0),
    )
}

pub(crate) fn insert_log_record(
    db: &Connection,
    path: &str,
    generation: &str,
    offset: u64,
    id: &str,
    event_key: &str,
    at_ms: i64,
    input: i64,
    output: i64,
    cache_read: i64,
    model: Option<&str>,
    context: Option<&str>,
    candidate: Option<&str>,
    duration_ms: Option<i64>,
) -> rusqlite::Result<()> {
    db.execute(
        "INSERT OR REPLACE INTO source_log_record(path,generation,offset,id,event_key,at_ms,input,output,cache_read,model,context,candidate,duration_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            path,
            generation,
            offset as i64,
            id,
            event_key,
            at_ms,
            input,
            output,
            cache_read,
            model,
            context,
            candidate,
            duration_ms
        ],
    )?;
    Ok(())
}

pub(crate) fn update_log_record_trace_session(
    db: &Connection,
    path: &str,
    generation: &str,
    trace: &str,
    session: &str,
    offset: u64,
) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE source_log_record SET id=?4,offset=?5 WHERE path=?1 AND generation=?2 AND context=?3 AND id=''",
        params![path, generation, trace, session, offset as i64],
    )?;
    Ok(())
}

pub(crate) fn update_log_record_trace_model(
    db: &Connection,
    path: &str,
    generation: &str,
    trace: &str,
    model: &str,
    offset: u64,
) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE source_log_record SET model=?4,offset=CASE WHEN id<>'' THEN ?5 ELSE offset END WHERE path=?1 AND generation=?2 AND context=?3 AND model IS NULL",
        params![path, generation, trace, model, offset as i64],
    )?;
    Ok(())
}

pub(crate) fn clear_stale_log_staging(
    db: &Connection,
    path: &str,
    generation: &str,
) -> rusqlite::Result<()> {
    for table in ["source_log_context", "source_log_record"] {
        db.execute(
            &format!("DELETE FROM {table} WHERE path=?1 AND generation<>?2"),
            params![path, generation],
        )?;
    }
    Ok(())
}

pub(crate) struct StagedLogRecord {
    pub offset: i64,
    pub id: String,
    pub event_key: String,
    pub at_ms: i64,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub model: Option<String>,
    pub context: Option<String>,
    pub candidate: Option<String>,
    pub duration_ms: Option<i64>,
}

pub(crate) fn query_log_records_page(
    db: &Connection,
    path: &str,
    generation: &str,
    after_offset: i64,
) -> rusqlite::Result<Vec<StagedLogRecord>> {
    let mut stmt = db.prepare(
        "SELECT offset,id,event_key,at_ms,input,output,cache_read,model,context,candidate,duration_ms
         FROM source_log_record WHERE path=?1 AND generation=?2 AND offset>?3 ORDER BY offset LIMIT 500",
    )?;
    let rows = stmt.query_map(params![path, generation, after_offset], |r| {
        Ok(StagedLogRecord {
            offset: r.get(0)?,
            id: r.get(1)?,
            event_key: r.get(2)?,
            at_ms: r.get(3)?,
            input: r.get(4)?,
            output: r.get(5)?,
            cache_read: r.get(6)?,
            model: r.get(7)?,
            context: r.get(8)?,
            candidate: r.get(9)?,
            duration_ms: r.get(10)?,
        })
    })?;
    rows.collect()
}

pub(crate) fn query_log_records_by_session(
    db: &Connection,
    session_id: &str,
) -> rusqlite::Result<
    Vec<(
        String,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
    )>,
> {
    let mut stmt = db.prepare(
        "SELECT path,generation,at_ms,model,context,candidate FROM source_log_record WHERE id=?1",
    )?;
    let rows = stmt.query_map([session_id], |r| {
        Ok((
            r.get(0)?,
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
        ))
    })?;
    rows.collect()
}
