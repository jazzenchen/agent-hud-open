//! Repair Windows path keys written with mixed separators by older versions.
//!
//! This is deliberately Windows-only: a backslash is an ordinary filename
//! character on Unix. It is lexical and does not resolve links or touch files.
use rusqlite::{params, Connection};

const MIGRATION: &str = "windows-native-log-paths-v1";

fn native_key(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let absolute = (bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\'))
        || path.starts_with("\\\\")
        || path.starts_with("//");
    (absolute && path.contains('/')).then(|| path.replace('/', "\\"))
}

pub(super) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM ingestion_meta WHERE key=?1)",
        [MIGRATION],
        |row| row.get::<_, bool>(0),
    )? {
        return tx.commit();
    }
    // Read all owners, including paths only present in the ledger or staging.
    let mut aliases = {
        let mut statement = tx.prepare(
            "SELECT path FROM ingestion_logs UNION SELECT canonical_path FROM session_mapping
             UNION SELECT contribution FROM usage_meter_event UNION SELECT contribution FROM usage_meter_bucket
             UNION SELECT contribution FROM usage_meter_mark UNION SELECT path FROM source_log_context
             UNION SELECT path FROM source_log_record UNION SELECT path FROM source_proto_identity",
        )?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    aliases.sort();
    for old in aliases {
        let Some(new) = native_key(&old) else {
            continue;
        };
        // If both spellings exist, retain the most recently updated checkpoint.
        tx.execute(
            "INSERT INTO ingestion_logs(path,payload,head,updated_at_ms)
             SELECT ?2,payload,head,updated_at_ms FROM ingestion_logs WHERE path=?1
             ON CONFLICT(path) DO UPDATE SET payload=excluded.payload,head=excluded.head,
                 updated_at_ms=excluded.updated_at_ms
             WHERE excluded.updated_at_ms > ingestion_logs.updated_at_ms",
            params![old, new],
        )?;
        tx.execute("DELETE FROM ingestion_logs WHERE path=?1", [&old])?;
        tx.execute(
            "UPDATE session_mapping SET canonical_path=?2 WHERE canonical_path=?1",
            params![old, new],
        )?;

        // Union ledger rows, preserving the newest observation of an identical
        // event or bucket. Never add two copies of a bucket together.
        tx.execute(
            "INSERT OR REPLACE INTO usage_meter_event
             SELECT ?2,e.event_key,e.source,e.model,e.at_ms,e.input,e.output,e.cache_read,e.cache_write,
                    e.reasoning,e.gap_ms,e.updated_at_ms,e.context_window,e.tools,e.duration_ms
             FROM usage_meter_event e WHERE e.contribution=?1 AND NOT EXISTS
                (SELECT 1 FROM usage_meter_event n WHERE n.contribution=?2 AND n.event_key=e.event_key
                 AND n.updated_at_ms>=e.updated_at_ms)", params![old, new])?;
        tx.execute(
            "DELETE FROM usage_meter_event WHERE contribution=?1",
            [&old],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO usage_meter_bucket
             SELECT ?2,b.source,b.model,b.at_ms,b.input,b.output,b.cache_read,b.cache_write,b.reasoning,b.updated_at_ms
             FROM usage_meter_bucket b WHERE b.contribution=?1 AND NOT EXISTS
                (SELECT 1 FROM usage_meter_bucket n WHERE n.contribution=?2 AND n.model=b.model AND n.at_ms=b.at_ms
                 AND n.updated_at_ms>=b.updated_at_ms)", params![old, new])?;
        tx.execute(
            "DELETE FROM usage_meter_bucket WHERE contribution=?1",
            [&old],
        )?;
        // An alias may hold only some events in a bucket. Rebuild every bucket
        // represented by the merged events; leave buckets with no events alone.
        tx.execute(
            "INSERT OR REPLACE INTO usage_meter_bucket(contribution,source,model,at_ms,input,output,cache_read,cache_write,reasoning,updated_at_ms)
             SELECT contribution,source,model,(at_ms/900000)*900000,SUM(input),SUM(output),SUM(cache_read),SUM(cache_write),SUM(reasoning),MAX(updated_at_ms)
             FROM usage_meter_event WHERE contribution=?1 GROUP BY contribution,source,model,(at_ms/900000)*900000",
            [&new],
        )?;
        for (table, column) in [
            ("usage_meter_mark", "contribution"),
            ("source_log_context", "path"),
            ("source_log_record", "path"),
            ("source_proto_identity", "path"),
        ] {
            tx.execute(
                &format!("UPDATE OR IGNORE {table} SET {column}=?2 WHERE {column}=?1"),
                params![old, new],
            )?;
            tx.execute(&format!("DELETE FROM {table} WHERE {column}=?1"), [&old])?;
        }
        // The decoded checkpoint owns its path as well as the SQL row key.
        tx.execute(
            "UPDATE ingestion_logs SET
                payload=CASE WHEN json_valid(payload) THEN json_set(payload,'$.path',?1) ELSE payload END,
                head=CASE WHEN json_valid(head) THEN json_set(head,'$.path',?1) ELSE head END
             WHERE path=?1", [&new])?;
    }
    // A virtual checkpoint may have a separately stored physical origin.
    for column in ["payload", "head"] {
        tx.execute_batch(&format!(
            "UPDATE ingestion_logs SET {column}=json_set({column},'$.origin_path',replace(json_extract({column},'$.origin_path'),'/','\\'))
             WHERE json_valid({column}) AND json_type({column},'$.origin_path')='text'
               AND (json_extract({column},'$.origin_path') GLOB '[A-Za-z]:[\\/]*'
                    OR json_extract({column},'$.origin_path') LIKE '\\\\%'
                    OR json_extract({column},'$.origin_path') LIKE '//%')"
        ))?;
    }
    tx.execute(
        "INSERT INTO ingestion_meta(key,value) VALUES(?1,'1')",
        [MIGRATION],
    )?;
    tx.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_windows_absolute_keys_are_rewritten() {
        assert_eq!(
            native_key("C:\\Users\\me/a.jsonl").as_deref(),
            Some("C:\\Users\\me\\a.jsonl")
        );
        assert_eq!(
            native_key("//server/share/a.jsonl").as_deref(),
            Some("\\\\server\\share\\a.jsonl")
        );
        assert_eq!(native_key("/account/cursor"), None);
        assert_eq!(native_key("C:\\Users\\me\\a.jsonl"), None);
    }

    #[test]
    fn merges_old_spellings_with_newest_checkpoint_and_ledger_and_survives_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let db = Connection::open(temp.path().join("old.sqlite")).unwrap();
        super::super::schema(&db).unwrap();
        let native = r"C:\logs\brain\transcript.jsonl";
        let mixed = r"C:\logs\brain/.system/../transcript.jsonl";
        // Aliases here differ only in separators; '..' must not be resolved.
        let mixed = mixed.replace("/.system/..", "");
        assert_eq!(native_key(&mixed).as_deref(), Some(native));
        for (path, offset, updated) in [(native, 10, 10), (mixed.as_str(), 20, 20)] {
            let payload =
                serde_json::json!({"path":path,"origin_path":path,"offset":offset}).to_string();
            db.execute(
                "INSERT INTO ingestion_logs(path,payload,head,updated_at_ms) VALUES(?1,?2,?2,?3)",
                params![path, payload, updated],
            )
            .unwrap();
        }
        db.execute("INSERT INTO session_mapping(source,session_id,canonical_path,updated_at_ms) VALUES('antigravity','s',?1,20)", [&mixed]).unwrap();
        for (path, key, input, updated) in [
            (native, "shared", 1, 10),
            (mixed.as_str(), "shared", 3, 20),
            (mixed.as_str(), "new", 5, 20),
        ] {
            db.execute("INSERT INTO usage_meter_event(contribution,event_key,source,model,at_ms,input,output,cache_read,cache_write,reasoning,updated_at_ms) VALUES(?1,?2,'antigravity','m',1,?3,0,0,0,0,?4)", params![path,key,input,updated]).unwrap();
        }
        db.execute("INSERT INTO usage_meter_bucket(contribution,source,model,at_ms,input,output,cache_read,cache_write,reasoning,updated_at_ms) VALUES(?1,'antigravity','m',0,1,0,0,0,0,10),(?2,'antigravity','m',0,8,0,0,0,0,20)", params![native,mixed]).unwrap();
        db.execute(
            "INSERT INTO usage_meter_mark(contribution,at_ms,kind) VALUES(?1,1,0),(?2,1,0)",
            params![native, mixed],
        )
        .unwrap();
        migrate(&db).unwrap();
        let row: (String, String) = db
            .query_row("SELECT path,payload FROM ingestion_logs", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&row.1).unwrap();
        assert_eq!(row.0, native);
        assert_eq!(payload["path"], native);
        assert_eq!(payload["origin_path"], native);
        assert_eq!(payload["offset"], 20);
        assert_eq!(
            db.query_row("SELECT canonical_path FROM session_mapping", [], |r| r
                .get::<_, String>(
                0
            ))
            .unwrap(),
            native
        );
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM usage_meter_event WHERE contribution=?1",
                [native],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        assert_eq!(
            db.query_row(
                "SELECT input FROM usage_meter_event WHERE contribution=?1 AND event_key='shared'",
                [native],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            3
        );
        assert_eq!(
            db.query_row(
                "SELECT input FROM usage_meter_bucket WHERE contribution=?1",
                [native],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            8
        );
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM usage_meter_mark WHERE contribution=?1",
                [native],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        drop(db);
        let db = Connection::open(temp.path().join("old.sqlite")).unwrap();
        migrate(&db).unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM ingestion_logs", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM usage_meter_event", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
}
