//! Persisted presentation heads, parser cache, and incremental checkpoint/session-mapping queries.
use crate::Log;
use rusqlite::{params, Connection};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

mod log_head;

pub(crate) use log_head::LogHead;

const CACHE_LOGS: usize = 24;
const CACHE_BYTES: usize = 24 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct ParserCache {
    entries: VecDeque<(PathBuf, Log, usize)>,
    bytes: usize,
}

impl ParserCache {
    pub fn take(&mut self, path: &Path) -> Option<(Log, usize)> {
        let index = self.entries.iter().position(|(key, _, _)| key == path)?;
        let (_, log, bytes) = self.entries.remove(index)?;
        self.bytes -= bytes;
        Some((log, bytes))
    }

    pub fn remove(&mut self, path: &Path) {
        self.take(path);
    }

    pub fn insert(&mut self, log: Log, bytes: usize) {
        self.remove(&log.path);
        if bytes > CACHE_BYTES {
            return;
        }
        while self.entries.len() >= CACHE_LOGS || self.bytes + bytes > CACHE_BYTES {
            if let Some((_, _, size)) = self.entries.pop_front() {
                self.bytes -= size;
            } else {
                break;
            }
        }
        self.bytes += bytes;
        self.entries.push_back((log.path.clone(), log, bytes));
    }
}

pub(crate) fn read(database: &Connection, path: &Path) -> Result<(Log, usize), String> {
    let payload: String = database
        .query_row(
            "SELECT payload FROM ingestion_logs WHERE path=?1",
            [path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .map_err(|_| "Parser checkpoint could not be read".to_owned())?;
    let mut log: Log = serde_json::from_str(&payload)
        .map_err(|_| "Parser checkpoint could not be decoded".to_owned())?;
    log.restore_buckets();
    Ok((log, payload.len()))
}

pub(crate) fn upsert_log(
    db: &Connection,
    path: &str,
    payload: &str,
    head: &str,
    updated_at_ms: i64,
) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO ingestion_logs(path,payload,head,updated_at_ms) VALUES(?1,?2,?3,?4)
         ON CONFLICT(path) DO UPDATE SET payload=excluded.payload,head=excluded.head,updated_at_ms=excluded.updated_at_ms",
        params![path, payload, head, updated_at_ms],
    )?;
    Ok(())
}

pub(crate) fn update_head(
    db: &Connection,
    path: &str,
    head: &str,
    updated_at_ms: i64,
) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE ingestion_logs SET head=?1, updated_at_ms=?2 WHERE path=?3",
        params![head, updated_at_ms, path],
    )?;
    Ok(())
}

pub(crate) fn delete_log(db: &Connection, path: &str) -> rusqlite::Result<()> {
    db.execute("DELETE FROM ingestion_logs WHERE path=?1", [path])?;
    Ok(())
}

pub(crate) fn rename_log(
    db: &Connection,
    from_path: &str,
    to_path: &str,
    payload: &str,
    head: &str,
    updated_at_ms: i64,
) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE ingestion_logs SET path=?1,payload=?2,head=?3,updated_at_ms=?4 WHERE path=?5",
        params![to_path, payload, head, updated_at_ms, from_path],
    )?;
    Ok(())
}

pub(crate) fn upsert_session_mapping(
    db: &Connection,
    source: &str,
    session_id: &str,
    canonical_path: &str,
    parent_id: Option<&str>,
    updated_at_ms: i64,
) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO session_mapping(source,session_id,canonical_path,parent_id,updated_at_ms)
         VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(source,session_id) DO UPDATE SET
           canonical_path=excluded.canonical_path,
           parent_id=coalesce(excluded.parent_id,session_mapping.parent_id),
           updated_at_ms=excluded.updated_at_ms",
        params![source, session_id, canonical_path, parent_id, updated_at_ms],
    )?;
    Ok(())
}

pub(crate) struct SyncChanges {
    pub heads: Vec<(PathBuf, LogHead)>,
    pub mappings: Vec<((String, String), PathBuf)>,
    pub max_updated_at_ms: i64,
    pub changed: bool,
}

/// Queries all records modified after `since_ms` across `ingestion_logs`, `session_mapping`,
/// `usage_meter_event`, and `usage_meter_bucket`. The rows of the logs in `held` move the time
/// on but are neither read back nor changes: the run that wrote them holds them already.
pub(crate) fn sync_since(
    db: &Connection,
    since_ms: i64,
    held: &[&str],
) -> Result<SyncChanges, String> {
    let mut max_ts = since_ms;
    let mut changed = false;
    let mut heads = Vec::new();
    {
        let mut stmt = db
            .prepare(
                "SELECT path, head, updated_at_ms FROM ingestion_logs WHERE updated_at_ms > ?1",
            )
            .map_err(|_| "Could not read updated checkpoints")?;
        let rows = stmt
            .query_map([since_ms], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|_| "Could not query updated checkpoints")?;
        for row in rows {
            let (path_str, head_json, ts) =
                row.map_err(|_| "Could not decode updated checkpoint")?;
            max_ts = max_ts.max(ts);
            if held.contains(&path_str.as_str()) {
                continue;
            }
            if let Some(head_json) = head_json {
                let mut head: LogHead =
                    LogHead::decode(&head_json).map_err(|_| "Could not parse checkpoint head")?;
                let path = PathBuf::from(path_str);
                head.path = path.clone();
                heads.push((path, head));
                changed = true;
            }
        }
    }
    let mut mappings = Vec::new();
    {
        let mut stmt = db
            .prepare("SELECT source, session_id, canonical_path, updated_at_ms FROM session_mapping WHERE updated_at_ms > ?1")
            .map_err(|_| "Could not read updated session mappings")?;
        let rows = stmt
            .query_map([since_ms], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|_| "Could not query updated session mappings")?;
        for row in rows {
            let (source, session_id, canonical_path, ts) =
                row.map_err(|_| "Could not decode session mapping")?;
            max_ts = max_ts.max(ts);
            if held.contains(&canonical_path.as_str()) {
                continue;
            }
            mappings.push(((source, session_id), PathBuf::from(canonical_path)));
            changed = true;
        }
    }
    // Only the rows written since, through the updated_at_ms index: grouping
    // them in SQL had SQLite walk every row in contribution order instead.
    for sql in [
        "SELECT contribution, updated_at_ms FROM usage_meter_event WHERE updated_at_ms > ?1",
        "SELECT contribution, updated_at_ms FROM usage_meter_bucket WHERE updated_at_ms > ?1",
    ] {
        let mut stmt = db
            .prepare_cached(sql)
            .map_err(|_| "Could not read updated usage")?;
        let rows = stmt
            .query_map([since_ms], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(|_| "Could not query updated usage")?;
        for row in rows {
            let (contribution, ts) = row.map_err(|_| "Could not decode updated usage")?;
            max_ts = max_ts.max(ts);
            changed |= !held.contains(&contribution.as_str());
        }
    }
    Ok(SyncChanges {
        heads,
        mappings,
        max_updated_at_ms: max_ts,
        changed,
    })
}
