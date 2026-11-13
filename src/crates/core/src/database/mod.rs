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

type Opened = (
    Connection,
    HashMap<PathBuf, checkpoints::LogHead>,
    HashMap<(String, String), PathBuf>,
    i64,
);

/// Opens `ingestion.sqlite`, applies schema migrations, and loads initial heads and session mappings.
pub(crate) fn open(directory: &Path) -> Result<Opened, String> {
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    match open_once(directory) {
        // The ledger is rebuilt from the agents' own logs, so a damaged one is
        // deleted and indexing starts over instead of failing every read.
        Err(failure) if failure.damaged => {
            remove_ledger(directory)?;
            open_once(directory).map_err(|failure| failure.message)
        }
        result => result.map_err(|failure| failure.message),
    }
}

/// Why the ledger did not open, and whether SQLite says the file is damaged.
struct Failure {
    message: String,
    damaged: bool,
}
impl Failure {
    fn sql(error: rusqlite::Error) -> Self {
        Self {
            damaged: matches!(
                error.sqlite_error_code(),
                Some(rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase)
            ),
            message: error.to_string(),
        }
    }
    fn other(message: String) -> Self {
        Self {
            message,
            damaged: false,
        }
    }
}

/// Deletes a damaged ledger and its journal files.
fn remove_ledger(directory: &Path) -> Result<(), String> {
    for name in [
        "ingestion.sqlite",
        "ingestion.sqlite-wal",
        "ingestion.sqlite-shm",
    ] {
        match fs::remove_file(directory.join(name)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(error.to_string())
            }
            _ => {}
        }
    }
    Ok(())
}

fn open_once(directory: &Path) -> Result<Opened, Failure> {
    let database = Connection::open(directory.join("ingestion.sqlite")).map_err(Failure::sql)?;
    database
        .busy_timeout(Duration::from_secs(5))
        .map_err(Failure::sql)?;
    schema(&database).map_err(Failure::sql)?;
    windows_paths::migrate(&database).map_err(Failure::sql)?;

    let transaction = database.unchecked_transaction().map_err(Failure::sql)?;
    let mut logs = HashMap::new();
    let mut max_updated_at = 0i64;
    {
        let mut statement = transaction
            .prepare(
                "SELECT path,coalesce(head,payload),head IS NULL,updated_at_ms FROM ingestion_logs",
            )
            .map_err(Failure::sql)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(Failure::sql)?;
        for row in rows {
            let (path, payload, upgrade, updated_at_ms) = row.map_err(Failure::sql)?;
            max_updated_at = max_updated_at.max(updated_at_ms);
            let head = checkpoints::LogHead::decode(&payload)
                .map_err(|e| Failure::other(format!("Unreadable checkpoint head: {e}")))?;
            if upgrade {
                let encoded =
                    serde_json::to_string(&head).map_err(|e| Failure::other(e.to_string()))?;
                transaction
                    .execute(
                        "UPDATE ingestion_logs SET head=?2 WHERE path=?1",
                        params![path, encoded],
                    )
                    .map_err(Failure::sql)?;
            }
            logs.insert(PathBuf::from(path), head);
        }
    }
    let mut session_map = HashMap::new();
    {
        let mut statement = transaction
            .prepare("SELECT source,session_id,canonical_path,updated_at_ms FROM session_mapping")
            .map_err(Failure::sql)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(Failure::sql)?;
        for row in rows {
            let (source, session_id, canonical_path, updated_at_ms) = row.map_err(Failure::sql)?;
            max_updated_at = max_updated_at.max(updated_at_ms);
            session_map.insert((source, session_id), PathBuf::from(canonical_path));
        }
    }
    transaction.commit().map_err(Failure::sql)?;
    Ok((database, logs, session_map, max_updated_at))
}

#[cfg(test)]
mod tests;
