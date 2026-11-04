//! Session lists: names and titles, archived and reviewer threads, search,
//! sub-agents and each Claude session's share of the 5-hour window.
use super::*;

fn project_worker_fixture(home: &Path, suffix: u8, managed: bool, known_origin: bool) -> String {
    let cli = format!("11111111-1111-4111-8111-{suffix:012}");
    let tab = format!("local_22222222-2222-4222-8222-{suffix:012}");
    let folder = home.join("Library/Application Support/Claude/claude-code-sessions/org/account");
    fs::create_dir_all(&folder).unwrap();
    let mut metadata = json!({"sessionId":tab,"cliSessionId":cli,"isArchived":false});
    if managed {
        metadata["remoteControlSpawn"] =
            json!({"projectThreadChild":true,"ccrSessionId":format!("cse_synthetic{suffix:06}")});
    }
    fs::write(folder.join(format!("{tab}.json")), metadata.to_string()).unwrap();
    let root = home.join(".claude/projects/synthetic");
    fs::create_dir_all(&root).unwrap();
    let thread = if known_origin {
        "cmsg_syntheticproject_thread"
    } else {
        "cmsg_conflicting_project"
    };
    let at = Utc::now().to_rfc3339();
    let lines = [
        json!({"sessionId":cli,"type":"user","uuid":"prompt","timestamp":at,"entrypoint":"claude-desktop","projectsUserTurn":true,"textNotTyped":true,"promptSource":"sdk","isSidechain":false,"message":{"role":"user","content":format!("<wake reason=\"mention\"><project type=\"project\" id=\"chan_syntheticproject\"><thread ts=\"{thread}\"><message trigger=\"true\">Synthetic task</message></thread></project></wake>")}}),
        json!({"sessionId":cli,"type":"assistant","uuid":"reply","timestamp":at,"message":{"id":"reply","role":"assistant","model":"synthetic-model","content":[{"type":"text","text":"Synthetic reply"}],"usage":{"input_tokens":10,"output_tokens":3}}}),
    ];
    fs::write(
        root.join(format!("{cli}.jsonl")),
        lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .unwrap();
    cli
}

#[test]
fn unsupported_project_host_preserves_existing_local_session_routes() {
    let temp = tempfile::tempdir().unwrap();
    let worker = project_worker_fixture(temp.path(), 1, true, true);
    let standalone = project_worker_fixture(temp.path(), 2, false, true);
    let mut core = fixture(temp.path());
    core.set_claude_projects_supported(false);
    core.request("snapshot", json!({})).unwrap();
    let page = core.request("sessions", json!({"scope":"all"})).unwrap();
    let rows = page["items"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for id in [worker, standalone] {
        let row = rows.iter().find(|row| row["id"] == id).unwrap();
        assert!(row["desktopProject"].is_null());
        let detail = core.request("session_detail", json!({"id":id})).unwrap();
        assert!(detail["session"]["desktopProject"].is_null());
        assert_eq!(
            detail["session"]["navigationTarget"]["kind"],
            "claude-desktop"
        );
    }
}

#[test]
fn project_workers_fold_into_original_conversation_and_never_fall_back_to_local() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let first = project_worker_fixture(home, 1, true, true);
    let second = project_worker_fixture(home, 2, true, true);
    let unresolved = project_worker_fixture(home, 3, true, false);
    let standalone = project_worker_fixture(home, 4, false, true);
    let mut core = fixture(home);
    let snapshot = core.request("snapshot", json!({})).unwrap();
    let key = "claude-project:chan_syntheticproject:cmsg_syntheticproject_thread";
    let page = core.request("sessions", json!({"scope":"all"})).unwrap();
    let rows = page["items"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        3,
        "one Project, one unavailable Project, one independent Local"
    );
    assert!(!rows
        .iter()
        .any(|row| row["id"] == first || row["id"] == second));
    let unknown = rows
        .iter()
        .find(|row| row["desktopProject"]["openAvailable"] == false)
        .unwrap();
    assert_eq!(unknown["desktopProject"]["localWorkerCount"], 1);
    let unknown_detail = core
        .request("session_detail", json!({"id":unknown["id"]}))
        .unwrap();
    assert_eq!(
        unknown_detail["session"]["desktopProject"]["localWorkerCount"],
        1
    );
    assert_eq!(unknown_detail["children"].as_array().unwrap().len(), 1);
    let project = rows.iter().find(|row| row["id"] == key).unwrap();
    assert_eq!(project["desktopProject"]["status"], "unknown");
    assert_eq!(project["desktopProject"]["execution"], "local");
    assert_eq!(project["desktopProject"]["kindLabel"], "Local");
    assert_eq!(project["desktopProject"]["localWorkerCount"], 2);
    assert_eq!(
        project["tokens"], 26,
        "the row adds up what its workers ran on this Mac"
    );
    assert_eq!(
        project["desktopProject"]["projectLabel"],
        "Claude Project synthe"
    );
    assert_eq!(
        page["projectLabels"]["claude-project:chan_syntheticproject"],
        "Claude Project synthe"
    );
    assert_eq!(unknown["tokens"], 13);
    assert_eq!(project["navigationTarget"]["kind"], "claude-project-thread");
    assert!(!project["title"].as_str().unwrap().contains("wake"));
    assert!(!snapshot["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["id"] == first));
    let target = project["navigationTarget"].clone();
    let detail = core.request("session_detail", json!({"id":key})).unwrap();
    assert_eq!(detail["children"].as_array().unwrap().len(), 2);
    assert_eq!(detail["usage"]["total"], 26);
    for worker in [&first, &second] {
        let detail = core
            .request("session_detail", json!({"id":worker}))
            .unwrap();
        assert_eq!(detail["session"]["navigationTarget"], target);
        assert_eq!(
            detail["session"]["tokens"], 13,
            "worker usage stays available exactly once"
        );
    }
    let detail = core
        .request("session_detail", json!({"id":unresolved}))
        .unwrap();
    assert!(detail["session"]["navigationTarget"].is_null());
    assert_eq!(detail["session"]["desktopProject"]["openAvailable"], false);
    let detail = core
        .request("session_detail", json!({"id":standalone}))
        .unwrap();
    assert_eq!(
        detail["session"]["navigationTarget"]["kind"],
        "claude-desktop"
    );
    assert!(
        detail["session"]["desktopProject"].is_null(),
        "ordinary Local needs both provenance sources"
    );

    let page = core
        .request(
            "sessions",
            json!({"scope":"all","project":"claude-project:chan_syntheticproject"}),
        )
        .unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["items"][0]["id"], key);
    assert_eq!(page["items"][0]["desktopProject"]["localWorkerCount"], 2);
}

#[test]
fn all_sessions_keeps_old_project_conversations() {
    let temp = tempfile::tempdir().unwrap();
    let cli = project_worker_fixture(temp.path(), 1, true, true);
    let path = temp
        .path()
        .join(format!(".claude/projects/synthetic/{cli}.jsonl"));
    let old = (Utc::now() - chrono::Duration::days(60)).to_rfc3339();
    let contents = fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| {
            let mut value: Value = serde_json::from_str(line).unwrap();
            value["timestamp"] = json!(old);
            value.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(path, contents).unwrap();
    let mut core = fixture(temp.path());
    core.request("snapshot", json!({})).unwrap();
    let all = core.request("sessions", json!({"scope":"all"})).unwrap();
    assert_eq!(all["items"].as_array().unwrap().len(), 1);
    assert_eq!(all["items"][0]["desktopProject"]["localWorkerCount"], 1);
    let recent = core.request("sessions", json!({"scope":"recent"})).unwrap();
    assert!(recent["items"].as_array().unwrap().is_empty());
}

#[test]
fn existing_project_worker_checkpoints_reparse_without_changing_usage() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let cli = project_worker_fixture(home, 1, true, true);
    let mut core = fixture(home);
    core.request("snapshot", json!({})).unwrap();
    drop(core);
    let db = rusqlite::Connection::open(home.join("app/ingestion.sqlite")).unwrap();
    // A checkpoint read by the rules before Project origins were kept.
    let older = agent_hud_agents::transcript::PARSER_VERSION - 1;
    db.execute(
        "UPDATE ingestion_logs SET head=json_remove(json_set(head,'$.parser_version',?1),'$.live.claude_project_origin'), payload=json_remove(json_set(payload,'$.parser_version',?1),'$.live.claude_project_origin')",
        [older],
    )
    .unwrap();
    drop(db);
    let mut core = fixture(home);
    core.request("snapshot", json!({})).unwrap();
    let detail = core.request("session_detail", json!({"id":cli})).unwrap();
    assert_eq!(
        detail["session"]["navigationTarget"]["kind"],
        "claude-project-thread"
    );
    assert_eq!(detail["session"]["tokens"], 13);
    let head = core.logs.values().find(|log| log.id == cli).unwrap();
    assert_eq!(
        head.parser_version,
        agent_hud_agents::transcript::PARSER_VERSION
    );
    assert!(head.live.claude_project_origin.is_some());
    core.request("snapshot", json!({})).unwrap();
    assert_eq!(
        core.request("session_detail", json!({"id":cli})).unwrap()["session"]["tokens"],
        13
    );
}

#[test]
fn claude_sessions_take_the_given_name_then_the_generated_title_then_the_prompt() {
    let mut log = Log::new("s.jsonl".into(), "claude");
    log.parse(&json!({"sessionId":"s1","cwd":"/w/repo","type":"user","message":{"content":"<command-name>/init</command-name>"}}));
    assert_eq!(
        log.title(),
        "repo",
        "a command prompt gives no title; the folder stands in"
    );
    log.parse(&json!({"sessionId":"s1","type":"user","message":{"content":"  fix the build\r\nthen test"}}));
    assert_eq!(log.title(), "fix the build");
    log.parse(&json!({"sessionId":"s1","type":"ai-title","aiTitle":"Fix the failing build"}));
    assert_eq!(log.title(), "Fix the failing build");
    log.parse(&json!({"sessionId":"s1","type":"custom-title","customTitle":"Release blocker"}));
    log.parse(&json!({"sessionId":"s1","type":"ai-title","aiTitle":"A newer generated title"}));
    assert_eq!(log.title(), "Release blocker");
    assert_eq!(
        crate::sessions::log_session(&log, 0, None)["title"],
        "Release blocker"
    );
}

#[test]
fn append_refresh_duplicate_sessions_and_settings_persistence() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".claude/projects/test");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("session.jsonl");
    let row = json!({"sessionId":"s1","type":"assistant","message":{"id":"m1","usage":{"input_tokens":10,"output_tokens":3}}});
    fs::write(&path, format!("{row}\n")).unwrap();
    let mut backend = fixture(temp.path());
    assert_eq!(
        backend.request("snapshot", json!({})).unwrap()["totals"]["total"],
        13
    );
    use std::io::Write;
    let next = json!({"sessionId":"s1","type":"assistant","message":{"id":"m2","usage":{"input_tokens":4,"output_tokens":2}}});
    let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
    write!(file, "{next}").unwrap();
    assert_eq!(
        backend.request("refresh", json!({})).unwrap()["totals"]["total"],
        13
    );
    writeln!(file).unwrap();
    assert_eq!(
        backend.request("refresh", json!({})).unwrap()["totals"]["total"],
        19
    );
    // Of a session's copies the most recently modified counts, as Open's
    // LedgerCopies: a newer copy holding only the first answer counts it.
    fs::write(root.join("copy.jsonl"), format!("{row}\n")).unwrap();
    let touch = |path: &Path, seconds: u64| {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(seconds))
            .unwrap();
    };
    touch(&root.join("copy.jsonl"), 60);
    assert_eq!(
        backend.request("refresh", json!({})).unwrap()["totals"]["total"],
        13
    );
    touch(&path, 120);
    assert_eq!(
        backend.request("refresh", json!({})).unwrap()["totals"]["total"],
        19
    );
    backend
        .request("set_provider", json!({"id":"claude","enabled":false}))
        .unwrap();
    let mut reopened = fixture(temp.path());
    assert_eq!(
        reopened.request("snapshot", json!({})).unwrap()["sessions"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert!(reopened.request("toggle_hud", json!({})).is_err());
}

#[test]
fn guardian_reviews_count_their_usage_but_list_no_session() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".codex/sessions/2026/10/21");
    fs::create_dir_all(&root).unwrap();
    let at = Utc::now().to_rfc3339();
    let rollout = |id: &str, source: Value, input: u64| {
        format!(
            "{}\n{}\n{}\n",
            json!({"timestamp":at,"type":"session_meta","payload":{"id":id,"timestamp":at,"cwd":"/w/repo","source":source,"originator":"codex_cli_rs"}}),
            json!({"timestamp":at,"type":"turn_context","payload":{"model":"gpt-5.5"}}),
            json!({"timestamp":at,"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":input,"output_tokens":10}}}}),
        )
    };
    let main = "11111111-1111-4111-8111-111111111111";
    let review = "22222222-2222-4222-8222-222222222222";
    fs::write(
        root.join(format!("rollout-a-{main}.jsonl")),
        rollout(main, json!("cli"), 100),
    )
    .unwrap();
    fs::write(
        root.join(format!("rollout-b-{review}.jsonl")),
        rollout(review, json!({"subagent":{"other":"guardian"}}), 50),
    )
    .unwrap();
    let mut backend = fixture(temp.path());
    let snapshot = backend.request("snapshot", json!({})).unwrap();
    assert_eq!(
        snapshot["totals"]["total"], 170,
        "a guardian review's usage counts"
    );
    let listed = |value: &Value| {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|session| session["id"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        listed(&snapshot["sessions"]),
        vec![json!(main)],
        "its session is not listed"
    );
    let all = backend.request("sessions", json!({"scope":"all"})).unwrap();
    assert_eq!(listed(&all["items"]), vec![json!(main)]);
    let codex = snapshot["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "codex")
        .unwrap();
    assert_eq!(codex["sessions"], 1);
}

#[test]
fn a_claude_sessions_window_share_counts_cache_writes_not_cache_reads() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".claude/projects/test");
    fs::create_dir_all(&root).unwrap();
    let at = Utc::now().to_rfc3339();
    let line = |session: &str, usage: Value| json!({"sessionId":session,"type":"assistant","timestamp":at,"message":{"id":format!("m-{session}"),"model":"claude-fable-5","usage":usage}});
    // a: 100 in + 100 out, a million cache reads; b: 100 in + 200 cache writes + 100 out.
    fs::write(
        root.join("a.jsonl"),
        format!(
            "{}\n",
            line(
                "a",
                json!({"input_tokens":100,"output_tokens":100,"cache_read_input_tokens":1_000_000})
            )
        ),
    )
    .unwrap();
    fs::write(
        root.join("b.jsonl"),
        format!(
            "{}\n",
            line(
                "b",
                json!({"input_tokens":100,"output_tokens":100,"cache_creation_input_tokens":200})
            )
        ),
    )
    .unwrap();
    let mut backend = fixture(temp.path());
    let reset = (Utc::now() + chrono::Duration::hours(4)).to_rfc3339();
    backend.set_account_projection(AccountProjection {
        quotas: vec![json!({"id":"claude:x:claude-session","vendor":"claude","name":"Current session","usedPercent":60.0,"resetAt":reset})],
        ..Default::default()
    });
    let snapshot = backend.request("snapshot", json!({})).unwrap();
    let share = |id: &str| {
        snapshot["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap()["pctOfWindow"]
            .as_f64()
            .unwrap()
    };
    assert!((share("a") - 20.0).abs() < 1e-9, "a: 200 of 600 tokens");
    assert!((share("b") - 40.0).abs() < 1e-9, "b: 400 of 600 tokens");
}

fn rollout(id: &str, at: &str, meta: Value, rest: &[Value]) -> String {
    let mut payload =
        json!({"id":id,"timestamp":at,"cwd":"/w/repo","originator":"codex_cli_rs","source":"cli"});
    for (key, value) in meta.as_object().unwrap() {
        payload[key] = value.clone();
    }
    let mut lines = vec![json!({"timestamp":at,"type":"session_meta","payload":payload})];
    lines.extend(rest.iter().cloned());
    lines.iter().map(|line| format!("{line}\n")).collect()
}

#[test]
fn archived_codex_threads_count_but_leave_the_list_and_desktop_threads_open_in_codex() {
    let temp = tempfile::tempdir().unwrap();
    let sessions = temp.path().join(".codex/sessions/2026/10/21");
    let archived = temp.path().join(".codex/archived_sessions");
    fs::create_dir_all(&sessions).unwrap();
    fs::create_dir_all(&archived).unwrap();
    let at = Utc::now().to_rfc3339();
    let count = |input: u64| json!({"timestamp":at,"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":input,"output_tokens":0}}}});
    let desktop = "33333333-3333-4333-8333-333333333333";
    let gone = "44444444-4444-4444-8444-444444444444";
    fs::write(
        sessions.join(format!("rollout-a-{desktop}.jsonl")),
        rollout(
            desktop,
            &at,
            json!({"source":"vscode","originator":"Codex Desktop"}),
            &[count(10)],
        ),
    )
    .unwrap();
    fs::write(
        archived.join(format!("rollout-b-{gone}.jsonl")),
        rollout(gone, &at, json!({}), &[count(5)]),
    )
    .unwrap();
    let mut backend = fixture(temp.path());
    backend
        .provider_roots
        .push((temp.path().join(".codex/archived_sessions"), "codex"));
    let snapshot = backend.request("snapshot", json!({})).unwrap();
    assert_eq!(
        snapshot["totals"]["total"], 15,
        "an archived thread's usage counts"
    );
    let listed = snapshot["sessions"].as_array().unwrap();
    assert_eq!(listed.len(), 1, "an archived thread is not listed");
    assert_eq!(listed[0]["id"], desktop);
    assert_eq!(
        listed[0]["navigationTarget"],
        json!({"kind":"codex-thread","id":desktop})
    );
}

#[test]
fn codex_titles_come_from_completed_prompts_then_agent_names_and_turns_carry_progress() {
    let at = "2026-10-21T08:00:00Z";
    let mut log = Log::new("rollout-x.jsonl".into(), "codex");
    let spawned = rollout(
        "55555555-5555-4555-8555-555555555555",
        at,
        json!({"source":{"subagent":{"thread_spawn":{"parent_thread_id":"p","agent_path":"/agents/reviewer","agent_nickname":"Ada"}}}}),
        &[],
    );
    for line in spawned.lines() {
        log.parse(&serde_json::from_str(line).unwrap());
    }
    assert_eq!(
        log.title(),
        "reviewer",
        "a spawned agent is titled by its path before its folder"
    );
    log.parse(&json!({"timestamp":at,"type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","content":[{"type":"image"},{"type":"text","text":"Review the parser\nthen report"}]}}}));
    assert_eq!(
        log.title(),
        "Review the parser",
        "a completed UserMessage item titles the session"
    );

    let mut live = LiveLog::default();
    live.codex(&json!({"timestamp":at,"type":"session_meta","payload":{"id":"s","timestamp":at}}));
    live.codex(&json!({"timestamp":"2026-10-21T08:00:01Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}));
    live.codex(&json!({"timestamp":"2026-10-21T08:00:02Z","type":"event_msg","payload":{"type":"agent_message","phase":"commentary","message":"Reading the tests first."}}));
    assert_eq!(
        live.turns.back().unwrap().message.as_deref(),
        Some("Reading the tests first."),
        "progress is the running turn's message"
    );
    live.codex(&json!({"timestamp":"2026-10-21T08:00:03Z","type":"event_msg","payload":{"type":"agent_message","phase":"final_answer","message":"All green."}}));
    live.codex(&json!({"timestamp":"2026-10-21T08:00:04Z","type":"event_msg","payload":{"type":"agent_message","phase":"commentary","message":"Cleaning up."}}));
    live.codex(&json!({"timestamp":"2026-10-21T08:00:05Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"t1"}}));
    assert_eq!(
        live.turns.back().unwrap().message.as_deref(),
        Some("All green."),
        "a completion without its own answer takes the final one"
    );
}

#[test]
fn session_search_matches_the_client_name() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".claude/projects/test");
    fs::create_dir_all(&root).unwrap();
    let row = json!({"sessionId":"s1","entrypoint":"claude-vscode","type":"assistant","message":{"id":"m1","usage":{"input_tokens":10,"output_tokens":3}}});
    fs::write(root.join("session.jsonl"), format!("{row}\n")).unwrap();
    let mut backend = fixture(temp.path());
    backend.request("snapshot", json!({})).unwrap();
    let found = |backend: &mut AgentHudCore, search: &str| {
        backend
            .request("sessions", json!({"scope":"all","search":search}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .clone()
    };
    let items = found(&mut backend, "IDE extension");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["client"], "Claude Code IDE extension");
    assert!(found(&mut backend, "Claude Code Desktop").is_empty());
}

#[test]
fn sessions_list_newest_last_message_first_whatever_their_state() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".claude/projects/test");
    fs::create_dir_all(&root).unwrap();
    let now = Utc::now();
    let at = |minutes: i64| (now - chrono::Duration::minutes(minutes)).to_rfc3339();
    let usage = json!({"input_tokens":10,"output_tokens":3});
    // "quiet" said its last words 20 minutes ago, then ran a tool until a
    // minute ago: its last event is the newest, its last message is not.
    let quiet = [
        json!({"sessionId":"quiet","type":"user","timestamp":at(30),"uuid":"q1","message":{"role":"user","content":"Run the suite"}}),
        json!({"sessionId":"quiet","type":"assistant","timestamp":at(20),"message":{"id":"qa","model":"claude-opus-4-5","role":"assistant","content":[{"type":"text","text":"Running it."},{"type":"tool_use","id":"t1","name":"Bash","input":{}}],"usage":usage}}),
        json!({"sessionId":"quiet","type":"user","timestamp":at(1),"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}),
    ];
    // "talked" finished its turn five minutes ago.
    let talked = [
        json!({"sessionId":"talked","type":"user","timestamp":at(6),"uuid":"t1","message":{"role":"user","content":"Hi"}}),
        json!({"sessionId":"talked","type":"assistant","timestamp":at(5),"message":{"id":"ta","model":"claude-opus-4-5","role":"assistant","content":[{"type":"text","text":"Hello."}],"stop_reason":"end_turn","usage":usage}}),
    ];
    let lines = |rows: &[Value]| rows.iter().map(|r| format!("{r}\n")).collect::<String>();
    fs::write(root.join("quiet.jsonl"), lines(&quiet)).unwrap();
    fs::write(root.join("talked.jsonl"), lines(&talked)).unwrap();
    let mut backend = fixture(temp.path());
    backend.request("snapshot", json!({})).unwrap();
    let page = backend
        .request(
            "sessions",
            json!({"scope":"recent","states":["running"],"sort":"active"}),
        )
        .unwrap();
    let items = page["items"].as_array().unwrap();
    let ids = items
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        ["talked", "quiet"],
        "a state filter or sort is no longer read"
    );
    assert_eq!(
        items[1]["state"], "running",
        "the state is still worked out"
    );
    assert!(page.get("activeCount").is_none());
    let message_at = |row: &Value| {
        chrono::DateTime::parse_from_rfc3339(row["lastMessageAt"].as_str().unwrap())
            .unwrap()
            .timestamp()
    };
    assert_eq!(
        message_at(&items[1]),
        (now - chrono::Duration::minutes(20)).timestamp()
    );
}

#[test]
fn unified_subagent_model_tracks_parent_and_child_across_agents() {
    let mut child = Log::new(
        "/tmp/.codebuddy/projects/p1/parent-uuid/subagents/agent-a1.jsonl".into(),
        "codebuddy",
    );
    child.parse(&json!({
        "type": "message",
        "id": "m1",
        "parentId": "parent-uuid",
        "timestamp": Utc::now().to_rfc3339(),
        "role": "user",
        "content": [{"type": "text", "text": "Inspect subagent task"}]
    }));
    assert!(child.live.is_subagent);
    assert_eq!(
        child.live.parent_id.as_deref(),
        Some("codebuddy:parent-uuid")
    );
}

#[test]
fn a_deepseek_harness_reply_reads_from_its_compressed_log() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join(".dsh/sessions/--w--/session-f8d6");
    fs::create_dir_all(&folder).unwrap();
    let at = Utc::now().timestamp_millis() - 60_000;
    let reply = "收到了你的消息；我是 DeepSeek Harness 中的 deepseek-flash 模型。";
    let rows = [
        json!({"type":"session","version":4,"id":"session-f8d6","createdAt":at,"cwd":"/w","isSeeded":false,"delegationDepth":0}),
        json!({"type":"turn/start","seq":44,"time":at + 1_000,"data":{"turn":3}}),
        json!({"type":"user/message","seq":48,"time":at + 1_024,"data":{"content":[{"type":"text","text":"[Agent HUD · from Claude Code · Test · m-b0bfc3510306]\n\nWhich model are you?"}],"source":{"kind":"user","rpcId":"m-b0bfc3510306"},"role":"user"}}),
        json!({"type":"user/message","seq":50,"time":at + 1_024,"data":{"content":[{"type":"text","text":"Time sampled while preparing turn 3, step 1"}],"source":{"kind":"time-context"},"role":"user"}}),
        json!({"type":"assistant/message","seq":54,"time":at + 2_673,"data":{"turn":3,"step":1,"message":{"role":"assistant","content":[{"type":"reasoning","text":"The user wants one sentence."},{"type":"text","text":reply}]},"usage":{"inputTokens":6841,"outputTokens":134,"cacheReadTokens":9472,"cacheWriteTokens":0}}}),
        json!({"type":"turn/end","seq":56,"time":at + 2_674,"data":{"turn":3,"reason":{"kind":"completed"}}}),
    ];
    let text = rows
        .iter()
        .map(|row| format!("{row}\n"))
        .collect::<String>();
    // Harness adds a frame each time it writes.
    let bytes = rows
        .iter()
        .flat_map(|row| zstd::stream::encode_all(format!("{row}\n").as_bytes(), 3).unwrap())
        .collect::<Vec<_>>();
    fs::write(folder.join("session.v4.jsonl.zstd"), bytes).unwrap();
    let mut core = fixture(temp.path());
    core.provider_roots
        .push((temp.path().join(".dsh/sessions"), "deepseek"));
    core.request("snapshot", json!({})).unwrap();
    let id = "deepseek:session-f8d6";

    // The end Agent HUD's messaging reads, and the pages from the start.
    let tail = core
        .request(
            "session_detail",
            json!({"id":id,"messageTail":true,"messageLimit":200,"callLimit":1}),
        )
        .unwrap();
    assert!(tail["messageNotice"].is_null(), "{tail:#}");
    assert_eq!(tail["endMessageCursor"], text.len());
    let first = core
        .request("session_detail", json!({"id":id,"messageCursor":0}))
        .unwrap();
    for page in [&tail, &first] {
        let messages = page["messages"].as_array().unwrap();
        let roles = messages
            .iter()
            .map(|row| (row["role"].as_str().unwrap(), row["isPrompt"] == true))
            .collect::<Vec<_>>();
        assert_eq!(
            roles,
            [("user", true), ("user", false), ("assistant", false)],
            "{page:#}"
        );
        assert_eq!(messages[2]["text"], reply);
        assert!(messages.iter().all(|row| row["turnId"] == "3"));
    }
}

#[test]
fn a_session_opens_where_its_row_says_from_the_detail_too() {
    // `open_session` reads the session as its detail shows it, from the
    // parsed log; the list shows it from the log's head. Both must name the
    // same way back, or the click falls back to revealing the log in Finder.
    let now = chrono::Utc::now().timestamp_millis();
    let cases = [
        (
            "grok-bot",
            "grok-bot:slothash:126dbae3-ad9b-4b3c-9bb5-07dfd11de3df",
            None,
            None,
            json!({"kind":"grok-bot-agent","id":"126dbae3-ad9b-4b3c-9bb5-07dfd11de3df"}),
        ),
        (
            "cursor",
            "cursor:839140eb-232f-47fb-ab0d-1e8fb31a5fcb",
            Some("/Users/a/project"),
            None,
            json!({"kind":"cursor-agent","id":"839140eb-232f-47fb-ab0d-1e8fb31a5fcb","cwd":"/Users/a/project"}),
        ),
        // A Cursor session with no folder has no target of its own.
        ("cursor", "cursor:3e8f", None, None, Value::Null),
        (
            "antigravity",
            "antigravity:c1",
            Some("/w"),
            Some("Desktop"),
            json!({"kind":"antigravity-conversation","id":"c1","cwd":"/w"}),
        ),
        // Antigravity's CLI has no conversation to open.
        (
            "antigravity",
            "antigravity:c2",
            None,
            Some("CLI"),
            Value::Null,
        ),
    ];
    for (source, id, cwd, client, expected) in cases {
        let mut log = Log::new(format!("{source}.log").into(), source);
        log.id = id.into();
        log.cwd = cwd.map(str::to_owned);
        log.client = client.map(str::to_owned);
        let detail = crate::sessions::log_session(&log, now, None);
        let row = crate::database::checkpoints::LogHead::from_log(&log).session(now, None);
        assert_eq!(row["navigationTarget"], expected, "{source} row");
        assert_eq!(detail["navigationTarget"], expected, "{source} detail");
    }
}
