//! Antigravity protobuf staging (`source_proto_identity`).

use rusqlite::{params, Connection};

pub(crate) fn clear_stale_proto_identity(
    db: &Connection,
    path: &str,
    generation: &str,
) -> rusqlite::Result<()> {
    db.execute(
        "DELETE FROM source_proto_identity WHERE path=?1 AND generation<>?2",
        params![path, generation],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn upsert_proto_identity(
    db: &Connection,
    path: &str,
    generation: &str,
    kind: i64,
    idx: i64,
    message: Option<&str>,
    execution: Option<&str>,
    at: Option<i64>,
    label: Option<&str>,
    model: Option<&str>,
    response: Option<&str>,
) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO source_proto_identity(path,generation,kind,idx,message,execution,at_ms,label,model,response)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(path,generation,kind,idx) DO UPDATE SET
           message=excluded.message,
           execution=excluded.execution,
           at_ms=excluded.at_ms,
           label=excluded.label,
           model=excluded.model,
           response=excluded.response",
        params![path, generation, kind, idx, message, execution, at, label, model, response],
    )?;
    Ok(())
}

pub(crate) fn proto_unique_step_time(
    db: &Connection,
    path: &str,
    generation: &str,
    key: &str,
    value: &str,
    message_present: bool,
    turn_execution: Option<&str>,
) -> rusqlite::Result<Option<i64>> {
    let n: i64 = db.query_row(
        &format!(
            "SELECT count(*) FROM source_proto_identity WHERE path=?1 AND generation=?2 AND kind=0 AND {key}=?3"
        ),
        params![path, generation, value],
        |r| r.get(0),
    )?;
    if n != 1 {
        return Ok(None);
    }
    let mut stmt = db.prepare(&format!(
        "SELECT at_ms,execution FROM source_proto_identity WHERE path=?1 AND generation=?2 AND kind=1 AND {key}=?3 LIMIT 2"
    ))?;
    let rows = stmt
        .query_map(params![path, generation, value], |r| {
            Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<String>>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(
        if rows.len() == 1
            && (!message_present
                || turn_execution.is_none()
                || turn_execution == rows[0].1.as_deref())
        {
            rows[0].0
        } else {
            None
        },
    )
}

pub(crate) fn proto_first_response_idx(
    db: &Connection,
    path: &str,
    generation: &str,
    response: &str,
) -> rusqlite::Result<i64> {
    db.query_row(
        "SELECT min(idx) FROM source_proto_identity WHERE path=?1 AND generation=?2 AND kind=0 AND response=?3",
        params![path, generation, response],
        |r| r.get(0),
    )
}

pub(crate) fn proto_unique_label_model(
    db: &Connection,
    path: &str,
    generation: &str,
    label: &str,
) -> rusqlite::Result<Option<String>> {
    let mut stmt = db.prepare(
        "SELECT DISTINCT model FROM source_proto_identity WHERE path=?1 AND generation=?2 AND kind=0 AND label=?3 AND model IS NOT NULL LIMIT 2",
    )?;
    let values = stmt
        .query_map(params![path, generation, label], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok((values.len() == 1).then(|| values[0].clone()))
}
