use super::page::{archive_tail, TAIL};
use super::*;
use std::io::Write;

fn write(rows: &[Value], filler: usize) -> (tempfile::TempDir, PathBuf, u64) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("log.jsonl");
    let mut file = File::create(&path).unwrap();
    let mut end = 0;
    for (n, row) in rows.iter().enumerate() {
        let line = format!("{row}\n");
        file.write_all(line.as_bytes()).unwrap();
        end += line.len() as u64;
        if n == 0 {
            // Lines that name no turn, past one look-back step, and one
            // line longer than any page reads.
            for _ in 0..filler {
                let line = format!("{}\n", json!({"type":"other","pad":"x".repeat(4096)}));
                file.write_all(line.as_bytes()).unwrap();
                end += line.len() as u64;
            }
            let long = format!(
                "{}\n",
                json!({"type":"user","pad":"y".repeat(MAX_LINE + 10)})
            );
            file.write_all(long.as_bytes()).unwrap();
            end += long.len() as u64;
        }
    }
    (temp, path, end)
}

fn recovered(source: &str, path: &Path, end: u64) -> (Option<String>, Option<Value>) {
    let mut context = Context::default();
    context.recover(turns::of(source), path, end);
    (context.turn_id, context.started)
}

#[test]
fn looking_back_finds_a_claude_prompt_far_behind() {
    let (_temp, path, end) = write(
        &[
            json!({"type":"user","timestamp":"T0","uuid":"u0","promptId":"p1","message":{"role":"user","content":"Go"}}),
            json!({"type":"assistant","timestamp":"T1","message":{"role":"assistant","content":[{"type":"text","text":"Working."}]}}),
            json!({"type":"user","timestamp":"T2","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"x","content":"ok"}]}}),
        ],
        600,
    );
    assert_eq!(
        recovered("claude", &path, end),
        (Some("p1".into()), Some(json!("T0")))
    );
}

#[test]
fn looking_back_finds_when_a_codex_turn_began() {
    let (_temp, path, end) = write(
        &[
            json!({"type":"event_msg","timestamp":"T0","payload":{"type":"task_started","turn_id":"t1"}}),
            json!({"type":"response_item","timestamp":"T1","payload":{"type":"message","turn_id":"t1"}}),
        ],
        600,
    );
    assert_eq!(
        recovered("codex", &path, end),
        (Some("t1".into()), Some(json!("T0")))
    );
}

#[test]
fn looking_back_stops_at_the_turn_before() {
    let (_temp, path, end) = write(
        &[
            json!({"type":"event_msg","timestamp":"T0","payload":{"type":"task_started","turn_id":"t0"}}),
            json!({"type":"response_item","timestamp":"T1","payload":{"type":"message","turn_id":"t1"}}),
        ],
        0,
    );
    assert_eq!(recovered("codex", &path, end), (Some("t1".into()), None));
}

#[test]
fn a_line_at_the_file_start_counts() {
    let (_temp, path, end) = write(
        &[
            json!({"type":"user","timestamp":"T0","uuid":"u0","promptId":"p1","message":{"role":"user","content":"Go"}}),
        ],
        0,
    );
    assert_eq!(
        recovered("claude", &path, end).0,
        Some("p1".into()),
        "the long line after it is skipped, the prompt still counts"
    );
    let only = path.with_file_name("only.jsonl");
    let row = format!(
        "{}\n",
        json!({"type":"user","timestamp":"T0","uuid":"u0","promptId":"p1","message":{"role":"user","content":"Go"}})
    );
    fs::write(&only, &row).unwrap();
    assert_eq!(
        recovered("claude", &only, row.len() as u64).0,
        Some("p1".into())
    );
}

/// The client that writes these logs.
const HARNESS: &str = agent_hud_agents::deepseek::AGENT.id;

/// A DeepSeek Harness log's text: two turns, the second a message from
/// another agent answered after its reasoning, with `before` and `after`
/// lines that name no turn around that message.
fn harness_log(before: usize, after: usize) -> String {
    let pad = |n: usize| json!({"type":"session-log-deepseek/delivery-accepted","seq":n,"time":1793127112000_i64,"data":{"pad":"x".repeat(4096)}});
    let mut rows = vec![
        json!({"type":"session","version":4,"id":"session-f8d6","createdAt":1793123855625_i64,"cwd":"/w"}),
        json!({"type":"turn/start","seq":5,"time":1793124207977_i64,"data":{"turn":1}}),
        json!({"type":"user/message","seq":8,"time":1793124208006_i64,"data":{"content":[{"type":"text","text":"Ask me one question."}],"source":{"kind":"user"},"role":"user"}}),
        json!({"type":"assistant/message","seq":25,"time":1793124282761_i64,"data":{"turn":1,"step":2,"message":{"role":"assistant","content":[{"type":"reasoning","text":""},{"type":"text","text":"You chose Blue."}]}}}),
        json!({"type":"turn/end","seq":27,"time":1793124282770_i64,"data":{"turn":1,"reason":{"kind":"completed"}}}),
    ];
    rows.extend((0..before).map(pad));
    rows.extend([
        json!({"type":"turn/start","seq":44,"time":1793127112393_i64,"data":{"turn":3}}),
        json!({"type":"user/message","seq":48,"time":1793127112417_i64,"data":{"content":[{"type":"text","text":"[Agent HUD · from Claude Code · Test · m-b0bfc3510306]\n\nWhich model are you?"}],"source":{"kind":"user","rpcId":"m-b0bfc3510306"},"role":"user"}}),
        json!({"type":"user/message","seq":50,"time":1793127112417_i64,"data":{"content":[{"type":"text","text":"Time sampled while preparing turn 3, step 1"}],"source":{"kind":"time-context"},"role":"user"}}),
    ]);
    rows.extend((0..after).map(pad));
    rows.extend([
        json!({"type":"assistant/message","seq":54,"time":1793127114066_i64,"data":{"turn":3,"step":1,"message":{"role":"assistant","content":[{"type":"reasoning","text":"The user wants one sentence."},{"type":"text","text":"收到了你的消息；我是 DeepSeek Harness 中的 deepseek-flash 模型。"}]}}}),
        json!({"type":"turn/end","seq":56,"time":1793127114067_i64,"data":{"turn":3,"reason":{"kind":"completed"}}}),
    ]);
    rows.iter().map(|row| format!("{row}\n")).collect()
}

/// `text` compressed as Harness writes it: a frame every few lines.
fn compress(text: &str) -> Vec<u8> {
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();
    lines
        .chunks(7)
        .flat_map(|chunk| zstd::stream::encode_all(chunk.concat().as_bytes(), 3).unwrap())
        .collect()
}

/// The same text as a plain and a compressed DeepSeek log.
fn both(text: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let plain = temp.path().join("session.v4.jsonl");
    let archive = temp.path().join("session.v4.jsonl.zstd");
    fs::write(&plain, text).unwrap();
    fs::write(&archive, compress(text)).unwrap();
    (temp, plain, archive)
}

fn tail(path: &Path, limit: usize) -> Result<Value, String> {
    page(
        path,
        HARNESS,
        "session",
        0,
        limit,
        true,
        &mut compressed::Archives::default(),
        &mut Contexts::default(),
    )
}

fn reply(page: &Value) -> &Value {
    page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|row| row["role"] == "assistant")
        .unwrap_or_else(|| panic!("no reply in {page:#}"))
}

#[test]
fn a_compressed_tail_is_the_tail_its_plain_text_gives() {
    // The message's turn starting inside the tail, before it, and a log
    // shorter than a tail.
    for (before, after) in [(600, 0), (0, 600), (0, 0)] {
        let text = harness_log(before, after);
        let (_temp, plain, archive) = both(&text);
        for limit in [200, 1] {
            let mut compressed = tail(&archive, limit).unwrap();
            let mut same = tail(&plain, limit).unwrap();
            // A plain tail's budget runs out right at the end, before
            // it reads that nothing follows.
            assert_eq!(compressed["messagesComplete"], true);
            compressed["messagesComplete"].take();
            same["messagesComplete"].take();
            assert_eq!(compressed, same);
            assert_eq!(compressed["endMessageCursor"], text.len());
            assert_eq!(
                compressed["messagesTruncatedBefore"],
                text.len() as u64 > TAIL
            );
            let reply = reply(&compressed);
            assert_eq!(
                reply["text"],
                "收到了你的消息；我是 DeepSeek Harness 中的 deepseek-flash 模型。"
            );
            assert_eq!(reply["turnId"], "3", "{before} {after}");
            assert_eq!(reply["turnStartedAt"], 1793127112393_i64);
        }
        let rows = tail(&archive, 200).unwrap()["messages"].clone();
        let message = rows
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["text"].as_str().unwrap().contains("· m-b0bfc3510306]"));
        if after == 0 {
            let message = message.expect("the message lies within the tail");
            assert_eq!(message["isPrompt"], true);
            assert_eq!(message["turnId"], "3");
        }
    }
}

#[test]
fn a_compressed_tail_ends_before_a_frame_still_being_written() {
    let text = harness_log(0, 0);
    let (_temp, _plain, archive) = both(&text);
    let mut bytes = compress(&text);
    let next = zstd::stream::encode_all(
        format!("{}\n", json!({"type":"turn/start","data":{"turn":4}})).as_bytes(),
        3,
    )
    .unwrap();
    bytes.extend_from_slice(&next[..next.len() / 2]);
    assert_eq!(
        zstd::stream::decode_all(&bytes[..]).unwrap_err().kind(),
        std::io::ErrorKind::UnexpectedEof
    );
    fs::write(&archive, bytes).unwrap();
    let page = tail(&archive, 200).unwrap();
    assert_eq!(page["endMessageCursor"], text.len());
    assert_eq!(reply(&page)["turnId"], "3");
}

#[test]
fn a_compressed_transcript_past_its_reach_has_no_tail() {
    let (_temp, _plain, archive) = both(&harness_log(600, 0));
    let mut contexts = Contexts::default();
    assert!(archive_tail(&archive, HARNESS, 200, &mut contexts, 4096).is_err());
    assert!(archive_tail(&archive, HARNESS, 200, &mut contexts, TAIL).is_ok());
}

#[test]
fn a_compressed_log_pages_forward_to_its_reply() {
    let text = harness_log(600, 0);
    let (_temp, _plain, archive) = both(&text);
    let mut archives = compressed::Archives::default();
    let mut contexts = Contexts::default();
    let mut cursor = 0;
    let mut rows = Vec::new();
    loop {
        let page = page(
            &archive,
            HARNESS,
            "session",
            cursor,
            100,
            false,
            &mut archives,
            &mut contexts,
        )
        .unwrap();
        assert_eq!(page["messagesPending"], false);
        rows.extend(page["messages"].as_array().unwrap().iter().cloned());
        match page["nextMessageCursor"].as_u64() {
            Some(next) => cursor = next,
            None => break,
        }
    }
    let seen = rows
        .iter()
        .map(|row| {
            (
                row["role"].as_str().unwrap(),
                row["turnId"].as_str().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        seen,
        [
            ("user", "1"),
            ("assistant", "1"),
            ("user", "3"),
            ("user", "3"),
            ("assistant", "3")
        ]
    );
    assert_eq!(
        rows.iter().filter(|row| row["isPrompt"] == true).count(),
        2,
        "Harness's context is no prompt"
    );
}

#[test]
fn a_zcode_session_pages_its_messages_from_the_database() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("db.sqlite");
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, data TEXT);
             CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
        )
        .unwrap();
    drop(connection);
    // An empty conversation still pages: no messages, nothing pending.
    let page = super::page(
        &db,
        "zcode",
        "zcode:sess_1",
        0,
        100,
        true,
        &mut compressed::Archives::default(),
        &mut Contexts::default(),
    )
    .unwrap();
    assert_eq!(page["messages"], serde_json::json!([]));
    assert_eq!(page["messagesPending"], false);
}
