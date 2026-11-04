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
fn the_sessions_days_add_up_every_matching_session_by_its_local_day() {
    use chrono::TimeZone;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".claude/projects/test");
    fs::create_dir_all(&root).unwrap();
    // New York keeps summer time, so its days are not 24 hours apart.
    let zone = chrono_tz::America::New_York;
    let now = Utc::now().timestamp_millis();
    let local_midnight = |date: chrono::NaiveDate| {
        zone.from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .unwrap()
            .timestamp_millis()
    };
    let today = zone.timestamp_millis_opt(now).unwrap().date_naive();
    let midnight = local_midnight(today);
    let yesterday = local_midnight(today.pred_opt().unwrap());
    let session = |id: &str, at: i64, cwd: &str, input: u64, model: &str| {
        let at = iso(at);
        let lines = [
            json!({"sessionId":id,"type":"user","timestamp":at,"uuid":format!("{id}-u"),"cwd":cwd,"message":{"role":"user","content":format!("Task {id}")}}),
            json!({"sessionId":id,"type":"assistant","timestamp":at,"message":{"id":format!("{id}-a"),"model":model,"role":"assistant","content":[{"type":"text","text":"Done."}],"stop_reason":"end_turn","usage":{"input_tokens":input,"output_tokens":0}}}),
        ];
        let text = lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>();
        fs::write(root.join(format!("{id}.jsonl")), text).unwrap();
    };
    // Six sessions today, two just before today began, one ten days ago.
    for (index, input) in [600, 500, 400, 300, 200, 100].into_iter().enumerate() {
        let cwd = if input == 100 {
            "/work/other"
        } else {
            "/work/app"
        };
        session(
            &format!("t{}", index + 1),
            midnight + (now - midnight) / (index as i64 + 2),
            cwd,
            input,
            "claude-opus-4-5",
        );
    }
    session("y1", midnight - 60_000, "/work/app", 50, "claude-opus-4-5");
    // A session on a model with no list price leaves its day no whole cost.
    session("y2", midnight - 120_000, "/work/app", 30, "house-model");
    session(
        "e1",
        now - 10 * 86_400_000,
        "/work/app",
        7,
        "claude-opus-4-5",
    );
    let mut core = fixture(temp.path());
    core.request("snapshot", json!({})).unwrap();
    let query = |extra: Value| {
        let mut query = json!({"scope":"all","limit":2,"timeZone":"America/New_York"});
        query
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        query
    };
    let page = core.request("sessions", query(json!({}))).unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    assert_eq!(page["total"], 9);
    let days = page["days"].as_array().unwrap();
    let starts = days
        .iter()
        .map(|day| day["start"].clone())
        .collect::<Vec<_>>();
    assert_eq!(starts, [iso(midnight), iso(yesterday), Value::Null]);
    let counts = days.iter().map(|day| {
        (
            day["sessions"].as_u64().unwrap(),
            day["tokens"].as_u64().unwrap(),
        )
    });
    assert_eq!(
        counts.collect::<Vec<_>>(),
        [(6, 2_100), (2, 80), (1, 7)],
        "every matching session, not only the page's two"
    );
    let ranked = |day: &Value| {
        let top = day["top"].as_array().unwrap().iter();
        top.map(|row| json!([row["id"], row["tokens"]]))
            .collect::<Value>()
    };
    assert_eq!(
        ranked(&days[0]),
        json!([
            ["t1", 600],
            ["t2", 500],
            ["t3", 400],
            ["t4", 300],
            ["t5", 200]
        ])
    );
    assert_eq!(days[0]["rest"], json!({"count": 1, "tokens": 100}));
    assert_eq!(days[0]["usage"]["input"], 2_100);
    assert_eq!(days[0]["top"][0]["title"], page["items"][0]["title"]);
    // A day's money is its rows' added up.
    let all = core
        .request("sessions", query(json!({"limit": 200})))
        .unwrap();
    let rows = all["items"].as_array().unwrap();
    let cost = rows[..6]
        .iter()
        .map(|row| row["cost"].as_f64().unwrap())
        .sum::<f64>();
    assert!(cost > 0.0);
    assert!((days[0]["cost"].as_f64().unwrap() - cost).abs() < 1e-12);
    assert_eq!(days[0]["pricedCost"], days[0]["cost"]);
    assert_eq!(days[0]["unpricedTokens"], 0);
    assert_eq!(days[1]["cost"], Value::Null);
    assert_eq!(days[1]["pricedCost"], rows[6]["cost"]);
    assert_eq!(days[1]["unpricedTokens"], 30);
    assert_eq!(all["days"], page["days"], "the same days on every page");

    // The days follow the list's filters.
    let other = core
        .request("sessions", query(json!({"search": "other"})))
        .unwrap();
    let days = other["days"].as_array().unwrap();
    assert_eq!(days.len(), 1);
    assert_eq!(days[0]["start"], iso(midnight));
    assert_eq!(
        (days[0]["sessions"].as_u64(), days[0]["tokens"].as_u64()),
        (Some(1), Some(100))
    );
    assert_eq!(ranked(&days[0]), json!([["t6", 100]]));
    let none = core
        .request("sessions", query(json!({"sources": ["codex"]})))
        .unwrap();
    assert_eq!(none["days"], json!([]));
    // Without a time zone the days are UTC's: the newest session's first.
    let utc = core.request("sessions", json!({"scope":"all"})).unwrap();
    let newest = Utc.timestamp_millis_opt(midnight + (now - midnight) / 2);
    let date = newest.unwrap().date_naive();
    assert_eq!(
        utc["days"][0]["start"],
        iso(date
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis())
    );
    assert!(core
        .request("sessions", json!({"timeZone": "Mars/Olympus"}))
        .is_err());
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

#[test]
fn claude_project_threads_keep_environment_and_identity_separate() {
    let temp = tempfile::tempdir().unwrap();
    let mut backend = fixture(temp.path());
    let now = Utc::now();
    let at = now.to_rfc3339();
    let local_root = temp.path().join(".claude/projects/synthetic");
    fs::create_dir_all(&local_root).unwrap();
    fs::write(
        local_root.join("local.jsonl"),
        format!("{}\n", json!({"sessionId":"local_synthetic01","type":"custom-title","customTitle":"Synthetic same title","timestamp":at})),
    )
    .unwrap();
    let initial = backend.request("snapshot", json!({})).unwrap();
    backend.set_claude_cloud_threads(
        vec![
            crate::clients::claude_cloud::ClaudeCloudThread {
                project_id: "chan_testproject01".into(),
                thread_id: "cmsg_testthread01".into(),
                title: "Synthetic same title".into(),
                project_name: "Synthetic project".into(),
                updated_at: at.clone(),
                status: "working".into(),
                execution: crate::ClaudeProjectExecution::Cloud,
            },
            crate::clients::claude_cloud::ClaudeCloudThread {
                project_id: "chan_testproject01".into(),
                thread_id: "cmsg_testthread02".into(),
                title: "Synthetic same title".into(),
                project_name: "Synthetic project".into(),
                updated_at: (now + chrono::Duration::seconds(1)).to_rfc3339(),
                status: "needs_input".into(),
                execution: crate::ClaudeProjectExecution::Cloud,
            },
            crate::clients::claude_cloud::ClaudeCloudThread {
                project_id: "chan_otherproject01".into(),
                thread_id: "cmsg_testthread03".into(),
                title: "Synthetic same title".into(),
                project_name: "Synthetic project".into(),
                updated_at: (now + chrono::Duration::seconds(2)).to_rfc3339(),
                status: "resolved".into(),
                execution: crate::ClaudeProjectExecution::Cloud,
            },
            crate::ClaudeCloudThread {
                project_id: "chan_testproject01".into(),
                thread_id: "cmsg_localthread01".into(),
                title: "Synthetic same title".into(),
                project_name: "Synthetic project".into(),
                updated_at: (now + chrono::Duration::seconds(3)).to_rfc3339(),
                status: "working".into(),
                execution: crate::ClaudeProjectExecution::Local,
            },
            crate::ClaudeCloudThread {
                project_id: "chan_testproject01".into(),
                thread_id: "cmsg_unknownthread01".into(),
                title: "Synthetic same title".into(),
                project_name: "Synthetic project".into(),
                updated_at: (now + chrono::Duration::seconds(4)).to_rfc3339(),
                status: "working".into(),
                execution: crate::ClaudeProjectExecution::Unknown,
            },
        ],
        now.timestamp_millis(),
    );
    let snapshot = backend.request("snapshot", json!({})).unwrap();
    assert_ne!(initial["sessionsRevision"], snapshot["sessionsRevision"]);
    let snapshot_rows = snapshot["sessions"].as_array().unwrap();
    assert_eq!(snapshot_rows.len(), 6);
    assert!(snapshot_rows
        .iter()
        .any(|row| row["id"] == "local_synthetic01"));
    let page = backend.request("sessions", json!({"scope":"all"})).unwrap();
    assert_eq!(
        page["projectLabels"]["claude-project:chan_testproject01"],
        "Synthetic project (testpr)"
    );
    assert_eq!(
        page["projectLabels"]["claude-project:chan_otherproject01"],
        "Synthetic project (otherp)"
    );
    assert_eq!(page["projectLabels"].as_object().unwrap().len(), 2);
    let rows = page["items"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    let projects = rows
        .iter()
        .filter(|row| row["desktopProject"].is_object())
        .collect::<Vec<_>>();
    assert_eq!(projects.len(), 5);
    assert!(projects
        .windows(2)
        .all(|pair| pair[0]["id"] != pair[1]["id"]));
    assert!(projects
        .iter()
        .all(|row| row["title"] == "Synthetic same title"));
    let first_project_threads = projects
        .iter()
        .filter(|row| row["navigationTarget"]["projectId"] == "chan_testproject01")
        .collect::<Vec<_>>();
    let second_project_threads = projects
        .iter()
        .filter(|row| row["navigationTarget"]["projectId"] == "chan_otherproject01")
        .collect::<Vec<_>>();
    assert_eq!(first_project_threads.len(), 4);
    assert!(first_project_threads
        .iter()
        .all(|row| row["project"] == "claude-project:chan_testproject01"));
    assert_eq!(second_project_threads.len(), 1);
    assert_eq!(
        second_project_threads[0]["project"],
        "claude-project:chan_otherproject01"
    );
    assert!(projects
        .iter()
        .all(|row| row["desktopProject"]["projectLabel"]
            .as_str()
            .unwrap()
            .starts_with("Synthetic project (")));
    assert!(projects
        .iter()
        .all(|row| row["navigationTarget"]["kind"] == "claude-project-thread"));
    let local = projects
        .iter()
        .find(|row| row["desktopProject"]["execution"] == "local")
        .unwrap();
    assert_eq!(local["client"], "Claude Code Desktop");
    assert_eq!(local["desktopProject"]["kindLabel"], "Local");
    let unknown = projects
        .iter()
        .find(|row| row["desktopProject"]["execution"] == "unknown")
        .unwrap();
    assert_eq!(
        unknown["desktopProject"]["kindLabel"],
        "Environment unknown"
    );
    let detail = backend
        .request("session_detail", json!({"id":projects[0]["id"]}))
        .unwrap();
    assert_eq!(
        detail["session"]["navigationTarget"],
        projects[0]["navigationTarget"]
    );
    assert_eq!(detail["session"]["usageStatus"], "unavailable");
    let filtered = backend
        .request(
            "sessions",
            json!({"scope":"all","source":"claude","project":"claude-project:chan_testproject01","sources":["claude"]}),
        )
        .unwrap();
    assert_eq!(filtered["items"].as_array().unwrap().len(), 4);
    let filtered_list = backend
        .request(
            "sessions",
            json!({"scope":"all","projects":["claude-project:chan_otherproject01"]}),
        )
        .unwrap();
    assert_eq!(filtered_list["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        filtered_list["items"][0]["project"],
        "claude-project:chan_otherproject01"
    );
    let cloud_id = projects
        .iter()
        .find(|row| {
            row["desktopProject"]["execution"] == "cloud"
                && row["navigationTarget"]["projectId"] == "chan_testproject01"
                && row["navigationTarget"]["id"] == "cmsg_testthread01"
        })
        .unwrap()["id"]
        .clone();
    let cloud_navigation =
        projects.iter().find(|row| row["id"] == cloud_id).unwrap()["navigationTarget"].clone();
    for (execution, execution_name, kind_label) in [
        (crate::ClaudeProjectExecution::Local, "local", "Local"),
        (
            crate::ClaudeProjectExecution::Unknown,
            "unknown",
            "Environment unknown",
        ),
    ] {
        backend.set_claude_cloud_threads(
            vec![crate::ClaudeCloudThread {
                project_id: "chan_testproject01".into(),
                thread_id: "cmsg_testthread01".into(),
                title: "Synthetic same title".into(),
                project_name: "Synthetic project".into(),
                updated_at: at.clone(),
                status: "working".into(),
                execution,
            }],
            now.timestamp_millis(),
        );
        let reclassified = backend
            .request("session_detail", json!({"id":cloud_id}))
            .unwrap()["session"]
            .clone();
        assert_eq!(reclassified["id"], cloud_id);
        assert_eq!(reclassified["navigationTarget"], cloud_navigation);
        assert_eq!(reclassified["desktopProject"]["execution"], execution_name);
        assert_eq!(reclassified["desktopProject"]["kindLabel"], kind_label);
        assert!(reclassified.get("cloud").is_none());
        assert!(!reclassified["navigationTarget"]
            .as_object()
            .unwrap()
            .contains_key("localId"));
    }
    assert!(backend
        .request("sessions", json!({"scope":"all","source":"codex"}))
        .unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
    backend.enabled.insert("claude".into(), false);
    let disabled = backend.request("sessions", json!({"scope":"all"})).unwrap();
    assert!(disabled["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["desktopProject"].is_null()));
}

#[test]
fn repeated_cloud_unavailability_preserves_sessions_revision() {
    let temp = tempfile::tempdir().unwrap();
    let mut backend = fixture(temp.path());
    let now = Utc::now().timestamp_millis();
    backend.invalidate_claude_cloud_threads(true);
    backend.refresh_sessions_revision(now);
    let revision = backend.sessions_revision;
    backend.invalidate_claude_cloud_threads(true);
    backend.refresh_sessions_revision(now);
    assert_eq!(backend.sessions_revision, revision);
}

#[test]
fn claude_project_status_mapping_invalidation_and_expiry_are_explicit() {
    let temp = tempfile::tempdir().unwrap();
    let mut backend = fixture(temp.path());
    let now = Utc::now();
    let updated_at = now.to_rfc3339();
    let statuses = [
        ("working", "running", "running"),
        ("needs_input", "waiting", "waiting"),
        ("resolved", "completed", "idle"),
        ("archived", "completed", "idle"),
        ("idle", "idle", "idle"),
        ("failed", "failed", "idle"),
        ("other", "unknown", "unverified"),
    ];
    for (index, (input, cloud_status, state)) in statuses.into_iter().enumerate() {
        let id = format!("cmsg_statuscase{index:02}");
        backend.set_claude_cloud_threads(
            vec![crate::ClaudeCloudThread {
                project_id: "chan_statuscase01".into(),
                thread_id: id.clone(),
                title: format!("Synthetic status {index}"),
                project_name: "Synthetic status project".into(),
                updated_at: updated_at.clone(),
                status: input.into(),
                execution: crate::ClaudeProjectExecution::Cloud,
            }],
            now.timestamp_millis(),
        );
        let session = backend
            .request(
                "session_detail",
                json!({"id":format!("claude-project:chan_statuscase01:{id}")}),
            )
            .unwrap()["session"]
            .clone();
        assert_eq!(session["desktopProject"]["status"], cloud_status, "{input}");
        assert_eq!(session["state"], state, "{input}");
    }
    backend.set_claude_cloud_threads(
        vec![crate::ClaudeCloudThread {
            project_id: "chan_statuscase01".into(),
            thread_id: "cmsg_expirycase01".into(),
            title: "Synthetic expiry".into(),
            project_name: "Synthetic status project".into(),
            updated_at: updated_at.clone(),
            status: "working".into(),
            execution: crate::ClaudeProjectExecution::Cloud,
        }],
        now.timestamp_millis(),
    );
    backend.refresh_sessions_revision(now.timestamp_millis());
    let revision = backend.sessions_revision;
    backend.refresh_sessions_revision(now.timestamp_millis() + 90_001);
    assert_ne!(
        backend.sessions_revision, revision,
        "expiry updates session revision"
    );
    backend.invalidate_claude_cloud_threads(false);
    let retained = backend
        .request(
            "session_detail",
            json!({"id":"claude-project:chan_statuscase01:cmsg_expirycase01"}),
        )
        .unwrap()["session"]
        .clone();
    assert_eq!(retained["desktopProject"]["status"], "unknown");
    assert_eq!(retained["state"], "unverified");
    backend.set_claude_cloud_threads(
        vec![crate::ClaudeCloudThread {
            project_id: "chan_statuscase01".into(),
            thread_id: "cmsg_expirycase01".into(),
            title: "Synthetic expiry".into(),
            project_name: "Synthetic status project".into(),
            updated_at,
            status: "working".into(),
            execution: crate::ClaudeProjectExecution::Cloud,
        }],
        now.timestamp_millis(),
    );
    assert_eq!(
        backend.claude_cloud.rows(now.timestamp_millis() + 90_001)[0]["desktopProject"]["status"],
        "unknown"
    );
    backend.invalidate_claude_cloud_threads(true);
    assert!(backend.claude_cloud.rows(now.timestamp_millis()).is_empty());
}

#[test]
fn cloud_metadata_preserves_merged_local_worker_usage_and_navigation() {
    let temp = tempfile::tempdir().unwrap();
    project_worker_fixture(temp.path(), 1, true, true);
    project_worker_fixture(temp.path(), 2, true, true);
    let mut core = fixture(temp.path());
    core.request("snapshot", json!({})).unwrap();
    let key = "claude-project:chan_syntheticproject:cmsg_syntheticproject_thread";
    let before = core.request("session_detail", json!({"id":key})).unwrap();
    let now = Utc::now();
    core.set_claude_cloud_threads(
        vec![crate::ClaudeCloudThread {
            project_id: "chan_syntheticproject".into(),
            thread_id: "cmsg_syntheticproject_thread".into(),
            title: "Synthetic Cloud coordinator".into(),
            project_name: "Synthetic Project".into(),
            updated_at: now.to_rfc3339(),
            status: "working".into(),
            execution: crate::ClaudeProjectExecution::Cloud,
        }],
        now.timestamp_millis(),
    );
    let after = core.request("session_detail", json!({"id":key})).unwrap();
    assert_eq!(after["session"]["desktopProject"]["execution"], "mixed");
    assert_eq!(after["session"]["desktopProject"]["status"], "running");
    assert_eq!(after["session"]["desktopProject"]["openAvailable"], true);
    assert_eq!(
        after["session"]["navigationTarget"],
        before["session"]["navigationTarget"]
    );
    assert_eq!(after["session"]["tokens"], 26);
    assert_eq!(after["usage"], before["usage"]);
    assert_eq!(after["children"].as_array().unwrap().len(), 2);
    assert!(after["session"].get("usageStatus").is_none());
    core.invalidate_claude_cloud_threads(true);
    let cleared = core.request("session_detail", json!({"id":key})).unwrap();
    assert_eq!(cleared["session"]["desktopProject"]["execution"], "local");
    assert_eq!(cleared["usage"], before["usage"]);
}
