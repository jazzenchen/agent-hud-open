use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

#[test]
fn qoder_session_details_include_runtime_segments_without_double_counting_the_transcript() {
    const ID: &str = "11111111-1111-4111-8111-111111111111";
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let config = home.join(".qoder");
    let segments = config.join(format!("logs/sessions/-fixture/{ID}/segments"));
    let first = segments.join("2026-11-23T10-00-first.jsonl");
    let second = segments.join("2026-11-23T10-00-second.jsonl");
    fs::create_dir_all(&segments).unwrap();
    fs::write(
        &first,
        concat!(
            r#"{"ts":"2026-11-23T10:00:00Z","type":"session.phase.finished","data":{"phase":"transcript.writer.initialized","success":true,"cwd":"/fixture"}}"#, "\n",
            r#"{"ts":"2026-11-23T10:00:01Z","type":"input.prompt.submitted","turn_id":"first","data":{"is_meta":false,"is_subagent":false,"skip_initial_user_message":true,"text_preview":"First test prompt"}}"#, "\n",
            r#"{"ts":"2026-11-23T10:00:02Z","type":"turn.started","turn_id":"first","data":{"is_subagent":false,"model":"auto"}}"#, "\n",
            r#"{"ts":"2026-11-23T10:00:03Z","type":"turn.finished","turn_id":"first","data":{"reason":"end_turn","input_tokens":100,"cache_read_input_tokens":80,"output_tokens":7}}"#, "\n",
        ),
    )
    .unwrap();
    fs::write(
        &second,
        concat!(
            r#"{"ts":"2026-11-23T10:00:04Z","type":"input.prompt.submitted","turn_id":"second","data":{"is_meta":false,"is_subagent":false,"skip_initial_user_message":true,"text_preview":"Second test prompt"}}"#, "\n",
            r#"{"ts":"2026-11-23T10:00:05Z","type":"turn.started","turn_id":"second","data":{"model":"auto"}}"#, "\n",
            r#"{"ts":"2026-11-23T10:00:06Z","type":"turn.finished","turn_id":"second","data":{"reason":"end_turn","input_tokens":200,"cache_read_input_tokens":180,"output_tokens":8}}"#, "\n",
        ),
    )
    .unwrap();
    let mut engine = Command::new(env!("CARGO_BIN_EXE_agent-hud-engine"))
        .env_clear()
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("AGENTHUD_DATA_DIR", temp.path().join("data"))
        .env("QODER_CONFIG_DIR", &config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = engine.stdin.take().unwrap();
    let mut stdout = BufReader::new(engine.stdout.take().unwrap());
    let mut request = |method, params| {
        writeln!(stdin, "{}", json!({"id":1,"method":method,"params":params})).unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert!(reply["error"].is_null(), "{reply}");
        reply["result"].clone()
    };
    request("snapshot", json!({}));
    let detail = request("session_detail", json!({"id": ID}));
    assert_eq!(detail["session"]["id"], ID);
    assert_eq!(detail["session"]["state"], "idle");
    assert_eq!(detail["usage"]["input"], 40);
    assert_eq!(detail["usage"]["cacheRead"], 260);
    assert_eq!(detail["usage"]["output"], 15);
    assert_eq!(detail["usage"]["total"], 315);
    assert_eq!(detail["turns"][0]["output"], 7);
    assert_eq!(detail["turns"][1]["output"], 8);
    assert_eq!(detail["messages"][0]["text"], "First test prompt");
    assert_eq!(detail["messages"][1]["text"], "Second test prompt");
    assert!(detail["messages"][2].is_null());
    assert!(detail["messageNotice"].is_string());

    let project = config.join("projects/-fixture");
    fs::create_dir_all(&project).unwrap();
    let transcript = project.join(format!("{ID}.jsonl"));
    let assistant = json!({"type":"assistant","sessionId":ID,
        "timestamp":"2026-11-23T10:00:06Z","message":{"id":"m1","role":"assistant",
        "model":"auto","content":[{"type":"text","text":"Full reply"}],
        "usage":{"input_tokens":100,"output_tokens":6}}});
    fs::write(&transcript, format!("{assistant}\n")).unwrap();
    request("refresh", json!({}));
    let detail = request("session_detail", json!({"id": ID}));
    assert_eq!(detail["usage"]["total"], 106);
    assert_eq!(detail["messages"][0]["text"], "Full reply");

    fs::remove_file(transcript).unwrap();
    request("refresh", json!({}));
    let detail = request("session_detail", json!({"id": ID}));
    assert_eq!(detail["usage"]["total"], 315);

    let removed = second.with_extension("removed");
    fs::rename(&second, &removed).unwrap();
    request("refresh", json!({}));
    let list = request("sessions", json!({"scope":"all", "source":"qoder"}));
    let rows = list["items"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], ID);
    let detail = request("session_detail", json!({"id": ID}));
    assert_eq!(detail["session"]["logPath"], json!(first));
    assert_eq!(detail["usage"]["total"], 315);
    assert_eq!(detail["turns"].as_array().unwrap().len(), 2);
    assert_eq!(detail["messages"].as_array().unwrap().len(), 1);
    assert_eq!(detail["messages"][0]["text"], "First test prompt");

    fs::rename(&removed, &second).unwrap();
    request("refresh", json!({}));
    let detail = request("session_detail", json!({"id": ID}));
    assert_eq!(detail["session"]["logPath"], json!(second));
    assert_eq!(detail["usage"]["total"], 315);
    assert_eq!(detail["messages"].as_array().unwrap().len(), 2);
    drop(stdin);
    assert!(engine.wait().unwrap().success());
}
