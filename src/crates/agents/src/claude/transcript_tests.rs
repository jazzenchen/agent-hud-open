use super::*;
use crate::{transcript::metering, LiveLog, Log};
use std::{fs, path::Path};

fn write_desktop_tab(
    base: &Path,
    folder: &str,
    stem: &str,
    cli_id: &str,
    archived: bool,
    spawn: Option<Value>,
) {
    let account = base.join(folder).join("org/acct");
    fs::create_dir_all(&account).unwrap();
    let mut metadata = json!({
        "sessionId": stem,
        "cliSessionId": cli_id,
        "isArchived": archived
    });
    if let Some(spawn) = spawn {
        metadata["remoteControlSpawn"] = spawn;
    }
    fs::write(account.join(format!("{stem}.json")), metadata.to_string()).unwrap();
}

fn assistant(id: Option<&str>, request: Option<&str>, model: &str, usage: Value) -> Value {
    let mut message = json!({"role":"assistant","model":model,"usage":usage,"content":[{"type":"text","text":"ok"}]});
    if let Some(id) = id {
        message["id"] = json!(id);
    }
    let mut line = json!({"type":"assistant","sessionId":"s1","timestamp":"2026-10-21T08:00:00Z","message":message});
    if let Some(request) = request {
        line["requestId"] = json!(request);
    }
    line
}

#[test]
fn a_managed_project_wake_does_not_become_the_session_title() {
    let mut log = Log::new("s1.jsonl".into(), "claude");
    let line = json!({
        "type":"user",
        "sessionId":"s1",
        "timestamp":"2026-11-03T08:00:00Z",
        "projectsUserTurn":true,
        "textNotTyped":true,
        "promptSource":"sdk",
        "entrypoint":"claude-desktop",
        "isSidechain":false,
        "message":{"role":"user","content":"<wake><project id=\"chan_abcdef123456\" type=\"project\"><thread ts=\"cmsg_abcdef123456_789\">"}
    });
    log.parse_claude(&line);

    assert_eq!(log.title(), "Claude Code");
    assert_eq!(log.meter_marks.len(), 1, "the wake still marks real work");
}

#[test]
fn thinking_tokens_count_as_reasoning_within_the_output() {
    let mut log = Log::new("s1.jsonl".into(), "claude");
    log.parse(&assistant(Some("m1"), None, "claude-fable-5", json!({"input_tokens":2,"output_tokens":448,"cache_read_input_tokens":100,"output_tokens_details":{"thinking_tokens":295}})));
    assert_eq!(log.usage.output, 153);
    assert_eq!(log.usage.reasoning, 295);
    assert_eq!(log.usage.total(), 2 + 448 + 100);
    let event = log.meter_events.values().next().unwrap();
    assert_eq!((event.usage.output, event.usage.reasoning), (153, 295));
    assert_eq!(
        log.turns[0]["output"], 448,
        "a turn's output keeps its thinking"
    );
}

#[test]
fn a_response_counts_its_whole_output_when_earlier_lines_had_part_of_it() {
    // Builds before about 2026-09 wrote the first blocks' lines with the
    // output counted so far; only the last line holds the whole count.
    let mut log = Log::new("s1.jsonl".into(), "claude");
    let line = |output, thinking: Option<u64>| {
        let mut usage = json!({"input_tokens":2,"output_tokens":output,"cache_read_input_tokens":57680,"cache_creation_input_tokens":4412});
        if let Some(thinking) = thinking {
            usage["output_tokens_details"] = json!({"thinking_tokens":thinking});
        }
        assistant(Some("m1"), Some("req_1"), "claude-fable-5", usage)
    };
    log.parse(&line(9, None));
    log.parse(&line(9, None));
    log.parse(&line(448, Some(295)));
    assert_eq!(log.usage.output + log.usage.reasoning, 448);
    assert_eq!(log.usage.total(), 2 + 448 + 57680 + 4412);
    // A first line that reported more than the final non-thinking share
    // still counts the response's output once.
    let mut log = Log::new("s1.jsonl".into(), "claude");
    log.parse(&line(300, None));
    log.parse(&line(448, Some(295)));
    assert_eq!((log.usage.output, log.usage.reasoning), (153, 295));
}

#[test]
fn a_response_without_a_message_id_counts_once_by_request_or_by_line() {
    let mut log = Log::new("s1.jsonl".into(), "claude");
    let usage = json!({"input_tokens":10,"output_tokens":3});
    log.parse(&assistant(
        None,
        Some("req_1"),
        "claude-fable-5",
        usage.clone(),
    ));
    log.parse(&assistant(
        None,
        Some("req_1"),
        "claude-fable-5",
        usage.clone(),
    ));
    assert_eq!(log.usage.total(), 13, "lines of one request count once");
    log.meter_line_start = 900;
    log.parse(&assistant(None, None, "claude-fable-5", usage.clone()));
    log.meter_line_start = 1200;
    log.parse(&assistant(None, None, "claude-fable-5", usage));
    assert_eq!(
        log.usage.total(),
        39,
        "a line with neither id counts by its place in the log"
    );
    let mut keys = log.meter_events.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    let mut expected = ["r:req_1", "o:900", "o:1200"]
        .map(metering::event_key)
        .to_vec();
    expected.sort();
    assert_eq!(keys, expected);
}

#[test]
fn the_session_model_is_the_one_that_wrote_the_most_output() {
    let mut log = Log::new("s1.jsonl".into(), "claude");
    log.parse(&assistant(
        Some("m1"),
        None,
        "claude-opus-5",
        json!({"input_tokens":1,"output_tokens":900}),
    ));
    log.parse(&assistant(
        Some("m2"),
        None,
        "claude-fable-5",
        json!({"input_tokens":1,"output_tokens":100}),
    ));
    log.parse(&assistant(
        Some("m3"),
        None,
        "<synthetic>",
        json!({"input_tokens":1,"output_tokens":5000}),
    ));
    assert_eq!(
        log.model, "claude-opus-5",
        "the latest line does not name the session's model; a synthetic line never does"
    );
    let synthetic = log.meter_events.get(&metering::event_key("m:m3")).unwrap();
    assert_eq!(
        synthetic.model, "",
        "a synthetic line's call has no model (Unknown)"
    );
    let mut only = Log::new("s2.jsonl".into(), "claude");
    only.parse(&assistant(
        Some("m1"),
        None,
        "<synthetic>",
        json!({"input_tokens":0,"output_tokens":0}),
    ));
    assert_eq!(only.model, "Unknown");
}

#[test]
fn a_limit_reset_reads_seconds_or_milliseconds() {
    let reset = |at: Value| {
        let mut live = LiveLog::default();
        live.claude(&json!({"type":"user","timestamp":"2026-10-21T08:00:00Z","message":{"role":"user","content":"go"}}));
        live.claude(&json!({"type":"assistant","timestamp":"2026-10-21T08:00:05Z","isApiErrorMessage":true,"error":"rate_limit","quotaLimits":{"resetsAt":at},"message":{"role":"assistant","model":"<synthetic>","content":[]}}));
        live.turns.back().unwrap().limited_until
    };
    assert_eq!(reset(json!(1_792_000_000)), Some(1_792_000_000_000));
    assert_eq!(reset(json!(1_792_000_000_000_i64)), Some(1_792_000_000_000));
}

#[test]
fn a_narration_is_an_item_of_the_agents_text() {
    use base64::Engine;
    let message = |kind: &str| {
        let inner = [&[0x42, kind.len() as u8][..], kind.as_bytes()].concat();
        let middle = [&[0x0a, inner.len() as u8][..], &inner].concat();
        [&[0x12, middle.len() as u8][..], &middle].concat()
    };
    let signature =
        |kind: &str| base64::engine::general_purpose::STANDARD_NO_PAD.encode(message(kind));
    let line = |kind: &str, words: &str| json!({"type":"assistant","uuid":"a1","timestamp":"2026-10-25T14:48:37Z","message":{"role":"assistant","id":"m1","content":[{"type":"thinking","thinking":words,"signature":signature(kind)}]}});
    let said = (ITEMS.read)(
        &line("narration", "I've pulled up the last seven turns."),
        0,
    );
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0].role, items::LineRole::Assistant);
    assert_eq!(said[0].kind, items::LineKind::Text);
    assert_eq!(
        said[0].text.as_deref(),
        Some("I've pulled up the last seven turns.")
    );
    assert!(
        (ITEMS.read)(&line("thinking", "private reasoning"), 0).is_empty(),
        "reasoning is no text"
    );
}

#[test]
fn a_narration_is_the_agents_words_not_its_thinking() {
    use base64::Engine;
    // field 2 { field 1 { field 8 "narration" } }
    let message = |kind: &str| {
        let inner = [&[0x42, kind.len() as u8][..], kind.as_bytes()].concat();
        let middle = [&[0x0a, inner.len() as u8][..], &inner].concat();
        [&[0x12, middle.len() as u8][..], &middle].concat()
    };
    let signature =
        |kind: &str| base64::engine::general_purpose::STANDARD_NO_PAD.encode(message(kind));
    assert_eq!(
        signature_kind(&signature("narration")).as_deref(),
        Some("narration")
    );
    assert_eq!(signature_kind("not base64!"), None);
    let mut live = LiveLog::new(Path::new("s1.jsonl"), "claude");
    live.claude(&json!({"type":"user","timestamp":"2026-10-21T08:00:00Z","message":{"role":"user","content":"go"}}));
    live.claude(&json!({"type":"assistant","timestamp":"2026-10-21T08:00:05Z","message":{"role":"assistant","model":"claude-fable-5","id":"m1","content":[{"type":"thinking","thinking":"Reading the build log now.","signature":signature("narration")}]}}));
    assert_eq!(
        live.activity.as_ref().unwrap().line.as_deref(),
        Some("text")
    );
    assert_eq!(
        live.turns.back().unwrap().message.as_deref(),
        Some("Reading the build log now.")
    );
    live.claude(&json!({"type":"assistant","timestamp":"2026-10-21T08:00:06Z","message":{"role":"assistant","model":"claude-fable-5","id":"m2","content":[{"type":"thinking","thinking":"private reasoning","signature":signature("thinking")}]}}));
    assert_eq!(
        live.activity.as_ref().unwrap().line.as_deref(),
        Some("thinking")
    );
    assert_eq!(
        live.turns.back().unwrap().message.as_deref(),
        Some("Reading the build log now.")
    );
}

#[test]
fn the_next_step_box_keeps_the_buttons_the_app_draws() {
    let ask = |actions: Value| {
        let input = json!({"recap":"Done.","ask":"Commit it?","actions":actions});
        claude_desktop_ask("mcp__ccd_turn__wrap_up", &input, "t1", None, 0).map(|ask| {
            ask.options
                .into_iter()
                .map(|option| (option.label, option.message))
                .collect::<Vec<_>>()
        })
    };
    let commit = ("Commit".to_owned(), None);
    // Unknown actions are left out before the first two are taken.
    assert_eq!(
        ask(
            json!([{"action":"commit","label":"Ship"},{"action":"bogus"},{"label":"Run tests","message":"Run the tests"}])
        ),
        Some(vec![
            commit.clone(),
            ("Run tests".into(), Some("Run the tests".into()))
        ]),
    );
    // A session offered as an action takes a place but draws no button.
    assert_eq!(
        ask(json!([{"action":"commit"},{"action":"new_session"},{"label":"Run"}])),
        Some(vec![commit.clone()])
    );
    // A reply that would mention, run a command or hide its words is dropped.
    assert_eq!(
        ask(json!([{"action":"commit"},{"label":"Ping @bob"},{"label":"Go","message":"/deploy"}])),
        Some(vec![commit.clone()]),
    );
    // Control characters and runs of spaces are cleaned from what is shown.
    assert_eq!(
        ask(json!([{"label":"Run\u{0007}   now"}])),
        Some(vec![("Run now".into(), None)]),
    );
    // A box whose first button cannot be drawn shows only when blocked.
    assert_eq!(ask(json!([{"action":"bogus"},{"action":"commit"}])), None);
    let offer = claude_desktop_ask(
        "mcp__ccd_session__start_session",
        &json!({"initiation":"offer","title":"Split the parser","initial_message":"Work on it"}),
        "t2",
        None,
        0,
    )
    .unwrap();
    assert!(
        offer.recap.is_none(),
        "an offer card shows its title and buttons only"
    );
}

#[test]
fn the_desktop_index_also_reads_the_folders_the_platform_names() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("AppData/Roaming/Claude");
    let account = base.join("claude-code-sessions/org/acct");
    fs::create_dir_all(&account).unwrap();
    let cli = "66666666-6666-4666-8666-666666666666";
    let tab = "local_88888888-8888-4888-8888-888888888888";
    fs::write(
        account.join(format!("{tab}.json")),
        json!({"sessionId":tab,"cliSessionId":cli,"isArchived":false}).to_string(),
    )
    .unwrap();
    let mut index = ClaudeDesktopIndex::default();
    index.set_bases(vec![base]);
    assert!(index.refresh(&[]));
    assert_eq!(
        index.targets.get(cli),
        Some(&json!({"kind":"claude-desktop","id":tab}))
    );
}

#[test]
fn project_child_metadata_distinguishes_standalone_invalid_archived_and_cowork_tabs() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("AppData/Roaming/Claude");
    let standalone = "66666666-6666-4666-8666-666666666661";
    let valid = "66666666-6666-4666-8666-666666666662";
    let invalid = "66666666-6666-4666-8666-666666666663";
    let missing = "66666666-6666-4666-8666-666666666664";
    let archived = "66666666-6666-4666-8666-666666666665";
    let cowork = "66666666-6666-4666-8666-666666666666";
    write_desktop_tab(
        &base,
        "claude-code-sessions",
        "local_88888888-8888-4888-8888-888888888881",
        standalone,
        false,
        None,
    );
    write_desktop_tab(
        &base,
        "claude-code-sessions",
        "local_88888888-8888-4888-8888-888888888882",
        valid,
        false,
        Some(json!({"projectThreadChild":true,"ccrSessionId":"cse_synthetic01"})),
    );
    write_desktop_tab(
        &base,
        "claude-code-sessions",
        "local_88888888-8888-4888-8888-888888888883",
        invalid,
        false,
        Some(json!({"projectThreadChild":true,"ccrSessionId":"local_synthetic01"})),
    );
    write_desktop_tab(
        &base,
        "claude-code-sessions",
        "local_88888888-8888-4888-8888-888888888884",
        missing,
        false,
        Some(json!({"projectThreadChild":true})),
    );
    write_desktop_tab(
        &base,
        "claude-code-sessions",
        "local_88888888-8888-4888-8888-888888888885",
        archived,
        true,
        Some(json!({"projectThreadChild":true,"ccrSessionId":"cse_synthetic02"})),
    );
    write_desktop_tab(
        &base,
        "local-agent-mode-sessions",
        "local_88888888-8888-4888-8888-888888888886",
        cowork,
        false,
        Some(json!({"projectThreadChild":true,"ccrSessionId":"cse_synthetic03"})),
    );

    let mut index = ClaudeDesktopIndex::default();
    index.set_bases(vec![base]);
    assert!(index.refresh(&[]));
    assert!(!index.project_workers.contains_key(standalone));
    assert_eq!(
        index.project_workers.get(valid),
        Some(&ClaudeDesktopProjectWorker::Identified {
            ccr_session_id: "cse_synthetic01".into()
        })
    );
    assert_eq!(
        index.project_workers.get(invalid),
        Some(&ClaudeDesktopProjectWorker::Unknown)
    );
    assert_eq!(
        index.project_workers.get(missing),
        Some(&ClaudeDesktopProjectWorker::Unknown)
    );
    assert!(!index.project_workers.contains_key(archived));
    assert!(index.archived.contains(archived));
    assert!(!index.project_workers.contains_key(cowork));
}

#[test]
fn conflicting_active_project_child_records_are_unknown() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("AppData/Roaming/Claude");
    let cli = "66666666-6666-4666-8666-666666666677";
    write_desktop_tab(
        &base,
        "claude-code-sessions",
        "local_88888888-8888-4888-8888-888888888887",
        cli,
        false,
        Some(json!({"projectThreadChild":true,"ccrSessionId":"cse_synthetic11"})),
    );
    write_desktop_tab(
        &base,
        "claude-code-sessions",
        "local_88888888-8888-4888-8888-888888888888",
        cli,
        false,
        Some(json!({"projectThreadChild":true,"ccrSessionId":"cse_synthetic12"})),
    );

    let mut index = ClaudeDesktopIndex::default();
    index.set_bases(vec![base]);
    assert!(index.refresh(&[]));
    assert_eq!(
        index.project_workers.get(cli),
        Some(&ClaudeDesktopProjectWorker::Unknown)
    );
    assert!(!index.targets.contains_key(cli));
}
