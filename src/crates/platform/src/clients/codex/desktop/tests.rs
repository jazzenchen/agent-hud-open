use super::*;
use agent_hud_agents::codex::AGENT;
use serde_json::json;
use std::{
    fs,
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
};

const THREAD: &str = "00000000-0000-4000-8000-000000000001";
fn permission(connector: &str, persist: Value) -> Value {
    json!({"id":7,"method":"mcpServer/elicitation/request","params":{"threadId":THREAD,"turnId":"turn-fixture","mode":"form","_meta":{"codex_approval_kind":"mcp_tool_call","connector_id":connector,"persist":persist,"tool_params":{"app":"com.example.Fixture","origin":"https://fixture.invalid"},"tool_name":"access_browser_origin"}}})
}
fn snapshot(requests: Vec<Value>) -> Value {
    json!({"cwd":"/fixture","requests":requests,"currentPermissions":{"approvalsReviewer":"user","approvalPolicy":"on-request","sandboxPolicy":{"type":"workspaceWrite"}},"turns":[]})
}

#[test]
fn scopes_and_browser_reviewer_match_source() {
    let state = snapshot(vec![permission(
        "computer-use",
        json!(["session", "always"]),
    )]);
    let found = pending::parse(&state, THREAD, "owner").unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].view["allowsAlways"], true);
    assert_eq!(
        reply::answer(&found[0], &json!({"kind":"allow"}), &state)
            .unwrap()
            .1["_meta"]["persist"],
        "session"
    );
    assert_eq!(
        reply::answer(&found[0], &json!({"kind":"always"}), &state)
            .unwrap()
            .1["_meta"]["persist"],
        "always"
    );
    assert_eq!(
        reply::answer(&found[0], &json!({"kind":"deny"}), &state)
            .unwrap()
            .1,
        json!({"action":"decline","content":null,"_meta":null})
    );
    assert!(pending::parse(
        &snapshot(vec![permission("computer-use", json!(["unknown"]))]),
        THREAD,
        "owner"
    )
    .unwrap()
    .is_empty());
    let mut browser = snapshot(vec![permission("browser-use", Value::Null)]);
    let found = pending::parse(&browser, THREAD, "owner").unwrap();
    assert_eq!(found.len(), 1);
    assert!(reply::answer(&found[0], &json!({"kind":"allow"}), &browser)
        .unwrap()
        .1["_meta"]
        .is_null());
    assert!(reply::answer(&found[0], &json!({"kind":"always"}), &browser).is_err());
    browser["currentPermissions"]["approvalsReviewer"] = json!("auto");
    assert!(pending::parse(&browser, THREAD, "owner")
        .unwrap()
        .is_empty());
    browser["currentPermissions"] = json!({"approvalsReviewer":"user","approvalPolicy":"never","sandboxPolicy":{"type":"dangerFullAccess"}});
    assert!(pending::parse(&browser, THREAD, "owner")
        .unwrap()
        .is_empty());
}
#[test]
fn native_cards_say_what_allowing_keeps_as_open_words_it() {
    use agent_hud_core::l10n::tr;
    let conversation = tr!("Allow this conversation");
    let once = tr!("Allow once");
    let app = snapshot(vec![permission(
        "computer-use",
        json!(["session", "always"]),
    )]);
    let card = &pending::parse(&app, THREAD, "owner").unwrap()[0].view;
    assert!(
        card["alwaysAllowRule"].is_null(),
        "an app has no rule to show: {card}"
    );
    assert_eq!(card["allowLabel"], json!(conversation));
    assert_eq!(card["badge"], "APP");
    let site = snapshot(vec![permission(
        "browser-use",
        json!(["session", "always"]),
    )]);
    let card = &pending::parse(&site, THREAD, "owner").unwrap()[0].view;
    assert_eq!(
        card["alwaysAllowRule"], "https://fixture.invalid",
        "always allowing adds the site"
    );
    assert_eq!(card["allowLabel"], json!(conversation));
    assert_eq!(card["badge"], "WEB");
    let once_only = snapshot(vec![permission("browser-use", Value::Null)]);
    let card = &pending::parse(&once_only, THREAD, "owner").unwrap()[0].view;
    assert!(card["alwaysAllowRule"].is_null());
    assert_eq!(card["allowLabel"], json!(once));
    // A browser request whose turn carries no settings shows no card; only
    // one no turn is found for falls back on the current permissions.
    let mut unsettled = snapshot(vec![permission("browser-use", Value::Null)]);
    unsettled["turns"] = json!([{"turnId":"turn-fixture"}]);
    assert!(pending::parse(&unsettled, THREAD, "owner")
        .unwrap()
        .is_empty());
    unsettled["turns"] = json!([{"turnId":"turn-fixture","params":{"approvalsReviewer":"user"}}]);
    assert_eq!(
        pending::parse(&unsettled, THREAD, "owner").unwrap().len(),
        1,
        "the turn's own settings count"
    );
}
#[test]
fn canonical_async_question_answer_and_explicit_settlement() {
    let mut state = snapshot(vec![]);
    state["turnHistory"] = json!({"kind":"canonical","history":{"islands":[{"entries":[{"value":"key"}]}],"entitiesByKey":{"key":{"turnId":"turn-fixture","status":"inProgress","items":[{"type":"agentMessage","id":"message","questions":[{"title":"Pick one","options":["First","Second"]}]}]}}}});
    let found = pending::parse(&state, THREAD, "owner").unwrap();
    assert_eq!(found.len(), 1);
    let key = found[0].request_id.as_str().unwrap();
    let (method, response) = reply::answer(
        &found[0],
        &json!({"kind":"answer","answers":{key:{"selected":["First"],"custom":"extra"}}}),
        &state,
    )
    .unwrap();
    assert_eq!(method, "thread-follower-steer-turn");
    assert!(response.as_str().unwrap().contains("First, extra"));
    assert!(reply::answer(
        &found[0],
        &json!({"kind":"answer","answers":{key:{"selected":["Invalid"]}}}),
        &state
    )
    .is_err());
    state["turnHistory"]["history"]["entitiesByKey"]["key"]["items"].as_array_mut().unwrap().push(json!({"type":"steeringUserMessage","status":"accepted","input":[{"type":"text","text":response}]}));
    assert!(pending::parse(&state, THREAD, "owner").unwrap().is_empty());
    state["turns"] = json!([{"turnId":"turn-fixture","status":"completed"}]);
    assert!(pending::parse(&state, THREAD, "owner").is_err()); // overlap cannot prove withdrawal
}
#[test]
fn immer_patch_operations_follow_source() {
    let mut state = json!({"items":[1,2,3],"nested":{"x":1}});
    patches::apply_patches(&json!([{"op":"replace","path":["items","length"],"value":2},{"op":"add","path":["items","-"],"value":4},{"op":"replace","path":["items",3],"value":5},{"op":"remove","path":["items",0]},{"op":"add","path":["nested","y"],"value":2}]),&mut state).unwrap();
    assert_eq!(state, json!({"items":[2,4,5],"nested":{"x":1,"y":2}}));
    patches::apply_patches(&json!([{"op":"nonsense","path":["missing"]},{"op":"replace","path":[],"value":{"ok":true}}]),&mut state).unwrap();
    assert_eq!(state, json!({"ok":true}));
    assert!(patches::apply_patches(
        &json!([{"op":"replace","path":["missing","child"],"value":1}]),
        &mut state
    )
    .is_err());
}
fn read(socket: &mut UnixStream) -> Value {
    let mut size = [0; 4];
    socket.read_exact(&mut size).unwrap();
    let mut bytes = vec![0; u32::from_le_bytes(size) as usize];
    socket.read_exact(&mut bytes).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
fn send(socket: &mut UnixStream, value: Value) {
    let data = serde_json::to_vec(&value).unwrap();
    socket
        .write_all(&(data.len() as u32).to_le_bytes())
        .unwrap();
    socket.write_all(&data).unwrap();
}
fn response(socket: &mut UnixStream, request: &Value, result: Value) {
    send(
        socket,
        json!({"type":"response","requestId":request["requestId"],"resultType":"success","handledByClientId":"owner","result":result}),
    );
}
fn wait(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(4);
    while !condition() {
        assert!(Instant::now() < end, "fixture deadline");
        thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn observer_owned_reply_settle_and_disconnect_fixture() {
    for uncertain in [false, true] {
        // No prefix, one-letter names: the IPC socket path must stay short
        // under macOS's long temporary folder.
        let temporary = tempfile::Builder::new().prefix("").tempdir().unwrap();
        let directory = temporary.path().join(if uncertain { "u" } else { "c" });
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(directory.join("ipc")).unwrap();
        let listener = UnixListener::bind(directory.join("ipc/ipc.sock")).unwrap();
        let requests = Requests::default();
        let mut observer =
            CodexDesktopObserver::start(directory.clone(), requests.clone(), Arc::new(|| {}))
                .unwrap();
        let (go, ready) = std::sync::mpsc::channel();
        let (done, ended) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(4)))
                .unwrap();
            let init = read(&mut stream);
            response(&mut stream, &init, json!({"clientId":"stream-client"}));
            let discovery = read(&mut stream);
            assert_eq!(discovery["method"], "thread-owner-discovery");
            response(&mut stream, &discovery, json!({}));
            assert_eq!(read(&mut stream)["params"]["following"], true);
            let mut state = snapshot(vec![permission(
                "computer-use",
                json!(["session", "always"]),
            )]);
            state["largeToolResult"] = json!("x".repeat(2 * 1024 * 1024));
            send(
                &mut stream,
                json!({"type":"broadcast","method":"thread-stream-state-changed","version":11,"sourceClientId":"owner","params":{"hostId":"local","conversationId":THREAD,"change":{"type":"snapshot","revision":1,"conversationState":state}}}),
            );
            let (mut answer, _) = listener.accept().unwrap();
            let init = read(&mut answer);
            response(&mut answer, &init, json!({"clientId":"answer-client"}));
            let discovery = read(&mut answer);
            response(&mut answer, &discovery, json!({}));
            let mutation = read(&mut answer);
            assert_eq!(mutation["method"], pending::ELICITATION_METHOD);
            assert_eq!(mutation["targetClientId"], "owner");
            assert_eq!(mutation["params"]["requestId"], 7);
            assert_eq!(mutation["params"]["response"]["_meta"]["persist"], "always");
            if uncertain {
                drop(answer);
            } else {
                response(
                    &mut answer,
                    &mutation,
                    json!({"method":pending::ELICITATION_METHOD,"result":{"ok":true}}),
                );
            }
            ready.recv_timeout(Duration::from_secs(4)).unwrap();
            send(
                &mut stream,
                json!({"type":"broadcast","method":"thread-stream-state-changed","version":11,"sourceClientId":"owner","params":{"hostId":"local","conversationId":THREAD,"change":{"type":"patches","baseRevision":1,"revision":2,"patches":[{"op":"replace","path":["requests"],"value":[]}]}}}),
            );
            done.send(()).unwrap();
        });
        // A CLI thread in flight is not followed; a Desktop thread is.
        observer.update_sessions(&[json!({"id":THREAD,"source":AGENT.id,"state":"running"})]);
        assert!(observer.shared.wanted().is_empty());
        // Another app-server client's thread opens in Codex, but Desktop does
        // not hold it.
        observer.update_sessions(&[json!({"id":THREAD,"source":AGENT.id,"state":"running","client":"codex_ask","navigationTarget":{"kind":"codex-thread","id":THREAD}})]);
        assert!(observer.shared.wanted().is_empty());
        observer.update_sessions(&[json!({"id":THREAD,"source":AGENT.id,"state":"running","navigationTarget":{"kind":"codex-thread","id":THREAD}})]);
        wait(|| requests.views(crate::hooks::now_ms()).len() == 1);
        assert!(
            observer.shared.state.lock().unwrap().known[THREAD].state["largeToolResult"].is_null()
        );
        let id = requests.views(crate::hooks::now_ms())[0]["id"]
            .as_str()
            .unwrap()
            .to_string();
        requests
            .answer(&id, &json!({"kind":"always"}), crate::hooks::now_ms())
            .unwrap();
        wait(|| {
            requests
                .deliveries(crate::hooks::now_ms())
                .iter()
                .any(|row| {
                    row["id"] == id
                        && row["state"] == if uncertain { "uncertain" } else { "confirmed" }
                })
        });
        go.send(()).unwrap();
        ended.recv_timeout(Duration::from_secs(4)).unwrap();
        worker.join().unwrap();
        wait(|| requests.views(crate::hooks::now_ms()).is_empty());
        observer.stop();
        assert!(requests
            .answer(&id, &json!({"kind":"allow"}), crate::hooks::now_ms())
            .is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn native_waiting_is_phase_only_and_clears_on_disconnect_and_stop() {
    let temporary = tempfile::Builder::new().prefix("").tempdir().unwrap();
    let directory = temporary.path().join("w");
    let requests = Requests::default();
    let mut observer =
        CodexDesktopObserver::start(directory, requests.clone(), Arc::new(|| {})).unwrap();
    let mut state = snapshot(vec![]);
    state["threadRuntimeStatus"] = json!({"type":"active","activeFlags":["waitingOnApproval"]});
    observer.shared.reconcile(THREAD, "owner", state.clone());
    assert!(requests
        .native_waiting_sessions()
        .get(AGENT.id)
        .is_some_and(|sessions| sessions.contains(THREAD)));
    assert!(requests.views(crate::hooks::now_ms()).is_empty());
    let revision = requests.revision();
    observer.shared.reconcile(THREAD, "owner", state.clone());
    assert_eq!(requests.revision(), revision);
    state["threadRuntimeStatus"]["activeFlags"] = json!(["waitingOnUserInput"]);
    state.as_object_mut().unwrap().remove("requests");
    observer.shared.reconcile(THREAD, "owner", state.clone()); // waiting is known even if no supported cards can be decoded
    assert!(requests
        .native_waiting_sessions()
        .get(AGENT.id)
        .is_some_and(|sessions| sessions.contains(THREAD)));
    assert!(requests.views(crate::hooks::now_ms()).is_empty());
    observer.shared.forget_connection();
    assert!(requests
        .native_waiting_sessions()
        .get(AGENT.id)
        .is_none_or(|sessions| sessions.is_empty()));
    observer.shared.reconcile(THREAD, "owner", state);
    observer.stop();
    assert!(requests
        .native_waiting_sessions()
        .get(AGENT.id)
        .is_none_or(|sessions| sessions.is_empty()));
    assert!(requests.views(crate::hooks::now_ms()).is_empty());
}
