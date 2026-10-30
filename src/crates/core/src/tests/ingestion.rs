//! Reading logs: cumulative and streamed usage, oversized records, old and
//! unreadable logs, batch updates and what a run keeps in memory.
use super::*;

#[cfg(windows)]
#[path = "ingestion_windows.rs"]
mod windows;

#[test]
fn codex_cumulative_repeats_and_counter_resets_follow_source_calls() {
    let mut log = Log::new("session.jsonl".into(), "codex");
    let row = |input, output, cache| json!({"timestamp":"2026-10-19T00:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":input,"output_tokens":output,"cached_input_tokens":cache,"reasoning_output_tokens":2}}}});
    log.parse(&row(100, 20, 30));
    log.parse(&row(100, 20, 30));
    log.parse(&row(180, 40, 50));
    log.parse(&row(100, 20, 30));
    assert_eq!(log.usage.total(), 340);
    assert_eq!(log.usage.input, 200);
    assert_eq!(log.usage.read, 80);
    assert_eq!(log.turns.len(), 3);
}

#[test]
fn a_codex_call_carries_its_provider_as_its_route_unless_it_is_openai() {
    let meta = |provider: Option<&str>| {
        let mut payload = json!({"id": "s1", "cwd": "/w"});
        if let Some(provider) = provider {
            payload["model_provider"] = json!(provider);
        }
        json!({"timestamp": "2026-10-19T00:00:00Z", "type": "session_meta", "payload": payload})
    };
    let context = json!({"timestamp":"2026-10-19T00:00:01Z","type":"turn_context","payload":{"model":"gpt-6-sol"}});
    let count = json!({"timestamp":"2026-10-19T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"output_tokens":20}}}});
    for (provider, model) in [
        (None, "gpt-6-sol"),
        (Some("openai"), "gpt-6-sol"),
        (Some("myrelay"), "gpt-6-sol#myrelay"),
        (Some("ollama"), "gpt-6-sol#ollama"),
    ] {
        let mut log = Log::new("session.jsonl".into(), "codex");
        assert!(!log.codex_provider_known());
        log.parse(&meta(provider));
        assert!(log.codex_provider_known());
        log.parse(&context);
        log.parse(&count);
        let models: Vec<_> = log.meter_events.values().map(|e| e.model.clone()).collect();
        assert_eq!(models, [model], "{provider:?}");
    }
    // A forked rollout's copied parent meta does not change its provider.
    let mut log = Log::new("session.jsonl".into(), "codex");
    log.parse(&meta(Some("myrelay")));
    let mut parent = meta(Some("openai"));
    parent["payload"]["id"] = json!("parent");
    log.note_codex_provider(&parent);
    log.parse(&context);
    log.parse(&count);
    assert_eq!(
        log.meter_events.values().next().unwrap().model,
        "gpt-6-sol#myrelay"
    );
}

#[test]
fn a_claude_call_is_recorded_under_the_route_its_session_reported() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    crate::database::schema(&db).unwrap();
    let event = |model: &str| agent_hud_agents::transcript::metering::Event {
        key: model.into(),
        model: model.into(),
        ..Default::default()
    };
    let routed = |session: &str, endpoint: Option<serde_json::Value>| {
        if let Some(endpoint) = endpoint {
            crate::database::endpoints::record(&db, "claude", session, &endpoint, 1).unwrap();
        }
        let mut log = Log::new("session.jsonl".into(), "claude");
        log.id = session.into();
        let mut events = vec![event("claude-sonnet-4-5"), event("qwen3-coder:30b")];
        crate::ingestion::route_claude_calls(&db, &log, &mut events).unwrap();
        events
            .into_iter()
            .map(|event| (event.model, event.context_window))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        routed("none", None),
        [
            ("claude-sonnet-4-5".into(), None),
            ("qwen3-coder:30b".into(), None)
        ],
        "no report: as before"
    );
    assert_eq!(
        routed("anthropic", Some(json!({"host": null, "platform": null})))[0].0,
        "claude-sonnet-4-5"
    );
    assert_eq!(
        routed("glm", Some(json!({"host": "api.z.ai", "platform": null})))[0].0,
        "claude-sonnet-4-5",
        "a vendor's own API is priced by model name"
    );
    assert_eq!(
        routed(
            "relay",
            Some(json!({"host": "relay.example.com", "platform": null}))
        )[0]
        .0,
        "claude-sonnet-4-5#relay.example.com"
    );
    assert_eq!(
        routed(
            "bedrock",
            Some(json!({"host": null, "platform": "bedrock"}))
        )[0]
        .0,
        "claude-sonnet-4-5#bedrock"
    );
    assert_eq!(
        routed(
            "ollama",
            Some(
                json!({"host": "localhost:11434", "platform": null, "server": "ollama", "contextWindow": 32768})
            )
        ),
        [
            ("claude-sonnet-4-5#ollama".into(), Some(32_768)),
            ("qwen3-coder:30b#ollama".into(), Some(32_768))
        ]
    );
    assert_eq!(
        routed(
            "gateway",
            Some(json!({"host": "localhost:4000", "platform": null}))
        ),
        [
            ("claude-sonnet-4-5#localhost:4000".into(), None),
            ("qwen3-coder:30b#localhost:4000".into(), None)
        ],
        "no server found: the host, which pricing reads"
    );
}

#[test]
fn claude_streamed_message_snapshots_replace_the_same_usage() {
    let mut log = Log::new("session.jsonl".into(), "claude");
    let row = |output| json!({"type":"assistant","message":{"id":"m1","usage":{"input_tokens":10,"output_tokens":output,"cache_read_input_tokens":30,"cache_creation_input_tokens":5}}});
    log.parse(&row(2));
    log.parse(&row(8));
    log.parse(&row(8));
    assert_eq!(log.usage.total(), 53);
    assert_eq!(log.turns.len(), 1);
    assert_eq!(log.turns[0]["output"], 8);
}

#[test]
fn oversized_and_partial_records_are_bounded() {
    let mut data = vec![b'x'; MAX_LINE * 3];
    data.extend_from_slice(b"\n{}\npartial");
    let mut reader = BufReader::new(data.as_slice());
    let (bytes, consumed, complete) = line(&mut reader).unwrap();
    assert_eq!(bytes.len(), MAX_LINE + 1);
    assert_eq!(consumed, MAX_LINE as u64 * 3 + 1);
    assert!(complete);
    assert_eq!(line(&mut reader).unwrap().0, b"{}\n");
    assert!(!line(&mut reader).unwrap().2);
}

#[test]
fn logs_last_written_over_31_days_ago_are_not_read_and_keep_what_they_counted() {
    use std::time::{Duration, SystemTime};
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = home.join(".claude/projects/test");
    fs::create_dir_all(&root).unwrap();
    let age = |path: &Path, days: u64| {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(days * 86_400))
            .unwrap();
    };
    let line = |session: &str, id: &str, pad: usize| json!({"sessionId":session,"type":"assistant","message":{"id":id,"usage":{"input_tokens":10,"output_tokens":3}},"pad":"x".repeat(pad)});
    // Read once, while recent: its usage counts.
    let counted = root.join("counted.jsonl");
    fs::write(&counted, format!("{}\n", line("s1", "m1", 0))).unwrap();
    let mut backend = fixture(home);
    assert_eq!(
        backend.request("snapshot", json!({})).unwrap()["totals"]["total"],
        13
    );
    drop(backend);
    // An earlier reader version's state on a log last written 40 days
    // ago, never read, and one 20 days old too long for one pass.
    age(&counted, 40);
    let ledger = rusqlite::Connection::open(home.join("app/ingestion.sqlite")).unwrap();
    ledger
        .execute(
            "UPDATE ingestion_logs SET head=json_set(head,'$.parser_version',6),payload=json_set(payload,'$.parser_version',6)",
            [],
        )
        .unwrap();
    let old = root.join("old.jsonl");
    fs::write(&old, format!("{}\n", line("s2", "m2", 0))).unwrap();
    age(&old, 40);
    let unfinished = root.join("unfinished.jsonl");
    let lines = (0..4)
        .map(|n| line("s3", &format!("m3-{n}"), 900_000).to_string() + "\n")
        .collect::<String>();
    fs::write(&unfinished, lines).unwrap();
    age(&unfinished, 20);

    let mut backend = fixture(home);
    let snapshot = backend.request("snapshot", json!({})).unwrap();
    assert!(
        !backend.logs.contains_key(&old),
        "a log last written over 31 days ago is not read"
    );
    let head = &backend.logs[&counted];
    assert_eq!(head.parser_version, 6, "nor read again for a newer reader");
    assert_eq!(head.usage.total(), 13, "what it counted stays");
    let partial = &backend.logs[&unfinished];
    assert!(
        partial.offset < partial.length,
        "a pass reads only part of a long log"
    );
    assert!(backend.indexing(), "the rest waits for the next pass");
    assert_eq!(snapshot["indexing"]["complete"], false);
    assert_eq!(snapshot["indexing"]["pending"], 1);
    assert_eq!(snapshot["totals"]["total"], 13 + 3 * 13);
    let snapshot = backend.request("refresh", json!({})).unwrap();
    assert_eq!(
        snapshot["totals"]["total"],
        13 + 4 * 13,
        "a log within the retention is read to its end"
    );
    assert!(!backend.indexing());
    assert_eq!(snapshot["indexing"]["complete"], true);
    assert_eq!(snapshot["indexing"]["pending"], 0);
}

#[test]
fn turn_text_stays_in_memory_and_a_relaunch_starts_without_it() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = home.join(".claude/projects/test");
    fs::create_dir_all(&root).unwrap();
    let id = "12121212-1212-4121-8121-121212121212";
    let path = root.join(format!("{id}.jsonl"));
    let at = |seconds: i64| (Utc::now() - chrono::Duration::seconds(60 - seconds)).to_rfc3339();
    let lines = [
        json!({"sessionId":id,"type":"user","uuid":"u1","timestamp":at(0),"message":{"role":"user","content":"Fix the build"}}),
        json!({"sessionId":id,"type":"assistant","timestamp":at(5),"message":{"id":"m1","role":"assistant","model":"claude-fable-5","content":[{"type":"text","text":"Plan first.\n\nThe build is green again."}],"usage":{"input_tokens":3,"output_tokens":2}}}),
        json!({"sessionId":id,"type":"assistant","timestamp":at(6),"message":{"id":"m2","role":"assistant","model":"claude-fable-5","stop_reason":"tool_use","content":[{"type":"tool_use","id":"t1","name":"mcp__ccd_turn__wrap_up","input":{"recap":"Fixed the build.","ask":"Commit it?","blocked":true}}],"usage":{"input_tokens":1,"output_tokens":1}}}),
    ];
    fs::write(
        &path,
        lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let message = |snapshot: &Value| {
        snapshot["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap()["liveFacts"]["message"]
            .clone()
    };
    let asks = |snapshot: &Value| {
        snapshot["approvals"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|card| card["title"] == "Commit it?")
            .count()
    };
    let stored = |words: &str| -> i64 {
        rusqlite::Connection::open(home.join("app/ingestion.sqlite"))
            .unwrap()
            .query_row(
                "SELECT count(*) FROM ingestion_logs WHERE instr(payload,?1)>0 OR instr(coalesce(head,''),?1)>0",
                [words],
                |row| row.get(0),
            )
            .unwrap()
    };
    let mut backend = fixture(home);
    let snapshot = backend.request("snapshot", json!({})).unwrap();
    assert_eq!(message(&snapshot), "The build is green again.");
    assert_eq!(asks(&snapshot), 1);
    for words in [
        "The build is green again.",
        "Commit it?",
        "Fixed the build.",
    ] {
        assert_eq!(stored(words), 0, "no checkpoint keeps \"{words}\"");
    }
    // Read back from its checkpoint within the run, the log keeps them.
    backend.parser_cache = ParserCache::default();
    use std::io::Write;
    let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
    writeln!(
        file,
        "{}",
        json!({"sessionId":id,"type":"system","subtype":"informational","timestamp":at(7)})
    )
    .unwrap();
    let snapshot = backend.request("refresh", json!({})).unwrap();
    assert_eq!(message(&snapshot), "The build is green again.");
    assert_eq!(asks(&snapshot), 1);
    drop(backend);
    // After a relaunch the reply is blank until the next message; the
    // question is read again from its call's line.
    let mut backend = fixture(home);
    let snapshot = backend.request("snapshot", json!({})).unwrap();
    assert_eq!(message(&snapshot), Value::Null);
    assert_eq!(asks(&snapshot), 1);
}

#[test]
fn a_log_that_cannot_be_read_waits_out_a_pause_and_is_not_waited_for() {
    let temp = tempfile::tempdir().unwrap();
    let mut backend = fixture(temp.path());
    let complete = backend.request("snapshot", json!({})).unwrap();
    assert_eq!(complete["indexing"]["complete"], true);
    let path = temp.path().join(".claude/projects/test/locked.jsonl");
    backend.reads.pending.insert(path.clone());
    assert!(backend.indexing());
    let now = Utc::now().timestamp_millis();
    backend
        .reads
        .failed
        .failed(&path, "claude: Permission denied".into(), now);
    assert!(
        backend.reads.failed.pausing(&path, now + 29_000),
        "30 s after the first failure"
    );
    assert!(!backend.reads.failed.pausing(&path, now + 30_000));
    assert!(!backend.indexing(), "a pausing log is not waited for");
    let snapshot = backend
        .request("request_snapshot", json!({"changesOnly":true}))
        .unwrap();
    assert_eq!(
        (
            &snapshot["indexing"]["complete"],
            &snapshot["indexing"]["pending"]
        ),
        (&json!(false), &json!(1)),
        "pausing a read changes scheduling, not history completeness"
    );
    backend
        .reads
        .failed
        .failed(&path, "claude: Permission denied".into(), now);
    assert!(
        backend.reads.failed.pausing(&path, now + 59_000),
        "the pause doubles"
    );
    for _ in 0..8 {
        backend
            .reads
            .failed
            .failed(&path, "claude: Permission denied".into(), now);
    }
    assert!(
        !backend.reads.failed.pausing(&path, now + 300_000),
        "up to 300 s"
    );
    backend.reads.failed.succeeded(&path);
    assert!(backend.indexing());
}

#[test]
fn records_over_a_mebibyte_still_count_their_usage_and_close_their_calls() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".claude/projects/test");
    fs::create_dir_all(&root).unwrap();
    let at = Utc::now().to_rfc3339();
    let huge = "x\\\"y".repeat(800_000); // 4 MB with escaped quotes inside
    let write = json!({"sessionId":"s1","timestamp":at,"type":"assistant","message":{"id":"m1","role":"assistant","model":"claude-fable-5","content":[{"type":"tool_use","id":"toolu_1","name":"Write","input":{"file_path":"/w/big.txt","content":huge}}],"usage":{"input_tokens":7,"output_tokens":5}}});
    let result = json!({"sessionId":"s1","timestamp":at,"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":huge}]}});
    fs::write(root.join("s1.jsonl"), format!("{write}\n{result}\n")).unwrap();
    let mut backend = fixture(temp.path());
    let snapshot = backend.request("snapshot", json!({})).unwrap();
    assert_eq!(
        snapshot["totals"]["total"], 12,
        "the oversized answer's usage counts"
    );
    let message = snapshot["connection"]["message"].as_str().unwrap();
    assert!(!message.contains("oversized"), "{message}");
    // A pass reads at most 2 MB of one log; the result comes on the next.
    backend.request("refresh", json!({})).unwrap();
    let head = backend.logs.values().find(|head| head.id == "s1").unwrap();
    let calls = head
        .live
        .activity
        .as_ref()
        .unwrap()
        .calls
        .iter()
        .map(|call| call.id.clone())
        .collect::<Vec<_>>();
    assert!(
        calls.is_empty(),
        "the oversized result closed its call: {calls:?} {:?}",
        head.live.activity.as_ref().unwrap().line
    );
}

#[test]
fn incremental_batch_updates_preserve_unchanged_sessions_via_mapping() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".copilot/session-state");
    fs::create_dir_all(root.join("s1")).unwrap();
    fs::create_dir_all(root.join("s2")).unwrap();
    fs::write(
        root.join("s1/events.jsonl"),
        format!(
            "{}\n",
            json!({"type":"session.start","timestamp":"2026-10-19T01:00:00Z","data":{"sessionId":"s1","selectedModel":"gpt-4o"}})
        ),
    )
    .unwrap();
    fs::write(
        root.join("s2/events.jsonl"),
        format!(
            "{}\n",
            json!({"type":"session.start","timestamp":"2026-10-19T01:00:00Z","data":{"sessionId":"s2","selectedModel":"gpt-4o"}})
        ),
    )
    .unwrap();
    let otel_path = root.join("otel.jsonl");
    let otel_row = |sid: &str, span: &str, inp: u64, out: u64| {
        json!({
            "type":"span",
            "name":"chat gpt-4o",
            "traceId":format!("trace-{sid}"),
            "spanId":span,
            "startTime":[1760835600,0],
            "endTime":[1760835605,0],
            "attributes":{
                "gen_ai.conversation.id":sid,
                "gen_ai.operation.name":"chat",
                "gen_ai.response.model":"gpt-4o",
                "gen_ai.usage.input_tokens":inp,
                "gen_ai.usage.output_tokens":out
            }
        })
    };
    fs::write(
        &otel_path,
        format!(
            "{}\n{}\n",
            otel_row("s1", "sp1", 100, 50),
            otel_row("s2", "sp2", 200, 80)
        ),
    )
    .unwrap();
    let mut backend = AgentHudCore::new(
        vec![SourceRoot {
            path: root.clone(),
            provider: "copilot",
        }],
        temp.path().join("app"),
    );
    let snap1 = backend.request("snapshot", json!({})).unwrap();
    assert_eq!(snap1["totals"]["total"], 430);

    // Append an incremental span only for s2; s1 is not touched in the new delta.
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .append(true)
        .open(&otel_path)
        .unwrap();
    writeln!(f, "{}", otel_row("s2", "sp3", 30, 20)).unwrap();
    let snap2 = backend.request("refresh", json!({})).unwrap();
    // s1 (150) + s2 (280 + 50) = 480
    assert_eq!(snap2["totals"]["total"], 480);
}

#[test]
fn a_log_this_run_writes_keeps_the_meter_view() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".claude/projects/p");
    fs::create_dir_all(&root).unwrap();
    let id = "11111111-2222-3333-4444-555555555555";
    let at = Utc::now().timestamp_millis() - 600_000;
    let reply = |message: &str, at: i64, output: u64| {
        let timestamp = DateTime::<Utc>::from_timestamp_millis(at)
            .unwrap()
            .to_rfc3339();
        format!(
            "{}\n",
            json!({"type":"assistant","sessionId":id,"timestamp":timestamp,"message":{"id":message,"model":"claude-opus-4-5","role":"assistant","content":[{"type":"text","text":"Done."}],"usage":{"input_tokens":10,"output_tokens":output,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}})
        )
    };
    let path = root.join(format!("{id}.jsonl"));
    fs::write(&path, reply("m1", at, 20)).unwrap();
    let mut backend = AgentHudCore::new(vec![], temp.path().join("app"));
    backend.provider_roots = vec![(temp.path().join(".claude/projects"), "claude")];
    backend.request("snapshot", json!({})).unwrap();
    assert!(backend.meters.view.is_some());

    use std::io::Write;
    let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
    write!(f, "{}", reply("m2", at + 1_000, 30)).unwrap();
    backend.scan_impl(None).unwrap();
    // The view reads the written log again alone rather than every log.
    assert!(backend.meters.view.is_some());
    assert!(backend
        .meters
        .changes
        .iter()
        .any(|changed| std::path::Path::new(changed) == path));
    let snapshot = backend.request("refresh", json!({})).unwrap();
    assert_eq!(snapshot["totals"]["total"], 70);
}

/// Between passes a stat-only look notices a log its client holds open and
/// writes to, without reading it; the pass that follows reads it. A hook's
/// reminder waits as a read until a pass takes it.
#[test]
fn a_look_between_passes_notices_a_held_log_without_reading_it() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".claude/projects/p");
    fs::create_dir_all(&root).unwrap();
    let id = "21111111-2222-3333-4444-555555555555";
    let at = Utc::now().timestamp_millis() - 60_000;
    let reply = |message: &str, at: i64, output: u64| {
        let timestamp = DateTime::<Utc>::from_timestamp_millis(at)
            .unwrap()
            .to_rfc3339();
        format!(
            "{}\n",
            json!({"type":"assistant","sessionId":id,"timestamp":timestamp,"message":{"id":message,"model":"claude-opus-4-5","role":"assistant","content":[{"type":"text","text":"Done."}],"usage":{"input_tokens":10,"output_tokens":output,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}})
        )
    };
    let path = root.join(format!("{id}.jsonl"));
    fs::write(&path, reply("m1", at, 20)).unwrap();
    let mut backend = AgentHudCore::new(vec![], temp.path().join("app"));
    let roots = vec![(temp.path().join(".claude/projects"), "claude")];
    backend.provider_roots = roots.clone();
    let first = backend.request("snapshot", json!({})).unwrap();
    assert_eq!(first["totals"]["total"], 30);
    assert!(backend.active_logs().contains(&path));
    assert!(!backend.reads.listing.stale(&roots, &backend.active_logs()));

    // A write the watcher has not reported yet.
    use std::io::Write;
    let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
    write!(f, "{}", reply("m2", at + 1_000, 30)).unwrap();
    drop(f);
    assert!(backend.look(), "the held log grew");
    let quiet = backend.request("request_snapshot", json!({})).unwrap();
    assert_eq!(quiet["totals"]["total"], 30, "the look read nothing");
    let read = backend.request("snapshot", json!({})).unwrap();
    assert_eq!(read["totals"]["total"], 70);
    assert!(!backend.reads.listing.stale(&roots, &backend.active_logs()));

    // A log a hook pointed at waits as a read until a pass takes it.
    assert_eq!(backend.remind_paths(vec![path.clone()]), 1);
    assert!(backend.has_reminded_reads());
    backend
        .request("reminder_snapshot", json!({"changesOnly": true}))
        .unwrap();
    assert!(!backend.has_reminded_reads());
}

/// With nothing to wake it, the next pass is due at the walk of every log
/// folder, or sooner at the retry of a log whose read failed.
/// A session read while its first line was still being written has no event
/// yet. Its client holds the log open, so the watcher reports the rest late;
/// the look between passes still notices it, and the session keeps the id
/// its rollout's name gives from the first read on (a VS Code Codex rollout,
/// 2026-10-26).
#[test]
fn a_session_read_mid_first_line_is_looked_at_and_keeps_its_id() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join(".codex/sessions/2026/10/26");
    fs::create_dir_all(&folder).unwrap();
    let id = "01a1740e-b93b-7861-b384-33bd4f68dc1d";
    let path = folder.join(format!("rollout-2026-10-26T01-14-44-{id}.jsonl"));
    let timestamp = (Utc::now() - chrono::Duration::seconds(30)).to_rfc3339();
    let meta = format!(
        "{}\n",
        json!({"timestamp":timestamp,"type":"session_meta","payload":{"id":id,"session_id":id,"timestamp":timestamp,"cwd":"/tmp/p","originator":"codex_vscode","source":"vscode","cli_version":"0.162.0","instructions":"x".repeat(4_000)}})
    );
    let (head, tail) = meta.split_at(2_000);
    fs::write(&path, head).unwrap();
    let mut backend = AgentHudCore::new(vec![], temp.path().join("app"));
    let roots = vec![(temp.path().join(".codex/sessions"), "codex")];
    backend.provider_roots = roots.clone();
    backend.request("snapshot", json!({})).unwrap();
    let first = &backend.logs[&path];
    assert!(first.waiting_for_more && !first.observed);
    assert_eq!(
        first.id, id,
        "the rollout's name gives the id before its first line is whole"
    );
    assert!(backend.active_logs().contains(&path));

    // The rest of the line, which the watcher has not reported yet.
    use std::io::Write;
    let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
    write!(f, "{tail}").unwrap();
    drop(f);
    assert!(backend.look(), "the half-read log grew");
    backend.request("snapshot", json!({})).unwrap();
    let read = &backend.logs[&path];
    assert!(read.observed && !read.waiting_for_more);
    assert_eq!(read.id, id);
}

#[test]
fn the_next_pass_is_due_at_the_folder_walk_or_a_failed_reads_retry() {
    let temp = tempfile::tempdir().unwrap();
    let mut backend = fixture(temp.path());
    assert_eq!(backend.next_pass_due(), None, "no walk yet");
    backend.request("snapshot", json!({})).unwrap();
    let walk = backend.next_pass_due().expect("a walk is due");
    let from_now = walk.saturating_duration_since(std::time::Instant::now());
    assert!(
        from_now > std::time::Duration::from_secs(290)
            && from_now <= std::time::Duration::from_secs(300),
        "{from_now:?}"
    );
    let path = temp.path().join(".claude/projects/test/locked.jsonl");
    backend.reads.pending.insert(path.clone());
    backend.reads.failed.failed(
        &path,
        "claude: Permission denied".into(),
        Utc::now().timestamp_millis(),
    );
    let retry = backend.next_pass_due().unwrap();
    let from_now = retry.saturating_duration_since(std::time::Instant::now());
    assert!(
        from_now > std::time::Duration::from_secs(25)
            && from_now <= std::time::Duration::from_secs(30),
        "{from_now:?}"
    );
}

#[test]
fn a_ledger_is_indexed_with_its_progress_and_records_its_data_version() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = home.join(".claude/projects/test");
    fs::create_dir_all(&root).unwrap();
    // A log too long for one pass.
    let line = |n: usize| {
        json!({"sessionId":"s1","type":"assistant","message":{"id":format!("m{n}"),"usage":{"input_tokens":10,"output_tokens":3}},"pad":"x".repeat(900_000)})
            .to_string()
            + "\n"
    };
    fs::write(
        root.join("long.jsonl"),
        (0..4).map(line).collect::<String>(),
    )
    .unwrap();
    let bytes = |snapshot: &Value| {
        let indexing = &snapshot["indexing"];
        (
            indexing["bytesDone"].as_u64().unwrap(),
            indexing["bytesTotal"].as_u64().unwrap(),
        )
    };

    let mut backend = fixture(home);
    let first = backend.request("snapshot", json!({})).unwrap();
    assert_eq!(first["indexing"]["kind"], "first", "{}", first["indexing"]);
    assert_eq!(first["indexing"]["dataVersion"], crate::DATA_VERSION);
    let (done, total) = bytes(&first);
    assert!(done > 0 && done < total, "{}", first["indexing"]);
    let finished = backend.request("refresh", json!({})).unwrap();
    assert_eq!(finished["indexing"]["kind"], Value::Null);
    let (done, total) = bytes(&finished);
    assert_eq!(done, total);
    drop(backend);
    let ledger = rusqlite::Connection::open(home.join("app/ingestion.sqlite")).unwrap();
    let recorded: String = ledger
        .query_row(
            "SELECT value FROM ingestion_meta WHERE key='data_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(recorded, crate::DATA_VERSION);

    // A ledger recorded under another data version is read again.
    ledger
        .execute(
            "UPDATE ingestion_meta SET value='0.5.9' WHERE key='data_version'",
            [],
        )
        .unwrap();
    ledger
        .execute(
            "UPDATE ingestion_logs SET head=json_set(head,'$.parser_version',6),payload=json_set(payload,'$.parser_version',6)",
            [],
        )
        .unwrap();
    drop(ledger);
    let mut backend = fixture(home);
    let again = backend.request("snapshot", json!({})).unwrap();
    assert_eq!(
        again["indexing"]["kind"], "reindex",
        "{}",
        again["indexing"]
    );
    let (done, total) = bytes(&again);
    assert!(done < total, "{}", again["indexing"]);
    let finished = backend.request("refresh", json!({})).unwrap();
    assert_eq!(finished["indexing"]["kind"], Value::Null);
    assert_eq!(
        finished["totals"]["total"],
        4 * 13,
        "read again, counted once"
    );

    // Launched again, a ledger that holds this build's data version is not.
    drop(backend);
    let mut backend = fixture(home);
    let settled = backend.request("snapshot", json!({})).unwrap();
    assert_eq!(settled["indexing"]["kind"], Value::Null);
}
