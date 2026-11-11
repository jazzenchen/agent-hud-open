//! WorkBuddy: ceilings, metering and account session records.
use super::*;

#[test]
fn workbuddy_ceilings_metering_and_account_session_records() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let wb_home = home.join(".workbuddy");
    let wb_projects = wb_home.join("projects");
    fs::create_dir_all(&wb_projects).unwrap();

    let db_path = wb_home.join("workbuddy.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE sessions (id TEXT PRIMARY KEY, custom_title TEXT, title TEXT);
         CREATE TABLE session_usage (session_id TEXT PRIMARY KEY, used INTEGER, size INTEGER, credit_json TEXT);",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sessions(id, custom_title, title) VALUES (?1, ?2, ?3)",
        ["wb_1", "Sidebar Renamed Session", "Generated Title"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session_usage(session_id, used, size, credit_json) VALUES (?1, ?2, ?3, ?4)",
        ("wb_1", 25000i64, 100000i64, r#"{"entry_a": 12.5}"#),
    )
    .unwrap();
    drop(conn);

    let now_ms = Utc::now().timestamp_millis();
    let transcript = wb_projects.join("wb_1.jsonl");
    fs::write(
        &transcript,
        format!(
            "{}\n{}\n",
            json!({
                "sessionId": "wb_1",
                "timestamp": now_ms - 5000,
                "type": "message",
                "role": "assistant",
                "status": "completed",
                "providerData": {
                    "model": "glm-5",
                    "messageId": "msg_1",
                    "usage": {
                        "input_tokens": 100,
                        "output_tokens": 50
                    }
                }
            }),
            json!({
                "sessionId": "wb_1",
                "timestamp": now_ms - 1000,
                "type": "message",
                "role": "assistant",
                "status": "incomplete",
                "providerData": {
                    "error": {
                        "status": 429,
                        "message": "当前套餐 5 小时用量已达上限，将于 2099-12-31 23:59:59 重置"
                    }
                }
            }),
        ),
    )
    .unwrap();

    let mut backend = AgentHudCore::new(
        vec![SourceRoot {
            path: wb_projects,
            provider: "workbuddy",
        }],
        home.join("app-data"),
    );
    backend.set_account_projection(AccountProjection {
        session_records: vec![json!({
            "source": "cursor",
            "id": "cursor-account:acct1:comp1",
            "title": "Cursor Dashboard Session",
            "model": "claude-3-7-sonnet",
            "key": "ev1:0",
            "at": now_ms - 2000,
            "input": 300,
            "output": 100,
            "read": 0,
            "write": 0,
            "parent": null,
            "archived": false,
        })],
        ..Default::default()
    });

    let snap = backend.request("snapshot", json!({})).unwrap();
    // 150 from WorkBuddy + 400 from Cursor account session = 550
    assert_eq!(snap["totals"]["total"], 550);
    let sessions = snap["sessions"].as_array().unwrap();
    let wb_session = sessions
        .iter()
        .find(|s| s["id"] == "workbuddy:wb_1")
        .expect("WorkBuddy session present");
    assert_eq!(wb_session["title"], "Sidebar Renamed Session");
    assert_eq!(wb_session["contextPercent"], 25.0);

    let cursor_session = sessions
        .iter()
        .find(|s| s["id"] == "cursor-account:acct1:comp1")
        .expect("Cursor account session present");
    assert_eq!(cursor_session["title"], "Cursor Dashboard Session");
    assert_eq!(cursor_session["tokens"], 400);

    let quotas = snap["quotas"].as_array().unwrap();
    assert_eq!(quotas.len(), 1);
    assert_eq!(quotas[0]["id"], "workbuddy:limit:glm-5:5h");
    assert_eq!(quotas[0]["name"], "glm-5 (WorkBuddy) · 5-hour usage limit");
    assert_eq!(quotas[0]["usedPercent"], 100.0);
    assert_eq!(quotas[0]["allModels"], false);
    assert_eq!(quotas[0]["windowDurationSeconds"], 18_000.0);
}

#[test]
fn an_unwritten_workbuddy_transcript_is_neither_counted_nor_listed() {
    let temp = tempfile::tempdir().unwrap();
    let projects = temp.path().join(".workbuddy/projects/folder");
    fs::create_dir_all(&projects).unwrap();
    let now_ms = Utc::now().timestamp_millis();
    fs::write(
        projects.join("wb_1.jsonl"),
        format!(
            "{}\n",
            json!({"sessionId": "wb_1", "timestamp": now_ms - 1000, "type": "message",
                "role": "user", "cwd": "/tmp/w", "content": [{"type": "input_text", "text": "hi"}]})
        ),
    )
    .unwrap();
    // A `-p` run that failed before it wrote a line.
    fs::write(projects.join("wb_empty.jsonl"), "").unwrap();
    let mut backend = AgentHudCore::new(
        vec![SourceRoot {
            path: temp.path().join(".workbuddy/projects"),
            provider: "workbuddy",
        }],
        temp.path().join("app-data"),
    );
    let snap = backend.request("snapshot", json!({})).unwrap();
    let provider = snap["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "workbuddy")
        .unwrap();
    assert_eq!(provider["sessions"], 1);
    let list = backend
        .request("sessions", json!({"scope": "all", "source": "workbuddy"}))
        .unwrap();
    let ids = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["workbuddy:wb_1"]);
}
