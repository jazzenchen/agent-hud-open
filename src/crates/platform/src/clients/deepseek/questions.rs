//! DeepSeekQuestionObserver.swift & DeepSeekQuestions.swift:
//! Observes questions DeepSeek Harness's web hosts ask over `ws://127.0.0.1:<port>/api/events.mux`,
//! confirms hosts via `POST /api/host.describe`, and delivers answers via `POST /api/respond`.
use crate::hooks::Wake;
use agent_hud_agents::deepseek::AGENT;
use agent_hud_core::l10n;
use agent_hud_core::requests::Requests;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::{atomic::AtomicBool, Condvar, Mutex, PoisonError},
    time::Instant,
};

mod card;
mod observer;
mod stream;

pub use card::{answer_entries, question_view};
pub use observer::Observer;

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
            let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
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
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            state.resolved.insert(id.to_owned(), Instant::now());
            state.pending.remove(id);
        }
        self.requests.settle_native(id);
        self.publish_waiting();
    }
}

#[cfg(test)]
mod tests;
