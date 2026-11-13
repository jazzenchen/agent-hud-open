use super::*;

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

#[test]
fn a_damaged_ledger_is_deleted_and_rebuilt() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("data");
    fs::create_dir_all(&directory).unwrap();
    let mut garbage = b"SQLite format 3\0".to_vec();
    garbage.extend(std::iter::repeat_n(0xA5u8, 8192));
    fs::write(directory.join("ingestion.sqlite"), &garbage).unwrap();
    let (database, logs, _, _) = open(&directory).expect("a fresh ledger replaces it");
    assert!(logs.is_empty());
    let check: String = database
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(check, "ok");
    let names: Vec<String> = fs::read_dir(&directory)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names
            .iter()
            .all(|name| name.starts_with("ingestion.sqlite") && !name.contains("damaged")),
        "no copy of the damaged file is kept: {names:?}"
    );
}

#[test]
fn a_ledger_that_fails_for_another_reason_is_left_alone() {
    let temp = tempfile::tempdir().unwrap();
    // A folder where the file should be: not damage, so nothing is deleted.
    fs::create_dir_all(temp.path().join("ingestion.sqlite")).unwrap();
    assert!(open(temp.path()).is_err());
    assert!(temp.path().join("ingestion.sqlite").is_dir());
}
