use super::*;
use crate::zcode::fixtures::{database, session, NOW};

const TURN: &str = "turn_011fbdd0";

/// A message and its parts, as ZCode 0.15 writes them.
fn message(db: &Connection, sequence: i64, id: &str, info: Value, parts: &[Value]) {
    db.execute(
        "INSERT INTO message(id,session_id,time_created,time_updated,data,sequence) VALUES(?1,'sess_a',?2,?2,?3,?4)",
        rusqlite::params![id, NOW + sequence, info.to_string(), sequence],
    )
    .unwrap();
    for (index, part) in parts.iter().enumerate() {
        db.execute(
            "INSERT INTO part(id,message_id,session_id,time_created,time_updated,data,sequence) VALUES(?1,?2,'sess_a',?3,?3,?4,?5)",
            rusqlite::params![format!("{id}-p{index}"), id, NOW + sequence, part.to_string(), index as i64],
        )
        .unwrap();
    }
}

fn semantics(origin: &str, kind: &str, ui: &str) -> Value {
    json!({"origin": origin, "kind": kind, "uiVisibility": ui, "providerVisibility": "visible", "transcriptVisibility": ui})
}

/// The turn this Mac's ZCode recorded on 2026-11-10, cut down.
fn conversation() -> (tempfile::TempDir, Connection) {
    let (dir, path) = database();
    let db = Connection::open(&path).unwrap();
    session(
        &db,
        "sess_a",
        "Reply with one word: OK",
        None,
        "interactive",
    );
    message(
        &db,
        0,
        "m-prompt",
        json!({"role":"user","time":{"created":NOW},"semantics":semantics("real_user","user_prompt","visible"),"anchor":{"turnId":TURN,"origin":"realUser"}}),
        &[
            json!({"type":"text","text":"请调用 agent-hud 的 list_sessions 工具","time":{"start":NOW,"end":NOW}}),
        ],
    );
    message(
        &db,
        1,
        "m-date",
        json!({"role":"user","time":{"created":NOW + 1},"semantics":semantics("agent_runtime","system_reminder","hidden"),"synthetic":true,"visibility":"model-only","metadata":{"source":"date"},"anchor":{"turnId":TURN,"origin":"synthetic"}}),
        &[json!({"type":"text","text":"Today's date is 2026-11-10.","synthetic":true})],
    );
    message(
        &db,
        2,
        "m-call",
        json!({"role":"assistant","time":{"created":NOW + 2,"completed":NOW + 3},"parentID":"m-prompt","semantics":semantics("agent_runtime","assistant_response","visible"),"anchor":{"turnId":TURN}}),
        &[
            json!({"type":"step-start"}),
            json!({"type":"reasoning","text":"Let me call the list_sessions tool.","time":{"start":NOW + 2}}),
            json!({"type":"tool","callID":"call_823d","tool":"mcp__agent-hud__list_sessions","state":{"status":"completed","input":{},"output":"{\"sessions\":[]}","time":{"start":NOW + 2,"end":NOW + 3}}}),
            json!({"type":"tool","callID":"call_9","tool":"Bash","state":{"status":"error","input":{"command":"false"},"error":"exit 1"}}),
            json!({"type":"step-finish","reason":"tool-calls"}),
        ],
    );
    message(
        &db,
        3,
        "m-reply",
        json!({"role":"assistant","time":{"created":NOW + 4},"parentID":"m-prompt","semantics":semantics("agent_runtime","assistant_response","visible"),"anchor":{"turnId":TURN}}),
        &[
            json!({"type":"step-start"}),
            json!({"type":"text","text":"ZCODE_E2E_OK — 18 个会话。"}),
            json!({"type":"step-finish","reason":"stop"}),
        ],
    );
    message(
        &db,
        4,
        "m-model",
        json!({"role":"assistant","time":{"created":NOW + 5},"semantics":semantics("system","timeline_event","visible")}),
        &[json!({"type":"timeline","timelineType":"model_change"})],
    );
    message(
        &db,
        5,
        "m-failed",
        json!({"role":"assistant","time":{"created":NOW + 6},"parentID":"m-prompt","error":{"name":"AiSdkModelAdapterError","data":{"message":"[1113][余额不足或无可用资源包,请充值。]"}},"semantics":semantics("agent_runtime","assistant_response","visible"),"anchor":{"turnId":"turn_2"}}),
        &[json!({"type":"step-start"})],
    );
    message(
        &db,
        6,
        "m-older",
        json!({"role":"user","time":{"created":NOW + 7}}),
        &[json!({"type":"text","text":"An older ZCode's prompt"})],
    );
    message(
        &db,
        7,
        "m-older-context",
        json!({"role":"user","time":{"created":NOW + 8},"visibility":"model-only","synthetic":true}),
        &[json!({"type":"text","text":"<system-reminder>context</system-reminder>"})],
    );
    message(
        &db,
        8,
        "m-notice",
        json!({"role":"user","time":{"created":NOW + 9},"semantics":semantics("agent_runtime","background_notification","visible"),"synthetic":true}),
        &[json!({"type":"text","text":"Background task finished"})],
    );
    (dir, db)
}

fn rows(db: &Connection) -> Vec<LineItem> {
    let page = messages(db, "zcode:sess_a", 0, 100).unwrap();
    page.lines
        .iter()
        .flat_map(|(place, line)| (ITEMS.read)(line, *place))
        .collect()
}

#[test]
fn a_session_shows_its_prompts_replies_and_tool_calls() {
    let (_dir, db) = conversation();
    let rows = rows(&db);
    let ids = rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "m-prompt-p0",
            "m-call-p1",
            "m-call-p2",
            "m-call-p2:result",
            "m-call-p3",
            "m-call-p3:result",
            "m-reply-p1",
            "m-failed:error",
            "m-older-p0",
            "m-notice-p0",
        ]
    );

    let prompt = &rows[0];
    assert_eq!(prompt.role, LineRole::User);
    assert!(prompt.is_prompt);
    assert_eq!(prompt.origin.as_deref(), Some("human"));
    assert_eq!(
        prompt.text.as_deref(),
        Some("请调用 agent-hud 的 list_sessions 工具")
    );
    assert_eq!(prompt.timestamp, crate::iso(NOW));

    assert_eq!(rows[1].kind, LineKind::Thinking);
    assert_eq!(rows[1].parent.as_deref(), Some("m-prompt"));

    let call = &rows[2];
    assert_eq!(call.role, LineRole::Assistant);
    assert_eq!(call.kind, LineKind::Tool);
    assert_eq!(call.tool_call_id.as_deref(), Some("call_823d"));
    let tool = call.tool.as_ref().unwrap();
    assert_eq!(tool.name, "mcp__agent-hud__list_sessions");
    assert_eq!(tool.arguments, json!({}));
    let result = &rows[3];
    assert_eq!(result.role, LineRole::Tool);
    assert_eq!(result.tool_call_id.as_deref(), Some("call_823d"));
    assert_eq!(result.text.as_deref(), Some("{\"sessions\":[]}"));
    assert_eq!(rows[5].text.as_deref(), Some("exit 1"));

    let reply = &rows[6];
    assert_eq!(reply.kind, LineKind::Text);
    assert_eq!(reply.text.as_deref(), Some("ZCODE_E2E_OK — 18 个会话。"));
    assert!(!reply.is_prompt);

    assert_eq!(
        rows[7].text.as_deref(),
        Some("[1113][余额不足或无可用资源包,请充值。]")
    );
    assert!(rows[8].is_prompt, "an older ZCode's own words");
    let notice = &rows[9];
    assert!(!notice.is_prompt);
    assert_eq!(notice.origin.as_deref(), Some("background_notification"));
    assert!(
        rows.iter().filter(|row| row.is_prompt).count() == 2,
        "the context ZCode hands the model is no prompt"
    );
}

#[test]
fn each_message_names_its_turn_and_the_prompt_starts_it() {
    let (_dir, db) = conversation();
    let page = messages(&db, "sess_a", 0, 100).unwrap();
    let line = |n: usize| &page.lines[n].1;
    let start = (TURNS.start)(line(0)).unwrap();
    assert!(start.whole);
    assert_eq!(start.turn.as_deref(), Some(TURN));
    assert_eq!(start.started, crate::iso(NOW).map(Value::from));
    let reminder = (TURNS.start)(line(1)).unwrap();
    assert!(!reminder.whole);
    assert_eq!(reminder.turn.as_deref(), Some(TURN));
    assert!(reminder.started.is_none());
    assert!(
        (TURNS.start)(line(4)).is_none(),
        "a separator names no turn"
    );
    assert!((TURNS.may_name)(line(0).to_string().as_bytes()));
}

#[test]
fn messages_are_paged_by_their_place() {
    let (_dir, db) = conversation();
    let first = messages(&db, "sess_a", 0, 3).unwrap();
    let places = |lines: &Lines| lines.lines.iter().map(|(at, _)| *at).collect::<Vec<_>>();
    assert_eq!(places(&first), [0, 1, 2]);
    assert_eq!(first.next, 3);
    assert!(!first.complete);
    assert!(!first.earlier);

    let rest = messages(&db, "sess_a", first.next, 100).unwrap();
    assert_eq!(places(&rest), [3, 4, 5, 6, 7, 8]);
    assert_eq!(rest.next, 9);
    assert!(rest.complete);
    assert!(rest.earlier);

    let last = last_messages(&db, "sess_a", 2).unwrap();
    assert_eq!(places(&last), [7, 8]);
    assert_eq!(last.next, 9);
    assert!(last.complete);
    assert!(last.earlier);

    let none = messages(&db, "sess_other", 0, 10).unwrap();
    assert!(none.lines.is_empty());
    assert!(none.complete);
}

#[test]
fn a_new_message_is_read_where_the_last_page_ended() {
    let (_dir, db) = conversation();
    let page = messages(&db, "sess_a", 0, 100).unwrap();
    message(
        &db,
        9,
        "m-next",
        json!({"role":"user","time":{"created":NOW + 10},"semantics":semantics("real_user","user_prompt","visible"),"anchor":{"turnId":"turn_3","origin":"realUser"}}),
        &[json!({"type":"text","text":"Next"})],
    );
    let next = messages(&db, "sess_a", page.next, 100).unwrap();
    assert_eq!(next.lines.len(), 1);
    assert_eq!(next.lines[0].1["id"], "m-next");
}
