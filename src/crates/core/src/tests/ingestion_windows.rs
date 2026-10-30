use super::*;

#[test]
fn windows_path_aliases_migrate_then_increment_and_restart_without_double_counting() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".claude/projects/p");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("11111111-2222-3333-4444-555555555555.jsonl");
    let at = Utc::now().to_rfc3339();
    let reply = |id: &str, output| {
        format!(
            "{}\n",
            json!({"sessionId":"11111111-2222-3333-4444-555555555555","type":"assistant","timestamp":at,"message":{"id":id,"model":"claude-opus-4-5","usage":{"input_tokens":10,"output_tokens":output}}})
        )
    };
    fs::write(&path, reply("m1", 20)).unwrap();
    let mut core = fixture(temp.path());
    assert_eq!(
        core.request("snapshot", json!({})).unwrap()["totals"]["total"],
        30
    );
    drop(core);
    let db = rusqlite::Connection::open(temp.path().join("app/ingestion.sqlite")).unwrap();
    let native = crate::log_files::native_path(&path)
        .to_string_lossy()
        .to_string();
    let mixed = native.replacen("\\p\\", "/p/", 1);
    assert_ne!(native, mixed);
    db.execute("INSERT INTO ingestion_logs(path,payload,head,updated_at_ms) SELECT ?2,json_set(payload,'$.path',?2),json_set(head,'$.path',?2),updated_at_ms+1 FROM ingestion_logs WHERE path=?1", rusqlite::params![native,mixed]).unwrap();
    db.execute(
        "UPDATE session_mapping SET canonical_path=?2 WHERE canonical_path=?1",
        rusqlite::params![native, mixed],
    )
    .unwrap();
    db.execute(
        "UPDATE usage_meter_event SET contribution=?2 WHERE contribution=?1",
        rusqlite::params![native, mixed],
    )
    .unwrap();
    db.execute(
        "UPDATE usage_meter_bucket SET contribution=?2 WHERE contribution=?1",
        rusqlite::params![native, mixed],
    )
    .unwrap();
    db.execute(
        "UPDATE usage_meter_mark SET contribution=?2 WHERE contribution=?1",
        rusqlite::params![native, mixed],
    )
    .unwrap();
    db.execute(
        "DELETE FROM ingestion_meta WHERE key='windows-native-log-paths-v1'",
        [],
    )
    .unwrap();
    drop(db);

    let mut core = fixture(temp.path());
    assert_eq!(
        core.request("snapshot", json!({})).unwrap()["totals"]["total"],
        30
    );
    let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
    write!(file, "{}", reply("m2", 30)).unwrap();
    assert_eq!(
        core.request("refresh", json!({})).unwrap()["totals"]["total"],
        70
    );
    drop(core);
    let mut core = fixture(temp.path());
    assert_eq!(
        core.request("snapshot", json!({})).unwrap()["totals"]["total"],
        70
    );
    let db = rusqlite::Connection::open(temp.path().join("app/ingestion.sqlite")).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM ingestion_logs", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM usage_meter_event", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
