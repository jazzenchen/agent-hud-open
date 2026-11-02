use super::*;
use crate::SourceRoot;
use rusqlite::{params, Connection};

fn core(root: &Path) -> AgentHudCore {
    AgentHudCore::new(
        vec![SourceRoot {
            path: root.join("opencode.db"),
            provider: "opencode",
        }],
        root.join("hud"),
    )
}

fn source(root: &Path) -> Connection {
    let db = Connection::open(root.join("opencode.db")).unwrap();
    db.execute_batch(
        "CREATE TABLE session(id TEXT PRIMARY KEY,title TEXT,directory TEXT,time_archived INTEGER,parent_id TEXT);
         CREATE TABLE message(id TEXT PRIMARY KEY,session_id TEXT,data TEXT);
         INSERT INTO session VALUES('s','old title','/repo',NULL,NULL);",
    )
    .unwrap();
    db
}

fn message(input: u64, output: u64) -> String {
    json!({
        "role":"assistant",
        "time":{"created":1_792_470_000_000i64,"completed":1_792_470_001_000i64},
        "modelID":"gpt-5","providerID":"openai",
        "tokens":{"input":input,"output":output}
    })
    .to_string()
}

fn finish(core: &mut AgentHudCore, first: Value) -> Value {
    let mut snapshot = first;
    for _ in 0..10 {
        if core.reads.pending.is_empty() {
            return snapshot;
        }
        snapshot = core.request("snapshot", json!({})).unwrap();
    }
    panic!("OpenCode fixture did not finish indexing");
}

fn refresh(core: &mut AgentHudCore) -> Value {
    let first = core.request("refresh", json!({})).unwrap();
    finish(core, first)
}

#[test]
fn opencode_reloads_updated_messages_and_session_metadata() {
    let root = tempfile::tempdir().unwrap();
    let db = source(root.path());
    db.execute("INSERT INTO message VALUES('m','s',?1)", [message(10, 1)])
        .unwrap();
    let mut backend = core(root.path());
    assert_eq!(refresh(&mut backend)["totals"]["total"], 11);
    drop(backend);

    // The persisted cursor must also reread an UPDATE after an app restart.
    db.execute(
        "UPDATE message SET data=?1 WHERE id='m'",
        [message(100, 20)],
    )
    .unwrap();
    db.execute("UPDATE session SET title='new title' WHERE id='s'", [])
        .unwrap();
    let mut backend = core(root.path());
    let snapshot = refresh(&mut backend);
    assert_eq!(snapshot["totals"]["total"], 120);
    let sessions = backend.request("sessions", json!({"scope":"all"})).unwrap();
    assert_eq!(sessions["items"][0]["title"], "new title");
    let detail = backend
        .request("session_detail", json!({"id":"opencode:s"}))
        .unwrap();
    assert_eq!(detail["usage"]["total"], 120);
    assert_eq!(detail["calls"].as_array().unwrap().len(), 1);

    db.execute("UPDATE session SET time_archived=1 WHERE id='s'", [])
        .unwrap();
    assert_eq!(refresh(&mut backend)["totals"]["total"], 120);
    let sessions = backend.request("sessions", json!({"scope":"all"})).unwrap();
    assert_eq!(sessions["total"], 0);
}

#[test]
fn opencode_new_table_replaces_existing_legacy_events_for_its_sessions() {
    let root = tempfile::tempdir().unwrap();
    let db = source(root.path());
    db.execute_batch("INSERT INTO session VALUES('other','other','/other',NULL,NULL)")
        .unwrap();
    db.execute("INSERT INTO message VALUES('old','s',?1)", [message(10, 1)])
        .unwrap();
    db.execute(
        "INSERT INTO message VALUES('other','other',?1)",
        [message(6, 1)],
    )
    .unwrap();
    let mut backend = core(root.path());
    assert_eq!(refresh(&mut backend)["totals"]["total"], 18);

    // The new table names the same reply with a different ID. It owns s only;
    // the other session still belongs to message.
    db.execute_batch(
        "CREATE TABLE session_message(id TEXT PRIMARY KEY,session_id TEXT,type TEXT,data TEXT)",
    )
    .unwrap();
    db.execute(
        "INSERT INTO session_message VALUES('new','s','assistant',?1)",
        [message(10, 1)],
    )
    .unwrap();
    assert_eq!(refresh(&mut backend)["totals"]["total"], 18);
    let detail = backend
        .request("session_detail", json!({"id":"opencode:s"}))
        .unwrap();
    assert_eq!(detail["usage"]["total"], 11);
    assert_eq!(detail["calls"].as_array().unwrap().len(), 1);

    db.execute(
        "UPDATE session_message SET data=?1 WHERE id='new'",
        [message(100, 20)],
    )
    .unwrap();
    assert_eq!(refresh(&mut backend)["totals"]["total"], 127);
}

#[test]
fn opencode_rebuilds_an_old_reader_checkpoint_without_a_source_change() {
    let root = tempfile::tempdir().unwrap();
    let db = source(root.path());
    db.execute_batch(
        "CREATE TABLE session_message(id TEXT PRIMARY KEY,session_id TEXT,type TEXT,data TEXT)",
    )
    .unwrap();
    db.execute("INSERT INTO message VALUES('old','s',?1)", [message(10, 1)])
        .unwrap();
    db.execute(
        "INSERT INTO session_message VALUES('new','s','assistant',?1)",
        [message(10, 1)],
    )
    .unwrap();
    let mut backend = core(root.path());
    assert_eq!(refresh(&mut backend)["totals"]["total"], 11);

    // Simulate the previous reader's completed checkpoint and duplicate ledger.
    // The source file itself stays untouched during the upgrade.
    let path = root.path().join("opencode.db");
    let vpath = virtual_path(&path, "opencode:s");
    let (mut physical, _) = backend.load_parser(&path).unwrap();
    let (mut session, _) = backend.load_parser(&vpath).unwrap();
    physical.reader_version = 1;
    physical.source_state =
        json!({"generation":"old-reader","cursor":null,"watermark":{"stage":0,"s0":1,"s1":1}});
    session.source_state["generation"] = json!("old-reader");
    session.count_source(
        "opencode:old".into(),
        "gpt-5#openai",
        1_792_470_000_000,
        Usage {
            input: 10,
            output: 1,
            ..Usage::default()
        },
        false,
    );
    let updated = backend.next_timestamp_ms();
    let ledger = backend.database.as_mut().unwrap();
    let transaction = ledger.transaction().unwrap();
    for log in [&physical, &session] {
        let path = log.path.to_string_lossy();
        checkpoints::upsert_log(
            &transaction,
            &path,
            &serde_json::to_string(log).unwrap(),
            &serde_json::to_string(&LogHead::from_log(log)).unwrap(),
            updated,
        )
        .unwrap();
        metering::write(
            &transaction,
            &path,
            "opencode",
            &log.meter_events.values().cloned().collect::<Vec<_>>(),
        )
        .unwrap();
    }
    transaction.commit().unwrap();
    drop(backend);

    let mut backend = core(root.path());
    assert_eq!(refresh(&mut backend)["totals"]["total"], 11);
    let detail = backend
        .request("session_detail", json!({"id":"opencode:s"}))
        .unwrap();
    assert_eq!(detail["callsTotal"], 1);
    assert_eq!(backend.logs[&path].reader_version, 2);
}

#[test]
fn opencode_generation_paging_resumes_and_unchanged_snapshots_do_no_source_writes() {
    let root = tempfile::tempdir().unwrap();
    let mut db = source(root.path());
    let transaction = db.transaction().unwrap();
    for index in 0..1001 {
        transaction
            .execute(
                "INSERT INTO message VALUES(?1,'s',?2)",
                params![format!("m{index}"), message(1, 1)],
            )
            .unwrap();
    }
    transaction.commit().unwrap();
    let mut backend = core(root.path());
    let first = backend.request("snapshot", json!({})).unwrap();
    assert_eq!(first["totals"]["total"], 2000);
    assert_eq!(backend.reads.pending.len(), 1);
    drop(backend);

    let mut backend = core(root.path());
    assert_eq!(refresh(&mut backend)["totals"]["total"], 2002);
    db.execute("UPDATE message SET data=?1 WHERE id='m0'", [message(3, 1)])
        .unwrap();
    assert_eq!(refresh(&mut backend)["totals"]["total"], 2004);
    let committed = backend.last_synced_at_ms;
    assert_eq!(
        backend.request("snapshot", json!({})).unwrap()["totals"]["total"],
        2004
    );
    assert_eq!(backend.last_synced_at_ms, committed);
}

/// Each call's (output, duration) in the ledger, by output.
fn timed_calls(backend: &AgentHudCore) -> Vec<(i64, Option<i64>)> {
    let db = backend.database.as_ref().unwrap();
    let mut statement = db
        .prepare("SELECT output,duration_ms FROM usage_meter_event ORDER BY output")
        .unwrap();
    let rows = statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    rows.map(Result::unwrap).collect()
}

#[test]
fn opencode_times_a_reply_only_when_it_called_no_tools() {
    let root = tempfile::tempdir().unwrap();
    let db = source(root.path());
    let created = chrono::Utc::now().timestamp_millis() - 60_000;
    let reply = |finish: &str, output: u64| {
        json!({
            "role":"assistant",
            "time":{"created":created,"completed":created + 4_000},
            "finish":finish,
            "modelID":"gpt-5","providerID":"openai",
            "tokens":{"input":10,"output":output}
        })
        .to_string()
    };
    db.execute(
        "INSERT INTO message VALUES('a','s',?1),('b','s',?2)",
        [reply("stop", 100), reply("tool-calls", 50)],
    )
    .unwrap();
    let mut backend = core(root.path());
    refresh(&mut backend);
    assert_eq!(timed_calls(&backend), [(50, None), (100, Some(4_000))]);
}

#[test]
fn grok_inference_records_keep_the_model_calls_own_time() {
    let root = tempfile::tempdir().unwrap();
    let logs = root.path().join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    let at = |offset: i64| {
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
            chrono::Utc::now().timestamp_millis() - 60_000 + offset,
        )
        .unwrap()
        .to_rfc3339()
    };
    let lines = [
        json!({"msg":"shell.turn.inference_done","sid":"g1","ts":at(0),"event_id":"e1","ctx":{"prompt_tokens":1000,"completion_tokens":200,"model_elapsed_ms":4734,"elapsed_since_turn_start_ms":13684}}),
        json!({"msg":"shell.turn.inference_done","sid":"g1","ts":at(9_000),"event_id":"e2","ctx":{"prompt_tokens":1200,"completion_tokens":300}}),
    ];
    std::fs::write(
        logs.join("unified.jsonl"),
        lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let mut backend = AgentHudCore::new(
        vec![SourceRoot {
            path: logs,
            provider: "grok-cli",
        }],
        root.path().join("hud"),
    );
    refresh(&mut backend);
    assert_eq!(timed_calls(&backend), [(200, Some(4_734)), (300, None)]);
}

#[test]
fn copilot_chat_spans_keep_their_own_time() {
    let root = tempfile::tempdir().unwrap();
    let otel = root.path().join("otel");
    std::fs::create_dir_all(&otel).unwrap();
    let start = chrono::Utc::now().timestamp() - 60;
    let span = |id: &str, end: Option<[i64; 2]>, output: u64| {
        let mut span = json!({
            "type":"span","name":"chat gpt-5","traceId":"t1","spanId":id,
            "startTime":[start,0],
            "attributes":{
                "gen_ai.conversation.id":"c1","gen_ai.operation.name":"chat",
                "gen_ai.response.model":"gpt-5",
                "gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":output
            }
        });
        if let Some(end) = end {
            span["endTime"] = json!(end);
        }
        span
    };
    let lines = [
        span("a", Some([start + 3, 250_000_000]), 200),
        span("b", None, 300),
    ];
    std::fs::write(
        otel.join("spans.jsonl"),
        lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let mut backend = AgentHudCore::new(
        vec![SourceRoot {
            path: otel,
            provider: "copilot",
        }],
        root.path().join("hud"),
    );
    refresh(&mut backend);
    assert_eq!(timed_calls(&backend), [(200, Some(3_250)), (300, None)]);
}

#[test]
fn cursor_sessions_come_from_its_database_and_their_tokens_from_the_turn_log() {
    use agent_hud_agents::hooks::turn_log;
    use std::fs;
    let root = tempfile::tempdir().unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let state = root.path().join("state.vscdb");
    let db = Connection::open(&state).unwrap();
    db.execute_batch("CREATE TABLE composerHeaders(composerId TEXT PRIMARY KEY, value TEXT)")
        .unwrap();
    let header = |name: &str, at: i64, archived: bool| {
        json!({
            "name": name,
            "createdAt": at - 60_000,
            "lastUpdatedAt": at - 60_000,
            "conversationCheckpointLastUpdatedAt": at,
            "isArchived": archived,
            "workspaceIdentifier": {"uri": {"fsPath": "/repo"}},
        })
        .to_string()
    };
    db.execute(
        "INSERT INTO composerHeaders VALUES('c1', ?1), ('c2', ?2)",
        [
            header("Code quality review", now - 1_000, false),
            header("Old try", now - 5_000, true),
        ],
    )
    .unwrap();
    let folder = root.path().join("clients/cursor");
    fs::create_dir_all(&folder).unwrap();
    let turn = |session: &str, id: &str, input: u64| {
        turn_log::line(&agent_hud_agents::HookEvent {
            source: "cursor".into(),
            session_id: session.into(),
            at_ms: now - 2_000,
            kind: "completion".into(),
            data: json!({"turnId": id, "model": "grok-4.7", "workspace": "/repo",
                         "usage": {"input": input, "output": 10, "read": 600, "write": 0, "reasoning": 0}}),
        })
        .unwrap()
    };
    let log = folder.join(turn_log::file_name(now).unwrap());
    fs::write(
        &log,
        turn("cursor:c1", "g1", 100) + &turn("cursor:c2", "g2", 50),
    )
    .unwrap();
    let mut backend = AgentHudCore::new(
        vec![
            SourceRoot {
                path: state.clone(),
                provider: "cursor",
            },
            SourceRoot {
                path: folder.clone(),
                provider: "cursor",
            },
        ],
        root.path().join("hud"),
    );
    // Both turns count, the archived session's too.
    assert_eq!(refresh(&mut backend)["totals"]["total"], 710 + 660);
    let sessions = backend.request("sessions", json!({"scope":"all"})).unwrap();
    assert_eq!(sessions["total"], 1, "the archived session is not listed");
    let listed = &sessions["items"][0];
    assert_eq!(listed["id"], "cursor:c1");
    assert_eq!(listed["title"], "Code quality review");
    assert_eq!(listed["tokens"], 710);

    // A later turn is read from where the log stopped, onto the same session.
    let mut file = fs::OpenOptions::new().append(true).open(&log).unwrap();
    std::io::Write::write_all(&mut file, turn("cursor:c1", "g3", 200).as_bytes()).unwrap();
    drop(file);
    assert_eq!(refresh(&mut backend)["totals"]["total"], 710 + 660 + 810);
    let detail = backend
        .request("session_detail", json!({"id":"cursor:c1"}))
        .unwrap();
    assert_eq!(detail["usage"]["total"], 710 + 810);
    assert_eq!(detail["calls"].as_array().unwrap().len(), 2);
}

#[test]
fn a_completion_for_an_archived_session_puts_no_placeholder_in_its_place() {
    use std::fs;
    let root = tempfile::tempdir().unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let state = root.path().join("state.vscdb");
    let db = Connection::open(&state).unwrap();
    db.execute_batch("CREATE TABLE composerHeaders(composerId TEXT PRIMARY KEY, value TEXT)")
        .unwrap();
    db.execute(
        "INSERT INTO composerHeaders VALUES('kept', ?1)",
        [
            json!({"name": "Code quality review", "createdAt": now - 60_000,
                "conversationCheckpointLastUpdatedAt": now - 2_000, "isArchived": true})
            .to_string(),
        ],
    )
    .unwrap();
    fs::create_dir_all(root.path().join("clients/cursor")).unwrap();
    let mut backend = AgentHudCore::new(
        vec![
            SourceRoot {
                path: state,
                provider: "cursor",
            },
            SourceRoot {
                path: root.path().join("clients/cursor"),
                provider: "cursor",
            },
        ],
        root.path().join("hud"),
    );
    refresh(&mut backend);
    let completion = |session: &str| agent_hud_agents::HookEvent {
        source: "cursor".into(),
        session_id: session.into(),
        at_ms: now - 1_000,
        kind: "completion".into(),
        data: json!({"turnId": format!("{session}-g1"), "model": "grok-4.7", "workspace": "/repo"}),
    };
    backend.ingest_hook_events(vec![
        completion("cursor:kept"),
        completion("cursor:unknown"),
    ]);
    let snapshot = backend.request("snapshot", json!({})).unwrap();
    let placeholders = |sessions: &Value| {
        sessions
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["evidence"] == "completion-hook")
            .map(|s| s["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    // The archived session stays hidden; a session no log knows still shows.
    assert_eq!(placeholders(&snapshot["sessions"]), ["cursor:unknown"]);
    let listed = backend.request("sessions", json!({"scope":"all"})).unwrap();
    assert_eq!(placeholders(&listed["items"]), ["cursor:unknown"]);
    assert!(backend
        .request("session_detail", json!({"id":"cursor:kept"}))
        .is_err());
}

#[test]
fn grok_bots_daily_tokens_land_on_the_rosters_own_bot_session() {
    use crate::grok_cache as bot;
    use std::fs;
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("sand-client-persistence");
    fs::create_dir_all(&cache).unwrap();
    let write = |key: &str, schema: u64, value: Value| {
        fs::write(
            bot::path(&cache, key),
            format!(
                "{}\n{}",
                serde_json::to_string(key).unwrap(),
                json!({"schemaVersion": schema, "value": value})
            ),
        )
        .unwrap();
    };
    let now = chrono::Utc::now().timestamp_millis();
    write(bot::ACCOUNT_KEY, 1, json!("slot-1"));
    write(
        &bot::roster_key("slot-1"),
        4,
        json!({"rows": [{"id": "bot-a", "title": "SEO & AEO Desk", "lastActivityAt": now - 1_000}]}),
    );
    let mut backend = AgentHudCore::new(
        vec![SourceRoot {
            path: cache,
            provider: "grok-bot",
        }],
        root.path().join("hud"),
    );
    refresh(&mut backend);
    let id = bot::session_id("slot-1", "bot-a");
    let day = (now / 86_400_000) * 86_400_000;
    let record = |tokens: u64| {
        json!({"source": "grok-bot", "id": id, "title": null, "model": "grok-bot",
               "key": format!("spend-bot-a-{}", day / 1000), "at": day,
               "input": tokens, "output": 0, "read": 0, "write": 0})
    };
    backend.set_account_projection(crate::accounts::AccountProjection {
        session_records: vec![record(4_000)],
        ..Default::default()
    });
    // The day's total grows: the same key replaces it.
    backend.set_account_projection(crate::accounts::AccountProjection {
        session_records: vec![record(9_000)],
        ..Default::default()
    });
    refresh(&mut backend);
    let sessions = backend
        .request("sessions", json!({"scope":"all","sources":["grok-bot"]}))
        .unwrap();
    assert_eq!(sessions["total"], 1, "one session per bot");
    assert_eq!(sessions["items"][0]["id"], id);
    assert_eq!(sessions["items"][0]["title"], "SEO & AEO Desk");
    assert_eq!(sessions["items"][0]["tokens"], 9_000);
}

#[test]
fn grok_bot_session_works_while_a_message_has_no_reply_and_idles_once_answered() {
    use crate::grok_cache as bot;
    use std::fs;
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("sand-client-persistence");
    fs::create_dir_all(&cache).unwrap();
    let write = |key: &str, schema: u64, value: Value| {
        fs::write(
            bot::path(&cache, key),
            format!(
                "{}\n{}",
                serde_json::to_string(key).unwrap(),
                json!({"schemaVersion": schema, "value": value})
            ),
        )
        .unwrap();
    };
    let now = chrono::Utc::now().timestamp_millis();
    let user = json!({"kind":"message","role":"user","id":"t1u","requestId":"r1","timestampMs":now - 700_000,"isStreaming":false});
    let reply =
        json!({"kind":"send-message","id":"t1s1","requestId":"r1","timestampMs":now - 600_000});
    write(bot::ACCOUNT_KEY, 1, json!("slot-1"));
    write(
        &bot::transcript_key("slot-1", "bot-a"),
        1,
        json!({"entries": [user.clone()]}),
    );
    let roster = |at: i64| {
        write(
            &bot::roster_key("slot-1"),
            4,
            json!({"rows": [{"id": "bot-a", "title": "SEO & AEO Desk", "lastActivityAt": at}]}),
        )
    };
    roster(now - 5_000);
    let mut backend = AgentHudCore::new(
        vec![SourceRoot {
            path: cache.clone(),
            provider: "grok-bot",
        }],
        root.path().join("hud"),
    );
    let state = |backend: &mut AgentHudCore| {
        refresh(backend);
        let sessions = backend
            .request("sessions", json!({"scope":"all","sources":["grok-bot"]}))
            .unwrap();
        sessions["items"][0]["state"].as_str().unwrap().to_owned()
    };
    assert_eq!(state(&mut backend), "running");
    // While it works the roster is looked at again, every 10 seconds.
    assert!(backend.reads.bot_poll.is_some());
    assert!(!backend.look());
    backend.reads.bot_poll.as_mut().unwrap().0 = 0;
    assert!(backend.look());
    assert!(backend.reads.bot_poll.is_none());
    write(
        &bot::transcript_key("slot-1", "bot-a"),
        1,
        json!({"entries": [user, reply]}),
    );
    roster(now - 600_000);
    assert_eq!(state(&mut backend), "idle");
    assert!(backend.reads.bot_poll.is_none());
}
