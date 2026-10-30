//! Unified SQLite storage layer for Agent HUD Core (`ingestion.sqlite`).
//!
//! Centralizes all table schemas, migrations, models, checkpoints, session mappings,
//! metering events/buckets, quota samples, and source staging tables.

pub(crate) mod checkpoints;
pub(crate) mod endpoints;
pub(crate) mod metering;
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

fn ensure_column(db: &Connection, table: &str, column: &str, def: &str) -> rusqlite::Result<()> {
    let exists = db
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|name| name == column);
    if !exists {
        db.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {def}"))?;
    }
    Ok(())
}

/// Initializes all database tables, columns, and indexes in one place.
pub(crate) fn schema(db: &Connection) -> rusqlite::Result<()> {
    // A large one-off write (a migration, an import) grows the WAL file, and
    // SQLite keeps that size after checkpointing unless a limit is set.
    db.execute_batch(
        "PRAGMA journal_mode=WAL;
        PRAGMA journal_size_limit=16777216;
        CREATE TABLE IF NOT EXISTS ingestion_logs(
            path TEXT PRIMARY KEY,
            payload TEXT NOT NULL,
            head TEXT,
            updated_at_ms INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS ingestion_meta(
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS session_mapping(
            source TEXT NOT NULL,
            session_id TEXT NOT NULL,
            canonical_path TEXT NOT NULL,
            parent_id TEXT,
            updated_at_ms INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(source, session_id)
        );
        CREATE INDEX IF NOT EXISTS session_mapping_updated ON session_mapping(updated_at_ms);
        CREATE TABLE IF NOT EXISTS session_endpoint(
            source TEXT NOT NULL,
            session_id TEXT NOT NULL,
            endpoint TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(source, session_id)
        );

        CREATE TABLE IF NOT EXISTS quota_sample(
            window_id TEXT NOT NULL,
            observed_at_ms INTEGER NOT NULL,
            remaining REAL NOT NULL,
            PRIMARY KEY(window_id, observed_at_ms)
        );
        CREATE TABLE IF NOT EXISTS usage_meter_event(
            contribution TEXT NOT NULL,
            event_key TEXT NOT NULL,
            source TEXT NOT NULL,
            model TEXT NOT NULL,
            at_ms INTEGER NOT NULL,
            input INTEGER NOT NULL,
            output INTEGER NOT NULL,
            cache_read INTEGER NOT NULL,
            cache_write INTEGER NOT NULL,
            reasoning INTEGER NOT NULL,
            gap_ms INTEGER,
            updated_at_ms INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(contribution, event_key)
        );
        CREATE INDEX IF NOT EXISTS usage_meter_event_order ON usage_meter_event(contribution, at_ms, event_key);
        CREATE INDEX IF NOT EXISTS usage_meter_event_date ON usage_meter_event(at_ms);
        CREATE TABLE IF NOT EXISTS usage_meter_bucket(
            contribution TEXT NOT NULL,
            source TEXT NOT NULL,
            model TEXT NOT NULL,
            at_ms INTEGER NOT NULL,
            input INTEGER NOT NULL,
            output INTEGER NOT NULL,
            cache_read INTEGER NOT NULL,
            cache_write INTEGER NOT NULL,
            reasoning INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(contribution, model, at_ms)
        );
        CREATE INDEX IF NOT EXISTS usage_meter_bucket_date ON usage_meter_bucket(at_ms);
        CREATE TEMP TABLE IF NOT EXISTS meter_selected(contribution TEXT PRIMARY KEY);

        CREATE TABLE IF NOT EXISTS source_log_context(
            path TEXT NOT NULL,
            generation TEXT NOT NULL,
            kind TEXT NOT NULL,
            key TEXT NOT NULL,
            offset INTEGER NOT NULL,
            value TEXT,
            aux TEXT,
            PRIMARY KEY(path, generation, kind, key, offset)
        );
        CREATE INDEX IF NOT EXISTS source_log_context_match ON source_log_context(path, generation, kind, key, offset);
        CREATE TABLE IF NOT EXISTS source_log_record(
            path TEXT NOT NULL,
            generation TEXT NOT NULL,
            offset INTEGER NOT NULL,
            id TEXT NOT NULL,
            event_key TEXT NOT NULL,
            at_ms INTEGER NOT NULL,
            input INTEGER NOT NULL,
            output INTEGER NOT NULL,
            cache_read INTEGER NOT NULL,
            model TEXT,
            context TEXT,
            candidate TEXT,
            PRIMARY KEY(path, generation, offset)
        );
        CREATE TABLE IF NOT EXISTS source_proto_identity(
            path TEXT NOT NULL,
            generation TEXT NOT NULL,
            kind INTEGER NOT NULL,
            idx INTEGER NOT NULL,
            message TEXT,
            execution TEXT,
            at_ms INTEGER,
            label TEXT,
            model TEXT,
            response TEXT,
            PRIMARY KEY(path, generation, kind, idx)
        );
        CREATE INDEX IF NOT EXISTS source_proto_message ON source_proto_identity(path, generation, kind, message);
        CREATE INDEX IF NOT EXISTS source_proto_execution ON source_proto_identity(path, generation, kind, execution);
        CREATE INDEX IF NOT EXISTS source_proto_label ON source_proto_identity(path, generation, kind, label);
        CREATE INDEX IF NOT EXISTS source_proto_response ON source_proto_identity(path, generation, kind, response);",
    )?;

    ensure_column(db, "ingestion_logs", "head", "TEXT")?;
    ensure_column(
        db,
        "ingestion_logs",
        "updated_at_ms",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        db,
        "usage_meter_event",
        "updated_at_ms",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        db,
        "usage_meter_bucket",
        "updated_at_ms",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    // The context window a client reported for a call, and the tools the call
    // asked for (a JSON array), as the log's lines said.
    ensure_column(db, "usage_meter_event", "context_window", "INTEGER")?;
    ensure_column(db, "usage_meter_event", "tools", "TEXT")?;
    // How long the call itself took, where its log records it (NULL where
    // not); calls read before the column existed keep NULL.
    ensure_column(db, "usage_meter_event", "duration_ms", "INTEGER")?;
    ensure_column(db, "source_log_record", "duration_ms", "INTEGER")?;
    // Prompt (0) and compaction (1) marks drive a session's turns.
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS usage_meter_mark(
            contribution TEXT NOT NULL,
            at_ms INTEGER NOT NULL,
            kind INTEGER NOT NULL,
            PRIMARY KEY(contribution, at_ms, kind)
        ) WITHOUT ROWID;
        CREATE INDEX IF NOT EXISTS usage_meter_mark_date ON usage_meter_mark(at_ms);",
    )?;
    // The largest prompt each model was seen with, kept past the retention;
    // seeded once from the calls already recorded.
    let seeded = db
        .prepare(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='usage_meter_model_context'",
        )?
        .exists([])?;
    if !seeded {
        db.execute_batch(
            "CREATE TABLE usage_meter_model_context(
                source TEXT NOT NULL,
                model TEXT NOT NULL,
                largest INTEGER NOT NULL,
                PRIMARY KEY(source, model)
            ) WITHOUT ROWID;
            INSERT OR REPLACE INTO usage_meter_model_context(source,model,largest)
                SELECT source,model,MAX(input+cache_write+cache_read) FROM usage_meter_event
                WHERE model<>'' GROUP BY source,model;",
        )?;
    }

    db.execute_batch(
        "CREATE INDEX IF NOT EXISTS ingestion_logs_updated ON ingestion_logs(updated_at_ms);
        CREATE INDEX IF NOT EXISTS usage_meter_event_updated ON usage_meter_event(updated_at_ms);
        CREATE INDEX IF NOT EXISTS usage_meter_bucket_updated ON usage_meter_bucket(updated_at_ms);",
    )?;

    // Clean up any legacy synthetic projection rows from older builds.
    let _ = db.execute_batch(
        "DELETE FROM usage_meter_event WHERE contribution LIKE '%/.agent-hud-projection-%';
         DELETE FROM usage_meter_bucket WHERE contribution LIKE '%/.agent-hud-projection-%';
         DELETE FROM ingestion_logs WHERE path LIKE '%/.agent-hud-projection-%';",
    );
    remove_turn_text(db)?;
    forget_dashboard_usage(db)
}

/// Until 2026-10-26 Cursor's and Grok Bot's sessions and tokens came from
/// Cursor's dashboard, which stopped listing Grok Bot's usage after a plan
/// change and counted Grok Bot's daily spend a second time as a Cursor
/// session. Cursor is read on this Mac now (its sessions in its database,
/// its tokens in its turn log) and Grok Bot keeps no tokens on this Mac, so
/// everything the dashboard gave leaves once, by `user_version` 2; the
/// sessions are read again from their local sources.
fn forget_dashboard_usage(db: &Connection) -> rusqlite::Result<()> {
    let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 2 {
        return Ok(());
    }
    let dashboard = "SELECT path FROM ingestion_logs
        WHERE CASE WHEN json_valid(coalesce(head,payload))
              THEN json_extract(coalesce(head,payload),'$.source') END IN ('cursor','grok-bot')";
    db.execute_batch(&format!(
        "BEGIN;
         DELETE FROM usage_meter_event WHERE source IN ('cursor','grok-bot');
         DELETE FROM usage_meter_bucket WHERE source IN ('cursor','grok-bot');
         DELETE FROM usage_meter_mark WHERE contribution IN ({dashboard});
         DELETE FROM usage_meter_model_context WHERE source IN ('cursor','grok-bot');
         DELETE FROM session_mapping WHERE source IN ('cursor','grok-bot');
         DELETE FROM ingestion_logs WHERE path IN ({dashboard});
         PRAGMA user_version=2;
         COMMIT;"
    ))
}

/// Checkpoints keep no conversation text since 2026-10-21 (agent-hud-native-open's
/// ledger stores none); the turn messages, final answers and desktop asks'
/// words earlier builds wrote leave them once, by `user_version` 1.
fn remove_turn_text(db: &Connection) -> rusqlite::Result<()> {
    let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 1 {
        return Ok(());
    }
    let words = (0..32)
        .map(|turn| format!("'$.live.turns[{turn}].message'"))
        .chain(
            [
                "final_answer",
                "activity.desktop_ask.question",
                "activity.desktop_ask.recap",
                "activity.desktop_ask.options",
            ]
            .map(|field| format!("'$.live.{field}'")),
        )
        .collect::<Vec<_>>()
        .join(",");
    let holds = |column: &str| {
        ["\"message\":\"", "\"final_answer\":\"", "\"question\":\""]
            .map(|key| format!("instr({column},'{key}')>0"))
            .join(" OR ")
    };
    db.execute_batch(&format!(
        "BEGIN;
         UPDATE ingestion_logs
         SET payload=CASE WHEN json_valid(payload) THEN json_remove(payload,{words}) ELSE payload END,
             head=CASE WHEN json_valid(head) THEN json_remove(head,{words}) ELSE head END
         WHERE {} OR {};
         PRAGMA user_version=1;
         COMMIT;",
        holds("payload"),
        holds("head"),
    ))
}

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
    match open_checked(directory).and_then(load) {
        // The ledger is rebuilt from the agents' own logs, so a damaged copy
        // is set aside and indexing starts over instead of failing every read.
        Err(error) if corrupt(&error) => {
            set_aside(directory);
            open_checked(directory).and_then(load)
        }
        result => result,
    }
}

const DAMAGED: &str = "ingestion.sqlite is damaged";

fn corrupt(error: &str) -> bool {
    error.starts_with(DAMAGED)
        || error.contains("malformed")
        || error.contains("not a database")
        || error.contains("corrupt")
}

/// Opens the ledger and checks its structure before anything reads it.
fn open_checked(directory: &Path) -> Result<Connection, String> {
    let database =
        Connection::open(directory.join("ingestion.sqlite")).map_err(|e| e.to_string())?;
    database
        .busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let check: String = database
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if check != "ok" {
        return Err(format!("{DAMAGED}: {check}"));
    }
    schema(&database).map_err(|e| e.to_string())?;
    windows_paths::migrate(&database).map_err(|e| e.to_string())?;
    Ok(database)
}

/// Moves a damaged ledger and its journal files beside it, named by time.
fn set_aside(directory: &Path) {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    for name in [
        "ingestion.sqlite",
        "ingestion.sqlite-wal",
        "ingestion.sqlite-shm",
    ] {
        let path = directory.join(name);
        if path.exists() {
            let _ = fs::rename(&path, directory.join(format!("{name}.damaged-{stamp}")));
        }
    }
}

#[allow(clippy::type_complexity)]
fn load(
    database: Connection,
) -> Result<
    (
        Connection,
        HashMap<PathBuf, checkpoints::LogHead>,
        HashMap<(String, String), PathBuf>,
        i64,
    ),
    String,
> {
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
mod tests {
    use super::*;

    #[test]
    fn a_damaged_ledger_is_set_aside_and_rebuilt() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("data");
        fs::create_dir_all(&directory).unwrap(); // plain directory
        let mut garbage = b"SQLite format 3\0".to_vec();
        garbage.extend(std::iter::repeat_n(0xA5u8, 8192));
        fs::write(directory.join("ingestion.sqlite"), &garbage).unwrap();
        let (database, logs, _, _) = open(&directory).expect("a fresh ledger replaces it");
        assert!(logs.is_empty());
        let check: String = database
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(check, "ok");
        let kept = fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".damaged-"))
            .count();
        assert_eq!(kept, 1);
    }

    #[test]
    fn a_large_write_leaves_the_wal_file_small_after_a_checkpoint() {
        let directory = tempfile::tempdir().unwrap();
        let (db, ..) = open(directory.path()).unwrap();
        let big = "x".repeat(32 * 1024 * 1024);
        db.execute(
            "INSERT INTO ingestion_meta(key,value) VALUES('big',?1)",
            [&big],
        )
        .unwrap();
        // The automatic checkpoint after each commit merges the big write;
        // the next writes restart the WAL from the beginning.
        for value in ["1", "2"] {
            db.execute(
                "INSERT OR REPLACE INTO ingestion_meta(key,value) VALUES('small',?1)",
                [value],
            )
            .unwrap();
        }
        let wal = fs::metadata(directory.path().join("ingestion.sqlite-wal"))
            .unwrap()
            .len();
        assert!(wal <= 16 * 1024 * 1024, "WAL file kept {wal} bytes");
    }

    #[test]
    fn turn_text_earlier_builds_kept_leaves_the_checkpoints_once() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE ingestion_logs(path TEXT PRIMARY KEY,payload TEXT NOT NULL,head TEXT,updated_at_ms INTEGER NOT NULL DEFAULT 0)")
            .unwrap();
        let live = r#"{"live":{"turns":[{"id":"t1","observed":1,"running":false,"message":"All green."}],"final_answer":"All green.","activity":{"desktop_ask":{"id":"c1","kind":"next_step","question":"Commit it?","recap":"Done.","options":[{"label":"Commit"}],"at":1,"offset":42}}}}"#;
        db.execute("INSERT INTO ingestion_logs(path,payload,head) VALUES('a',?1,?1),('b','{\"live\":{}}',NULL)", [live])
            .unwrap();
        schema(&db).unwrap();
        let (payload, head): (String, String) = db
            .query_row(
                "SELECT payload,head FROM ingestion_logs WHERE path='a'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        for stored in [&payload, &head] {
            let value: serde_json::Value = serde_json::from_str(stored).unwrap();
            assert_eq!(
                value["live"]["turns"][0],
                serde_json::json!({"id":"t1","observed":1,"running":false})
            );
            assert_eq!(value["live"]["final_answer"], serde_json::Value::Null);
            assert_eq!(
                value["live"]["activity"]["desktop_ask"],
                serde_json::json!({"id":"c1","kind":"next_step","at":1,"offset":42})
            );
        }
        let version: i64 = db
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
    }

    #[test]
    fn the_dashboards_cursor_and_grok_bot_usage_leaves_once() {
        let db = Connection::open_in_memory().unwrap();
        schema(&db).unwrap();
        db.execute_batch("PRAGMA user_version=1").unwrap();
        let log = |path: &str, source: &str| {
            db.execute(
                "INSERT INTO ingestion_logs(path,payload,head) VALUES(?1,'{}',?2)",
                [path, &format!(r#"{{"source":"{source}"}}"#)],
            )
            .unwrap();
            db.execute(
                "INSERT INTO usage_meter_event(contribution,event_key,source,model,at_ms,input,output,cache_read,cache_write,reasoning) VALUES(?1,'k',?2,'m',1,1,1,0,0,0)",
                [path, source],
            )
            .unwrap();
            db.execute(
                "INSERT INTO usage_meter_mark(contribution,at_ms,kind) VALUES(?1,1,0)",
                [path],
            )
            .unwrap();
            db.execute(
                "INSERT INTO session_mapping(source,session_id,canonical_path) VALUES(?1,?2,?2)",
                [source, path],
            )
            .unwrap();
        };
        log("/account/cursor#hud-session-61", "cursor");
        log("/bot/roster.blob#hud-session-62", "grok-bot");
        log("/bot/roster.blob", "grok-bot");
        log("/claude/s.jsonl", "claude");
        db.execute(
            "INSERT INTO ingestion_logs(path,payload,head) VALUES('/odd','not json',NULL)",
            [],
        )
        .unwrap();
        schema(&db).unwrap();
        let count = |sql: &str| -> i64 { db.query_row(sql, [], |row| row.get(0)).unwrap() };
        assert_eq!(
            count("SELECT count(*) FROM ingestion_logs"),
            2,
            "the Claude log and the unreadable one stay"
        );
        assert_eq!(count("SELECT count(*) FROM usage_meter_event"), 1);
        assert_eq!(count("SELECT count(*) FROM usage_meter_mark"), 1);
        assert_eq!(count("SELECT count(*) FROM session_mapping"), 1);
        assert_eq!(count("PRAGMA user_version"), 2);

        // A later Cursor turn stays: the cleanup runs once.
        log("/data/clients/cursor/turns-2026-10.jsonl", "cursor");
        schema(&db).unwrap();
        assert_eq!(count("SELECT count(*) FROM usage_meter_event"), 2);
    }
}
