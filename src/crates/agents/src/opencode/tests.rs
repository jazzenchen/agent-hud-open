use super::*;
use rusqlite::Connection;
use serde_json::{json, Value};

fn database(sql: &str) -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(sql).unwrap();
    db
}

/// Every record a whole pass reads, page after page.
fn records(db: &Connection, store: &Store) -> Vec<crate::transcript::records::Record> {
    let mut cursor = Value::Null;
    let mut all = Vec::new();
    loop {
        let page = transcript::read(db, &cursor, store).unwrap();
        all.extend(page.records);
        match page.cursor {
            Some(next) => cursor = next,
            None => return all,
        }
    }
}

#[test]
fn every_client_of_the_database_has_its_store_and_its_own_plugin() {
    let ids = STORES.iter().map(|s| s.agent.id).collect::<Vec<_>>();
    assert_eq!(ids, ["opencode", "kilo", "mimocode"]);
    for client in STORES {
        assert!(std::ptr::eq(store(client.agent.id).unwrap(), client));
        let observer = client.agent.observer.unwrap();
        // The plugin is OpenCode's, written for this client's ids and folder.
        assert!(observer.body.starts_with(observer.marker));
        let prelude = format!(
            "{}const SOURCE = \"{id}\", FOLDER = \"{id}\";\n",
            observer.marker,
            id = client.agent.id
        );
        assert!(observer.body.starts_with(&prelude), "{}", client.agent.id);
        assert!(observer.body.ends_with(include_str!("observer.js")));
        assert_eq!(observer.at_key, "completedAtMs");
        assert!(!observer.states);
    }
    assert!(!include_str!("observer.js").contains("\"opencode"));
    assert!(store("pi").is_none());
}

#[test]
fn opencode_reads_as_it_did() {
    let db = database(
        "CREATE TABLE session(id TEXT PRIMARY KEY,title TEXT,directory TEXT,time_archived INTEGER,parent_id TEXT);
         CREATE TABLE message(id TEXT PRIMARY KEY,session_id TEXT,data TEXT);
         INSERT INTO session VALUES('s','New session - 2026-10-20T10:00:00.000Z','/repo',NULL,NULL);",
    );
    db.execute(
        "INSERT INTO message VALUES('m','s',?1)",
        [json!({"role":"assistant","time":{"created":1_792_470_000_000i64},"modelID":"gpt-5","providerID":"openai","tokens":{"input":10,"output":1}}).to_string()],
    )
    .unwrap();
    let read = records(&db, &STORE);
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].id, "opencode:s");
    assert_eq!(read[0].key, "opencode:m");
    assert_eq!(
        read[0].title, None,
        "OpenCode's made-up title names nothing"
    );
}

#[test]
fn kilo_code_reads_its_replies_and_leaves_out_moved_tasks_and_broken_rows() {
    let db = database(include_str!("../../tests/fixtures/kilo/macos/kilo.sql"));
    let read = records(&db, &KILO_STORE);
    let keys = read.iter().map(|r| r.key.as_str()).collect::<Vec<_>>();
    // The task moved in from the VS Code extension, and the row that is not
    // JSON, are left out; the row did not stop the others being read.
    assert_eq!(keys, ["kilo:msg_2", "kilo:msg_3", "kilo:msg_4"]);
    let reply = &read[0];
    assert_eq!(reply.id, "kilo:ses_1");
    assert_eq!(reply.title.as_deref(), Some("Fix the login form"));
    assert_eq!(reply.cwd.as_deref(), Some("/Users/me/work/app"));
    assert_eq!(reply.model, "claude-sonnet-4#kilo");
    assert_eq!(
        (
            reply.usage.input,
            reply.usage.output,
            reply.usage.reasoning,
            reply.usage.read,
            reply.usage.write
        ),
        (1000, 200, 40, 500, 100)
    );
    assert_eq!(reply.duration_ms, Some(29_000));
    assert_eq!(read[1].duration_ms, None, "a reply that called tools");
    let child = &read[2];
    assert_eq!(child.parent.as_deref(), Some("kilo:ses_1"));
    assert_eq!(child.title, None);
}

#[test]
fn mimo_code_leaves_out_what_it_imported_and_its_made_up_titles() {
    let db = database(include_str!(
        "../../tests/fixtures/mimocode/macos/mimocode.sql"
    ));
    let read = records(&db, &MIMO_CODE_STORE);
    let keys = read.iter().map(|r| r.key.as_str()).collect::<Vec<_>>();
    // The imported Claude Code reply is read from Claude Code's own log; the
    // reply MiMo Code wrote after it is its own. An import that named no
    // messages leaves its whole session out.
    assert_eq!(
        keys,
        ["mimocode:msg_own", "mimocode:msg_sub", "mimocode:msg_cc_2"]
    );
    assert_eq!(read[0].id, "mimocode:ses_own");
    assert_eq!(read[0].title, None, "\"Untitled\" names nothing");
    // A subagent's replies are kept in its session.
    assert_eq!(read[1].id, "mimocode:ses_own");
    assert_eq!(read[2].title.as_deref(), Some("Imported from Claude Code"));
    assert_eq!(read[0].usage.total(), 950);
}

#[test]
fn an_import_table_from_before_message_ids_leaves_its_sessions_out() {
    let db = database(
        "CREATE TABLE session(id TEXT PRIMARY KEY,title TEXT,directory TEXT,time_archived INTEGER,parent_id TEXT);
         CREATE TABLE message(id TEXT PRIMARY KEY,session_id TEXT,data TEXT);
         CREATE TABLE claude_import(source_uuid TEXT PRIMARY KEY,session_id TEXT,source_path TEXT);
         INSERT INTO claude_import VALUES('u','s_cc','x');",
    );
    for (id, session) in [("m1", "s_cc"), ("m2", "s_own")] {
        db.execute(
            "INSERT INTO message VALUES(?1,?2,?3)",
            [id.to_owned(), session.to_owned(), json!({"role":"assistant","time":{"created":1_792_470_000_000i64},"tokens":{"input":5,"output":1}}).to_string()],
        )
        .unwrap();
    }
    let read = records(&db, &MIMO_CODE_STORE);
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].id, "mimocode:s_own");
    // OpenCode names no import tables: the same rows are all its own.
    assert_eq!(records(&db, &STORE).len(), 2);
}
