//! DeepSeekQuestionObserver.swift & DeepSeekQuestions.swift:
//! Observes questions DeepSeek Harness's web hosts ask over `ws://127.0.0.1:<port>/api/events.mux`,
//! confirms hosts via `POST /api/host.describe`, and delivers answers via `POST /api/respond`.
use crate::hooks::Wake;
use agent_hud_agents::deepseek::AGENT;
use agent_hud_core::l10n;
use agent_hud_core::requests::{Requests, Ticket};
use chrono::Utc;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    net::{SocketAddr, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::RecvTimeoutError,
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tungstenite::Message;

pub const DEFAULT_PORT: u16 = 3080;
const ENDPOINT_LIFETIME: Duration = Duration::from_secs(60);
const RESOLUTION_MEMORY: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestionItem {
    pub id: String,
    pub question: String,
    pub header: Option<String>,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub rpc_id: String,
    pub session_id: String,
    pub questions: Vec<QuestionItem>,
}

#[derive(Clone, Debug)]
pub struct Pending {
    pub id: String,
    pub rpc_id: String,
    pub port: u16,
    pub raw_session_id: String,
    pub session_id: String,
    pub questions: Vec<QuestionItem>,
    pub view: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Receipt {
    Accepted,
    NotPending,
}

pub fn request_id(port: u16, rpc_id: &str) -> String {
    format!("deepseek:{port}:{rpc_id}")
}

pub fn describe_envelope() -> Value {
    json!({
        "type": "client-request",
        "rpcId": agent_hud_core::grok_cache::hash(&["host.describe"]),
        "method": "host.describe",
        "payload": {},
    })
}

fn unwrap_server_response(reply: &Value) -> Result<&Value, String> {
    if reply["type"].as_str() != Some("server-response") || !reply["result"].is_object() {
        return Err("DeepSeek response has an unsupported format.".into());
    }
    if reply["result"]["ok"].as_bool() != Some(true) {
        return Err(l10n::text(
            "DeepSeek Harness 的提问服务不在这个端口",
            "DeepSeek Harness's question service is not on that port.",
        ));
    }
    Ok(&reply["result"]["value"])
}

pub fn is_valid_describe(reply: &Value) -> bool {
    let Ok(value) = unwrap_server_response(reply) else {
        return false;
    };
    value["version"].is_string() && (value["provider"].is_string() || value["model"].is_string())
}

pub fn parse_frame(json: &Value) -> Option<Frame> {
    if json["type"].as_str() != Some("server-request")
        || json["method"].as_str() != Some("question/requested")
    {
        return None;
    }
    let rpc_id = json["rpcId"].as_str()?.trim();
    if rpc_id.is_empty() {
        return None;
    }
    let payload = &json["payload"];
    let session_id = payload["sessionId"].as_str()?.trim();
    if session_id.is_empty() {
        return None;
    }
    let questions = parse_questions(&payload["questions"])?;
    Some(Frame {
        rpc_id: rpc_id.to_owned(),
        session_id: session_id.to_owned(),
        questions,
    })
}

/// The questions of one request, as both channels carry them; None when
/// one of them is not a choice the HUD can offer (a plan to review, a
/// detail to read, no options).
pub fn parse_questions(items: &Value) -> Option<Vec<QuestionItem>> {
    let items = items.as_array()?;
    if items.is_empty() {
        return None;
    }
    let mut ids = HashSet::new();
    let mut questions = Vec::with_capacity(items.len());
    for item in items {
        let id = item["id"].as_str()?.trim();
        if id.is_empty() || !ids.insert(id.to_owned()) {
            return None;
        }
        if item["intent"]["kind"].as_str() == Some("plan-review") {
            return None;
        }
        if item["detail"]
            .as_str()
            .is_some_and(|d| !d.trim().is_empty())
        {
            return None;
        }
        let question = item["question"].as_str()?.trim();
        if question.is_empty() {
            return None;
        }
        let raw_options = item["options"].as_array()?;
        if raw_options.is_empty() {
            return None;
        }
        let mut options = Vec::with_capacity(raw_options.len());
        for opt in raw_options {
            let label = opt["label"].as_str()?;
            if label.is_empty() {
                return None;
            }
            let description = opt["description"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            options.push(QuestionOption {
                label: label.to_owned(),
                description,
            });
        }
        if options.len() != raw_options.len() {
            return None;
        }
        let header = item["header"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let multi_select = item["multiSelect"].as_bool().unwrap_or(false);
        questions.push(QuestionItem {
            id: id.to_owned(),
            question: item["question"].as_str()?.to_owned(),
            header,
            options,
            multi_select,
        });
    }
    Some(questions)
}

pub fn build_pending(frame: Frame, port: u16) -> Pending {
    let id = request_id(port, &frame.rpc_id);
    let session_id = format!("{}:{}", AGENT.id, frame.session_id);
    let view = question_view(&id, &session_id, &frame.questions);
    Pending {
        id,
        rpc_id: frame.rpc_id,
        port,
        raw_session_id: frame.session_id,
        session_id,
        questions: frame.questions,
        view,
    }
}

/// The HUD's card for a DeepSeek Harness question, whichever channel asked.
pub fn question_view(id: &str, session_id: &str, questions: &[QuestionItem]) -> Value {
    let questions_json: Vec<Value> = questions
        .iter()
        .map(|q| {
            json!({
                "id": q.id,
                "answerKey": q.id,
                "question": q.question,
                "header": q.header,
                "options": q.options.iter().map(|o| json!({
                    "label": o.label,
                    "description": o.description,
                })).collect::<Vec<_>>(),
                "multiSelect": q.multi_select,
            })
        })
        .collect();
    let summary = questions
        .first()
        .map(|q| q.question.clone())
        .unwrap_or_default();
    let input_obj = json!({ "questions": questions_json });
    json!({
        "id": id,
        "source": AGENT.id,
        "vendor": AGENT.name,
        "sessionId": session_id,
        "title": summary,
        "summary": summary,
        "toolName": "AskUserQuestion",
        "kind": "question",
        "input": serde_json::to_string(&input_obj).unwrap_or_default(),
        "detail": null,
        "cwd": null,
        "createdAt": Utc::now().to_rfc3339(),
        "allowsAlways": false,
        "questions": questions_json,
        "answersQuestion": true,
        "answersPlan": false,
        "plan": null,
        "suggestions": [],
    })
}

pub fn build_answer(decision: &Value, pending: &Pending) -> Result<Option<Value>, String> {
    let Some(entries) = answer_entries(decision, &pending.questions)? else {
        return Ok(None);
    };
    Ok(Some(json!({
        "type": "client-response",
        "rpcId": pending.rpc_id,
        "result": {
            "ok": true,
            "value": {
                "sessionId": pending.raw_session_id,
                "answer": {
                    "answers": entries,
                },
            },
        },
    })))
}

/// Harness's answer items for the HUD's decision, one per question; None
/// when the user left the question to be answered in Harness.
pub fn answer_entries(
    decision: &Value,
    questions: &[QuestionItem],
) -> Result<Option<Vec<Value>>, String> {
    let kind = decision["kind"].as_str().unwrap_or("");
    if kind == "leave" {
        return Ok(None);
    }
    if kind != "answer" {
        return Err(l10n::text(
            "DeepSeek Harness 的提问只能回答，不能批准",
            "DeepSeek Harness's questions can only be answered, not approved.",
        ));
    }
    let answers = decision["answers"].as_object();
    let mut entries = Vec::with_capacity(questions.len());
    for question in questions {
        let given = answers.and_then(|map| map.get(&question.id));
        let selected: Vec<Value> = given
            .and_then(|g| g["selected"].as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| json!(s)))
                    .collect()
            })
            .unwrap_or_default();
        let mut entry = json!({
            "id": question.id,
            "selected": selected,
        });
        if let Some(custom) = given
            .and_then(|g| g["custom"].as_str())
            .filter(|s| !s.is_empty())
        {
            entry["custom"] = json!(custom);
        }
        entries.push(entry);
    }
    Ok(Some(entries))
}

pub fn parse_receipt(reply: &Value) -> Result<Receipt, String> {
    let value = unwrap_server_response(reply)?;
    if value["accepted"].as_bool() == Some(true) {
        return Ok(Receipt::Accepted);
    }
    if value["accepted"].as_bool() == Some(false) && value["reason"].as_str() == Some("not-pending")
    {
        return Ok(Receipt::NotPending);
    }
    Err("DeepSeek response receipt has an unsupported format.".into())
}

pub fn is_harness(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    if !lower.contains("--profile web") && !lower.contains("--profile=web") {
        return false;
    }
    lower.split_whitespace().any(|token| {
        let trimmed = token.trim_matches(|c| c == '"' || c == '\'');
        trimmed == "dsh"
            || trimmed.ends_with("/dsh")
            || trimmed == "bin.js"
            || trimmed.ends_with("/bin.js")
    })
}

pub fn port_flag(command: &str) -> Option<u16> {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    for (idx, token) in tokens.iter().enumerate() {
        let raw = if let Some(rest) = token.strip_prefix("--port=") {
            Some(rest)
        } else if *token == "--port" {
            tokens.get(idx + 1).copied()
        } else {
            None
        };
        if let Some(raw) = raw {
            let clean = raw.trim_matches(|c| c == '"' || c == '\'');
            if let Ok(port) = clean.parse::<u16>() {
                if port > 0 {
                    return Some(port);
                }
            }
        }
    }
    None
}

pub fn discover_ports() -> Vec<u16> {
    let mut ports = HashSet::new();
    for proc in crate::os::processes().into_iter().flatten() {
        if !is_harness(&proc.command) {
            continue;
        }
        ports.insert(port_flag(&proc.command).unwrap_or(DEFAULT_PORT));
    }
    let mut sorted: Vec<u16> = ports.into_iter().collect();
    sorted.sort_unstable();
    sorted
}

struct State {
    endpoints: HashMap<u16, Instant>,
    streams: HashSet<u16>,
    pending: HashMap<String, Pending>,
    resolved: HashMap<String, Instant>,
}

struct Shared {
    active: AtomicBool,
    state: Mutex<State>,
    changed: Condvar,
    requests: Requests,
    wake: Wake,
    http: Client,
}

impl Shared {
    fn publish_waiting(&self) {
        let sessions: Vec<String> = {
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            let mut set: Vec<String> = state
                .pending
                .values()
                .map(|p| p.session_id.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            set.sort();
            set
        };
        self.requests
            .set_native_waiting(AGENT.id, &sessions, crate::hooks::now_ms());
        (self.wake)();
    }

    fn withdraw(&self, id: &str) {
        {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state.resolved.insert(id.to_owned(), Instant::now());
            state.pending.remove(id);
        }
        self.requests.settle_native(id);
        self.publish_waiting();
    }
}

pub struct Observer {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl Observer {
    pub fn start(requests: Requests, wake: Wake) -> Result<Self, String> {
        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(4))
            .build()
            .map_err(|_| "DeepSeek HTTP client could not start")?;
        let shared = Arc::new(Shared {
            active: AtomicBool::new(true),
            state: Mutex::new(State {
                endpoints: HashMap::new(),
                streams: HashSet::new(),
                pending: HashMap::new(),
                resolved: HashMap::new(),
            }),
            changed: Condvar::new(),
            requests,
            wake,
            http,
        });
        let owned = shared.clone();
        let worker = thread::Builder::new()
            .name("DeepSeek questions".into())
            .spawn(move || work(owned))
            .map_err(|_| "DeepSeek question observer could not start")?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        self.shared.active.store(false, Ordering::Release);
        self.shared.changed.notify_all();
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
            for id in state.pending.keys() {
                self.shared.requests.settle_native(id);
            }
            state.pending.clear();
            state.streams.clear();
            state.endpoints.clear();
            state.resolved.clear();
        }
        self.shared
            .requests
            .set_native_waiting(AGENT.id, &[], crate::hooks::now_ms());
        if let Some(worker) = self.worker.take() {
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}

fn describe_port(http: &Client, port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/api/host.describe");
    let Ok(resp) = http
        .post(&url)
        .header("Host", "127.0.0.1")
        .json(&describe_envelope())
        .timeout(Duration::from_secs(2))
        .send()
    else {
        return false;
    };
    if !resp.status().is_success() {
        return false;
    }
    let Ok(reply) = resp.json::<Value>() else {
        return false;
    };
    is_valid_describe(&reply)
}

fn post_answer(http: &Client, port: u16, body: &Value) -> Result<Receipt, String> {
    let url = format!("http://127.0.0.1:{port}/api/respond");
    let resp = http
        .post(&url)
        .header("Host", "127.0.0.1")
        .json(body)
        .timeout(Duration::from_secs(4))
        .send()
        .map_err(|_| {
            l10n::text(
                "DeepSeek Harness 没有确认收到回答",
                "DeepSeek Harness did not confirm receipt of the answer.",
            )
        })?;
    if !resp.status().is_success() {
        return Err(l10n::text(
            "DeepSeek Harness 拒绝了这个回答",
            "DeepSeek Harness rejected the answer.",
        ));
    }
    let reply = resp.json::<Value>().map_err(|_| {
        l10n::text(
            "DeepSeek Harness 返回的回执无效",
            "DeepSeek Harness returned an invalid receipt.",
        )
    })?;
    parse_receipt(&reply)
}

fn handle_frame(shared: &Arc<Shared>, port: u16, frame_json: &Value) {
    let now = Instant::now();
    {
        let mut state = prune_resolved(shared, now);
        let _ = &mut state;
    }
    if frame_json["method"].as_str() == Some("question/resolved") {
        if let Some(rpc_id) = frame_json["payload"]["questionRpcId"]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            shared.withdraw(&request_id(port, rpc_id));
        }
        return;
    }
    let Some(frame) = parse_frame(frame_json) else {
        return;
    };
    let id = request_id(port, &frame.rpc_id);
    let pending = build_pending(frame, port);
    let ticket = {
        let mut state = shared.state.lock().unwrap_or_else(|p| p.into_inner());
        state
            .resolved
            .retain(|_, seen| now.duration_since(*seen) < RESOLUTION_MEMORY);
        if state.resolved.contains_key(&id) {
            return;
        }
        let ticket = shared.requests.register_native(
            id.clone(),
            pending.view.clone(),
            crate::hooks::now_ms(),
        );
        state.pending.insert(id, pending.clone());
        ticket
    };
    shared.publish_waiting();
    if let Some(ticket) = ticket {
        let owned = shared.clone();
        let _ = thread::Builder::new()
            .name("DeepSeek question answer".into())
            .spawn(move || answer_loop(owned, pending, ticket));
    }
}

fn prune_resolved<'a>(shared: &'a Arc<Shared>, now: Instant) -> std::sync::MutexGuard<'a, State> {
    let mut state = shared.state.lock().unwrap_or_else(|p| p.into_inner());
    state
        .resolved
        .retain(|_, seen| now.duration_since(*seen) < RESOLUTION_MEMORY);
    state
}

fn answer_loop(shared: Arc<Shared>, pending: Pending, ticket: Ticket) {
    while shared.active.load(Ordering::Acquire) {
        let decision = match ticket.response.recv_timeout(Duration::from_millis(100)) {
            Ok(Some(decision)) => decision,
            Ok(None) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => continue,
        };
        if !shared.active.load(Ordering::Acquire) {
            return;
        }
        match build_answer(&decision, &pending) {
            Ok(None) => return,
            Ok(Some(body)) => match post_answer(&shared.http, pending.port, &body) {
                Ok(Receipt::Accepted | Receipt::NotPending) => {
                    shared.requests.note_native_delivery(
                        &pending.id,
                        AGENT.id,
                        &pending.session_id,
                        "confirmed",
                        None,
                        crate::hooks::now_ms(),
                    );
                    shared.withdraw(&pending.id);
                }
                Err(reason) => {
                    shared.requests.note_native_delivery(
                        &pending.id,
                        AGENT.id,
                        &pending.session_id,
                        "failed",
                        Some(reason),
                        crate::hooks::now_ms(),
                    );
                    shared.requests.settle_native(&pending.id);
                }
            },
            Err(reason) => {
                shared.requests.note_native_delivery(
                    &pending.id,
                    AGENT.id,
                    &pending.session_id,
                    "failed",
                    Some(reason),
                    crate::hooks::now_ms(),
                );
                shared.requests.settle_native(&pending.id);
            }
        }
        return;
    }
}

fn stream_port(shared: Arc<Shared>, port: u16) {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    if let Ok(tcp) = TcpStream::connect_timeout(&addr, Duration::from_secs(3)) {
        let _ = tcp.set_read_timeout(Some(Duration::from_millis(500)));
        let _ = tcp.set_write_timeout(Some(Duration::from_secs(2)));
        let url = format!("ws://127.0.0.1:{port}/api/events.mux");
        if let Ok((mut ws, _)) = tungstenite::client(url.as_str(), tcp) {
            while shared.active.load(Ordering::Acquire) {
                let still_tracked = shared
                    .state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .streams
                    .contains(&port);
                if !still_tracked {
                    break;
                }
                match ws.read() {
                    Ok(Message::Text(text)) => {
                        if let Ok(json) = serde_json::from_str::<Value>(text.as_ref()) {
                            handle_frame(&shared, port, &json);
                        }
                    }
                    Ok(Message::Binary(bytes)) => {
                        if let Ok(json) = serde_json::from_slice::<Value>(&bytes) {
                            handle_frame(&shared, port, &json);
                        }
                    }
                    Ok(Message::Ping(payload)) => {
                        let _ = ws.send(Message::Pong(payload));
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(ref err))
                        if matches!(
                            err.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        continue;
                    }
                    Err(_) => break,
                }
            }
        }
    }
    let settled: Vec<String> = {
        let mut state = shared.state.lock().unwrap_or_else(|p| p.into_inner());
        state.streams.remove(&port);
        state.endpoints.remove(&port);
        let ids: Vec<String> = state
            .pending
            .values()
            .filter(|p| p.port == port)
            .map(|p| p.id.clone())
            .collect();
        for id in &ids {
            state.pending.remove(id);
        }
        ids
    };
    for id in settled {
        shared.requests.settle_native(&id);
    }
    shared.publish_waiting();
}

fn work(shared: Arc<Shared>) {
    while shared.active.load(Ordering::Acquire) {
        let found = discover_ports();
        if !shared.active.load(Ordering::Acquire) {
            break;
        }
        let now = Instant::now();
        let mut to_describe = Vec::new();
        {
            let state = shared.state.lock().unwrap_or_else(|p| p.into_inner());
            for &port in &found {
                if state
                    .endpoints
                    .get(&port)
                    .is_none_or(|last| now.duration_since(*last) > ENDPOINT_LIFETIME)
                {
                    to_describe.push(port);
                }
            }
        }
        let mut newly_described = HashMap::new();
        for port in to_describe {
            if !shared.active.load(Ordering::Acquire) {
                return;
            }
            newly_described.insert(port, describe_port(&shared.http, port));
        }
        let mut spawn_ports = Vec::new();
        let mut removed_ids = Vec::new();
        {
            let mut state = shared.state.lock().unwrap_or_else(|p| p.into_inner());
            for (port, ok) in newly_described {
                if ok {
                    state.endpoints.insert(port, now);
                } else {
                    state.endpoints.remove(&port);
                }
            }
            let confirmed: HashSet<u16> = found
                .into_iter()
                .filter(|port| state.endpoints.contains_key(port))
                .collect();
            for &port in &confirmed {
                if state.streams.insert(port) {
                    spawn_ports.push(port);
                }
            }
            let stale_ports: Vec<u16> = state
                .streams
                .iter()
                .copied()
                .filter(|p| !confirmed.contains(p))
                .collect();
            for port in stale_ports {
                state.streams.remove(&port);
                state.endpoints.remove(&port);
                let ids: Vec<String> = state
                    .pending
                    .values()
                    .filter(|p| p.port == port)
                    .map(|p| p.id.clone())
                    .collect();
                for id in ids {
                    state.pending.remove(&id);
                    removed_ids.push(id);
                }
            }
        }
        for id in removed_ids {
            shared.requests.settle_native(&id);
        }
        for port in spawn_ports {
            let owned = shared.clone();
            let _ = thread::Builder::new()
                .name(format!("DeepSeek mux {port}"))
                .spawn(move || stream_port(owned, port));
        }
        shared.publish_waiting();
        let guard = shared.state.lock().unwrap_or_else(|p| p.into_inner());
        if shared.active.load(Ordering::Acquire) {
            let _ = shared.changed.wait_timeout(guard, Duration::from_secs(2));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_harness_discovery_frame_parsing_and_answer_receipt_match_source() {
        assert!(is_harness(
            "/opt/homebrew/bin/node /usr/local/lib/node_modules/deepseek-harness/bin.js --profile web --port 3199"
        ));
        assert_eq!(
            port_flag(
                "/opt/homebrew/bin/node /usr/local/lib/node_modules/deepseek-harness/bin.js --profile web --port 3199"
            ),
            Some(3199)
        );
        assert!(is_harness("dsh --profile=web"));
        assert_eq!(port_flag("dsh --profile=web"), None);
        assert!(!is_harness("dsh --profile cli"));

        let describe_reply = json!({
            "type": "server-response",
            "result": {
                "ok": true,
                "value": {
                    "version": "0.4.2",
                    "provider": "deepseek",
                    "model": "deepseek-chat"
                }
            }
        });
        assert!(is_valid_describe(&describe_reply));

        let raw_frame = json!({
            "type": "server-request",
            "rpcId": "rpc_77",
            "method": "question/requested",
            "payload": {
                "sessionId": "sess_abc",
                "questions": [
                    {
                        "id": "q_target",
                        "question": "Which target should we build?",
                        "header": "Target",
                        "multiSelect": false,
                        "options": [
                            {"label": "Debug", "description": "Fast compile"},
                            {"label": "Release", "description": "Optimized"}
                        ]
                    }
                ]
            }
        });
        let frame = parse_frame(&raw_frame).expect("valid question frame");
        let pending = build_pending(frame, 3080);
        assert_eq!(pending.id, "deepseek:3080:rpc_77");
        assert_eq!(pending.session_id, "deepseek:sess_abc");
        assert_eq!(pending.view["answersQuestion"], true);

        let decision = json!({
            "kind": "answer",
            "answers": {
                "q_target": {
                    "selected": ["Release"],
                    "custom": "With LTO"
                }
            }
        });
        let answer_body = build_answer(&decision, &pending)
            .unwrap()
            .expect("answer payload");
        assert_eq!(answer_body["type"], "client-response");
        assert_eq!(answer_body["rpcId"], "rpc_77");
        assert_eq!(
            answer_body["result"]["value"]["sessionId"],
            json!("sess_abc")
        );
        assert_eq!(
            answer_body["result"]["value"]["answer"]["answers"],
            json!([{
                "id": "q_target",
                "selected": ["Release"],
                "custom": "With LTO"
            }])
        );

        let receipt_ok = json!({
            "type": "server-response",
            "result": {
                "ok": true,
                "value": { "accepted": true }
            }
        });
        assert_eq!(parse_receipt(&receipt_ok).unwrap(), Receipt::Accepted);

        let receipt_not_pending = json!({
            "type": "server-response",
            "result": {
                "ok": true,
                "value": { "accepted": false, "reason": "not-pending" }
            }
        });
        assert_eq!(
            parse_receipt(&receipt_not_pending).unwrap(),
            Receipt::NotPending
        );
    }
}
