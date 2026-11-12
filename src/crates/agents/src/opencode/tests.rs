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
    assert_eq!(ids, ["opencode", "kilo", "mimocode", "teleagent"]);
    for client in STORES {
        assert!(std::ptr::eq(store(client.agent.id).unwrap(), client));
        // TeleAgent uses the same database and does not load the OpenCode plugin.
        let Some(observer) = client.agent.observer else {
            assert_eq!(client.agent.id, "teleagent");
            continue;
        };
        // The plugin is OpenCode's, written for this client's ids and folder
        // and exported the way the client loads it.
        assert!(observer.body.starts_with(observer.marker));
        let export = match client.agent.id {
            "opencode" => include_str!("export.js"),
            _ => "export const AgentHUDSessionObserver = server;\n",
        };
        assert_eq!(
            observer.body,
            format!(
                "{}const SOURCE = \"{id}\", FOLDER = \"{id}\";\nconst CHANNEL = /*agent-hud:channel*/ null;\n{}{}{export}",
                observer.marker,
                include_str!("../hooks/mailbox.js"),
                include_str!("observer.js"),
                id = client.agent.id
            ),
            "{}",
            client.agent.id
        );
        assert_eq!(observer.at_key, "completedAtMs");
        assert!(!observer.states);
    }
    assert!(!include_str!("observer.js").contains("\"opencode"));
    assert!(store("pi").is_none());
}

#[test]
fn opencode_1_and_2_load_the_same_file_and_the_forks_load_it_as_before() {
    // Open #46: OpenCode 2 refused the plugin, which exported only OpenCode
    // 1's function. Both versions load every script in `plugin/` and
    // `plugins/`, so the one script exports the plugin of the version that
    // loads it: OpenCode 1 calls a function, and an object stopped OpenCode
    // before 1.2.11; OpenCode 2 needs an object with an id and `setup`, and
    // only it gives its plugins `@opencode/plugin`.
    let exports = |body: &str| {
        body.lines()
            .filter(|line| line.starts_with("export "))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert!(exports(include_str!("observer.js")).is_empty());
    assert_eq!(exports(OBSERVER.body), ["export { plugin as default };"]);
    let export = include_str!("export.js");
    for line in [
        "let plugin = server;",
        "  await import(\"@opencode/plugin\");",
        "  plugin = { id: \"agent-hud\", setup, server };",
    ] {
        assert!(export.lines().any(|l| l == line), "{line}");
    }
    // Where earlier releases wrote it, so their script is replaced rather
    // than left behind for OpenCode 2 to refuse.
    assert_eq!(OBSERVER.script.path, "plugin/agent-hud.js");
    // Kilo Code and MiMo Code load OpenCode 1's plugin, named as before.
    for fork in [&KILO_OBSERVER, &MIMO_CODE_OBSERVER] {
        assert_eq!(
            exports(fork.body),
            ["export const AgentHUDSessionObserver = server;"]
        );
    }
}

#[test]
fn a_turn_recorded_under_opencode_2_is_a_completed_turn_without_a_terminal() {
    // OpenCode 2's plugin runs in its background service, which belongs to no
    // terminal, so its records carry no navigation target.
    let record = json!({
        "version": 1, "sessionID": "opencode:ses_e5415bdccffe", "workspace": "/repo", "title": "Fix the login form",
        "model": "mock-1", "providerID": "mock", "turnID": "msg_1abea442f001",
        "startedAtMs": 1_793_885_619_559i64, "completedAtMs": 1_793_885_619_686i64,
    });
    let event = crate::parse_observation("opencode", &record).unwrap();
    assert_eq!(event.session_id, "opencode:ses_e5415bdccffe");
    assert_eq!(event.data["turnId"], "msg_1abea442f001");
    assert_eq!(event.data["state"], "completed");
    assert_eq!(event.data["outcome"], "completed");
    assert_eq!(event.data["workspace"], "/repo");
    assert_eq!(event.data["navigationTarget"], Value::Null);
}

#[test]
fn a_turn_that_took_a_message_names_it_and_carries_its_reply() {
    let record = json!({
        "version": 1, "sessionID": "opencode:ses_e5415bdccffe", "workspace": "/repo", "turnID": "msg_1",
        "startedAtMs": 1_793_885_619_559i64, "completedAtMs": 1_793_885_619_686i64,
        "delivered": "m-d8776a7228b7", "message": "Done: OC_SENT",
    });
    let event = crate::parse_observation("opencode", &record).unwrap();
    assert_eq!(event.data["delivered"], "m-d8776a7228b7");
    assert_eq!(event.data["message"], "Done: OC_SENT");
    // A message id that is not one, and an empty reply, are left out.
    let record = json!({
        "version": 1, "sessionID": "opencode:ses_e5415bdccffe", "workspace": "/repo", "turnID": "msg_1",
        "startedAtMs": 1_793_885_619_559i64, "completedAtMs": 1_793_885_619_686i64,
        "delivered": "../m", "message": "  ",
    });
    let event = crate::parse_observation("opencode", &record).unwrap();
    assert!(event.data.get("delivered").is_none());
    assert!(event.data.get("message").is_none());
    // One that failed ends without an answer, so its sender learns it.
    let record = json!({
        "version": 1, "sessionID": "opencode:ses_e5415bdccffe", "workspace": "/repo", "turnID": "msg_1",
        "startedAtMs": 1_793_885_619_559i64, "completedAtMs": 1_793_885_619_686i64,
        "delivered": "m-d8776a7228b7", "state": "ended", "stopReason": "error",
    });
    let event = crate::parse_observation("opencode", &record).unwrap();
    assert_eq!(event.data["state"], "ended");
    assert_eq!(event.data["outcome"], "failed");
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

#[test]
fn teleagent_reads_each_account_and_its_cached_tokens() {
    use crate::{Base, Location, Places, Profiles};
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    struct Disk(HashSet<PathBuf>);
    impl Places for Disk {
        fn exists(&self, path: &Path) -> bool {
            self.0.contains(path)
        }
        fn same(&self, a: &Path, b: &Path) -> bool {
            a == b
        }
        fn names(&self, folder: &Path) -> Vec<String> {
            self.0
                .iter()
                .filter(|path| path.parent() == Some(folder))
                .filter_map(|path| path.file_name()?.to_str().map(str::to_owned))
                .collect()
        }
    }
    let place = Location {
        base: Base::DataHome,
        env: &[],
        folders: &["TeleAgent"],
        profiles: Some(Profiles::Inside("users")),
        path: "teleagent.db",
        not_if: None,
    };
    let share = tempfile::tempdir().unwrap();
    let account = share
        .path()
        .join("TeleAgent")
        .join("users")
        .join("v1_public_1903017520943529984");
    let skipped = share
        .path()
        .join("TeleAgent")
        .join("users")
        .join("not a profile");
    let disk = Disk(
        [
            account.clone(),
            account.join("teleagent.db"),
            skipped.clone(),
            skipped.join("teleagent.db"),
        ]
        .into_iter()
        .collect(),
    );
    let found = place.resolve(share.path(), &|_| None, &disk);
    assert!(found
        .iter()
        .any(|path| path == &account.join("teleagent.db")));
    assert!(found
        .iter()
        .all(|path| !path.to_string_lossy().contains("not a profile")));

    let db = database(
        "CREATE TABLE session(id TEXT PRIMARY KEY,title TEXT,directory TEXT,time_archived INTEGER,parent_id TEXT,workspace_id TEXT);
         CREATE TABLE message(id TEXT PRIMARY KEY,session_id TEXT,time_created INTEGER,data TEXT);
         INSERT INTO session VALUES('ses_1','调研报告','/work/report',NULL,NULL,'ws');",
    );
    db.execute(
        "INSERT INTO message VALUES('msg_1','ses_1',1,?1)",
        [json!({
            "role":"assistant",
            "finish":"stop",
            "time":{"created":1_794_389_084_772i64,"completed":1_794_389_090_000i64},
            "modelID":"chat-pro",
            "providerID":"NewApi",
            "tokens":{"input":12,"output":3,"reasoning":1,"cache":{"read":4,"write":2}},
            "cost":0
        })
        .to_string()],
    )
    .unwrap();
    let read = records(&db, &TELEAGENT_STORE);
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].id, "teleagent:ses_1");
    assert_eq!(read[0].key, "teleagent:msg_1");
    assert_eq!(read[0].title.as_deref(), Some("调研报告"));
    assert_eq!(read[0].cwd.as_deref(), Some("/work/report"));
    assert_eq!(read[0].model, "chat-pro#NewApi");
    assert_eq!(
        (
            read[0].usage.input,
            read[0].usage.output,
            read[0].usage.reasoning,
            read[0].usage.read,
            read[0].usage.write
        ),
        (12, 3, 1, 4, 2)
    );
}
