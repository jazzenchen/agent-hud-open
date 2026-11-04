//! Unified SQLite storage layer for Agent HUD Core (`ingestion.sqlite`).
//!
//! Centralizes all table schemas, migrations, models, checkpoints, session mappings,
//! metering events/buckets, quota samples, and source staging tables.

pub(crate) mod checkpoints;
pub(crate) mod endpoints;
pub(crate) mod metering;
pub(crate) mod quota_link;
mod schema;
pub(crate) mod staging;
#[cfg(windows)]
mod windows_paths;
#[cfg(not(windows))]
#[path = "unix_paths.rs"]
mod windows_paths;

use rusqlite::{params, Connection};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) use schema::schema;

/// Opens `ingestion.sqlite`, applies schema migrations, and loads initial heads and session mappings.
pub(crate) fn open(
    directory: &Path,
) -> Result<
    (
        Connection,
        HashMap<PathBuf, checkpoints::LogHead>,
        HashMap<(String, String), PathBuf>,
        i64,
    ),
    String,
> {
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let database =
        Connection::open(directory.join("ingestion.sqlite")).map_err(|e| e.to_string())?;
    database
        .busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    schema(&database).map_err(|e| e.to_string())?;
    windows_paths::migrate(&database).map_err(|e| e.to_string())?;

    let transaction = database
        .unchecked_transaction()
        .map_err(|e| e.to_string())?;
    let mut logs = HashMap::new();
    let mut max_updated_at = 0i64;
    {
        let mut statement = transaction
            .prepare(
                "SELECT path,coalesce(head,payload),head IS NULL,updated_at_ms FROM ingestion_logs",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (path, payload, upgrade, updated_at_ms) = row.map_err(|e| e.to_string())?;
            max_updated_at = max_updated_at.max(updated_at_ms);
            let head = checkpoints::LogHead::decode(&payload)
                .map_err(|e| format!("Unreadable checkpoint head: {e}"))?;
            if upgrade {
                let encoded = serde_json::to_string(&head).map_err(|e| e.to_string())?;
                transaction
                    .execute(
                        "UPDATE ingestion_logs SET head=?2 WHERE path=?1",
                        params![path, encoded],
                    )
                    .map_err(|e| e.to_string())?;
            }
            logs.insert(PathBuf::from(path), head);
        }
    }
    let mut session_map = HashMap::new();
    {
        let mut statement = transaction
            .prepare("SELECT source,session_id,canonical_path,updated_at_ms FROM session_mapping")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (source, session_id, canonical_path, updated_at_ms) =
                row.map_err(|e| e.to_string())?;
            max_updated_at = max_updated_at.max(updated_at_ms);
            session_map.insert((source, session_id), PathBuf::from(canonical_path));
        }
    }
    transaction.commit().map_err(|e| e.to_string())?;
    Ok((database, logs, session_map, max_updated_at))
}

#[cfg(test)]
mod tests;
