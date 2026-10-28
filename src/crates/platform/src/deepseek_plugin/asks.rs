//! The questions and approvals DeepSeek Harness sessions wait on, which
//! Agent HUD's plugin writes into the channel folder (`asks/<id>.json`),
//! shown on the HUD next to Harness's own cards. The HUD's answer goes back
//! as `asks/<id>.answer`; the plugin hands it to Harness and removes both
//! files. An ask whose file goes is settled: answered in Harness, cancelled,
//! or answered here. While this runs, `app.json` tells the plugin the app is
//! there, so a host with no window of its own can still be answered here.
use super::DeepSeekChannel;
use crate::claude_plugin::{valid_name, write_private};
use crate::clients::deepseek::questions::{self, QuestionItem};
use crate::hooks::Wake;
use agent_hud_agents::deepseek::AGENT;
use agent_hud_core::l10n;
use agent_hud_core::requests::{Requests, Ticket};
use chrono::Utc;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::RecvTimeoutError,
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const POLL: Duration = Duration::from_millis(300);
const HEARTBEAT: Duration = Duration::from_secs(2);
const SWEEP: Duration = Duration::from_secs(60);
/// An ask from a session that has not checked in for this long belongs to a
/// host that stopped: its file goes.
const ORPHAN_GRACE: Duration = Duration::from_secs(5);
/// How much of a call's arguments a card shows.
const DETAIL_LIMIT: usize = 2_000;

/// What a session waits on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Question(Vec<QuestionItem>),
    Approval,
}

/// One ask the HUD can show.
#[derive(Clone, Debug)]
pub struct Ask {
    /// The plugin's id for it, the file's name.
    pub id: String,
    /// Harness's session id.
    pub session: String,
    pub kind: Kind,
    /// The HUD's card.
    pub view: Value,
}

/// The id the HUD's requests know an ask by.
pub fn request_id(ask: &str) -> String {
    format!("{}-plugin:{ask}", AGENT.id)
}

fn session_id(session: &str) -> String {
    format!("{}:{session}", AGENT.id)
}

/// Reads an ask as the plugin wrote it; None when it is not one the HUD can
/// show (a plan to review, a question without choices), which Harness's own
/// card still answers.
pub fn parse(id: &str, value: &Value) -> Option<Ask> {
    if !valid_name(id) || value["id"] != id {
        return None;
    }
    let session = value["sessionId"].as_str().filter(|s| valid_name(s))?;
    let request = request_id(id);
    let (kind, view) = match value["kind"].as_str()? {
        "question" => {
            let items = questions::parse_questions(&value["questions"])?;
            let view = questions::question_view(&request, &session_id(session), &items);
            (Kind::Question(items), view)
        }
        "approval" => (Kind::Approval, approval_view(&request, session, value)?),
        _ => return None,
    };
    Some(Ask {
        id: id.to_owned(),
        session: session.to_owned(),
        kind,
        view,
    })
}

/// The card for an approval: why Harness asks, in the app's language, and
/// the command or arguments of the call it asks about.
fn approval_view(id: &str, session: &str, value: &Value) -> Option<Value> {
    let tool = value["toolName"].as_str().filter(|s| !s.is_empty())?;
    let shown = |s: &&str| !s.trim().is_empty();
    let reason = &value["displayReason"];
    let localized = match (
        reason["zh"].as_str().filter(shown),
        reason["en"].as_str().filter(shown),
    ) {
        (Some(zh), Some(en)) => Some(l10n::text(zh, en)),
        (zh, en) => zh.or(en).map(str::to_owned),
    };
    let summary = localized
        .or_else(|| value["reason"].as_str().filter(shown).map(str::to_owned))
        .unwrap_or_else(|| tool.to_owned());
    let detail = value["arguments"].as_str().map(|text| {
        let arguments = serde_json::from_str::<Value>(text).unwrap_or(Value::Null);
        let subject = ["command", "file_path", "path", "url"]
            .into_iter()
            .find_map(|key| arguments[key].as_str().filter(shown));
        subject
            .unwrap_or(text)
            .chars()
            .take(DETAIL_LIMIT)
            .collect::<String>()
    });
    Some(json!({
        "id": id,
        "source": AGENT.id,
        "vendor": AGENT.name,
        "sessionId": session_id(session),
        "title": summary,
        "summary": summary,
        "toolName": tool,
        "kind": "permission",
        "input": detail.clone().unwrap_or_default(),
        "detail": detail,
        "cwd": null,
        "createdAt": Utc::now().to_rfc3339(),
        "allowsAlways": false,
        "questions": [],
        "answersQuestion": false,
        "answersPlan": false,
        "plan": null,
        "suggestions": [],
    }))
}

/// What the plugin hands Harness for the HUD's decision; None when the user
/// left it to Harness's own card.
pub fn answer(kind: &Kind, decision: &Value) -> Result<Option<Value>, String> {
    match kind {
        Kind::Question(items) => Ok(questions::answer_entries(decision, items)?
            .map(|entries| json!({ "answers": entries }))),
        Kind::Approval => match decision["kind"].as_str() {
            Some("leave") => Ok(None),
            Some("allow") => Ok(Some(json!("allowed-once"))),
            Some("deny") => Ok(Some(json!("rejected"))),
            _ => Err(l10n::text(
                "DeepSeek Harness 的批准只能允许一次或拒绝",
                "DeepSeek Harness's approvals can only be allowed once or rejected.",
            )),
        },
    }
}

struct Known {
    request: String,
    session: String,
    /// Registered with the HUD; an ask it cannot show is only remembered.
    shown: bool,
}

struct Shared {
    active: AtomicBool,
    channel: DeepSeekChannel,
    requests: Requests,
    wake: Wake,
    known: Mutex<HashMap<String, Known>>,
    sleep: Mutex<()>,
    changed: Condvar,
}

/// Watches the channel folder's asks while the HUD answers requests.
pub struct Observer {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl Observer {
    pub fn start(channel: DeepSeekChannel, requests: Requests, wake: Wake) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            active: AtomicBool::new(true),
            channel,
            requests,
            wake,
            known: Mutex::new(HashMap::new()),
            sleep: Mutex::new(()),
            changed: Condvar::new(),
        });
        let owned = shared.clone();
        let worker = thread::Builder::new()
            .name("DeepSeek plugin asks".into())
            .spawn(move || work(owned))
            .map_err(|_| "DeepSeek Harness plugin observer could not start")?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    /// One pass now, for tests.
    #[cfg(test)]
    pub(crate) fn pass(&self) {
        pass(&self.shared, SystemTime::now());
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        self.shared.active.store(false, Ordering::Release);
        self.shared.changed.notify_all();
        let known =
            std::mem::take(&mut *self.shared.known.lock().unwrap_or_else(|p| p.into_inner()));
        for entry in known.values().filter(|entry| entry.shown) {
            self.shared.requests.settle_native(&entry.request);
        }
        self.shared
            .requests
            .set_native_waiting(AGENT.id, &[], crate::hooks::now_ms());
        let _ = fs::remove_file(self.shared.channel.directory().join("app.json"));
        if let Some(worker) = self.worker.take() {
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}

fn millis(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn work(shared: Arc<Shared>) {
    let mut heartbeat: Option<SystemTime> = None;
    let mut swept: Option<SystemTime> = None;
    while shared.active.load(Ordering::Acquire) {
        let now = SystemTime::now();
        let due = |last: Option<SystemTime>, every: Duration| {
            last.is_none_or(|last| now.duration_since(last).unwrap_or_default() >= every)
        };
        if due(heartbeat, HEARTBEAT) {
            let file = shared.channel.directory().join("app.json");
            if crate::os::private_directory(shared.channel.directory()).is_ok()
                && write_private(&file, json!({ "at": millis(now) }).to_string().as_bytes()).is_ok()
            {
                heartbeat = Some(now);
            }
        }
        if due(swept, SWEEP) {
            shared.channel.sweep(now);
            swept = Some(now);
        }
        pass(&shared, now);
        let guard = shared.sleep.lock().unwrap_or_else(|p| p.into_inner());
        if shared.active.load(Ordering::Acquire) {
            let _ = shared.changed.wait_timeout(guard, POLL);
        }
    }
}

/// Shows new asks, settles the ones whose files went, and says which
/// sessions wait.
fn pass(shared: &Arc<Shared>, now: SystemTime) {
    let folder = shared.channel.directory().join("asks");
    let loaded = shared.channel.loaded(now);
    let mut present = HashSet::new();
    let mut started = Vec::new();
    {
        let mut known = shared.known.lock().unwrap_or_else(|p| p.into_inner());
        for entry in fs::read_dir(&folder).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|s| valid_name(s))
            else {
                continue;
            };
            present.insert(id.to_owned());
            if known.contains_key(id) {
                continue;
            }
            let Some(value) = fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            else {
                continue;
            };
            let session = value["sessionId"].as_str().unwrap_or_default();
            if !loaded.contains(session) {
                let old = fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|at| now.duration_since(at).ok())
                    .is_some_and(|age| age > ORPHAN_GRACE);
                if old {
                    let _ = fs::remove_file(&path);
                }
                continue;
            }
            let Some(ask) = parse(id, &value) else {
                known.insert(
                    id.to_owned(),
                    Known {
                        request: request_id(id),
                        session: session.to_owned(),
                        shown: false,
                    },
                );
                continue;
            };
            let request = request_id(id);
            let ticket = shared.requests.register_native(
                request.clone(),
                ask.view.clone(),
                crate::hooks::now_ms(),
            );
            known.insert(
                id.to_owned(),
                Known {
                    request,
                    session: ask.session.clone(),
                    shown: ticket.is_some(),
                },
            );
            if let Some(ticket) = ticket {
                started.push((ask, ticket));
            }
        }
        let gone: Vec<String> = known
            .keys()
            .filter(|id| !present.contains(*id))
            .cloned()
            .collect();
        for id in gone {
            if let Some(entry) = known.remove(&id) {
                if entry.shown {
                    shared.requests.settle_native(&entry.request);
                }
            }
        }
    }
    for (ask, ticket) in started {
        let owned = shared.clone();
        let _ = thread::Builder::new()
            .name("DeepSeek plugin answer".into())
            .spawn(move || deliver(owned, ask, ticket));
    }
    publish(shared);
}

fn publish(shared: &Shared) {
    let mut sessions: Vec<String> = shared
        .known
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .values()
        .filter(|entry| entry.shown)
        .map(|entry| session_id(&entry.session))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    sessions.sort();
    shared
        .requests
        .set_native_waiting(AGENT.id, &sessions, crate::hooks::now_ms());
    (shared.wake)();
}

/// Waits for the HUD's decision on `ask`, and leaves it for the plugin.
fn deliver(shared: Arc<Shared>, ask: Ask, ticket: Ticket) {
    let decision = loop {
        if !shared.active.load(Ordering::Acquire) {
            return;
        }
        match ticket.response.recv_timeout(Duration::from_millis(100)) {
            Ok(Some(decision)) => break decision,
            Ok(None) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => continue,
        }
    };
    if !shared.active.load(Ordering::Acquire) {
        return;
    }
    let request = request_id(&ask.id);
    let session = session_id(&ask.session);
    let left = answer(&ask.kind, &decision).and_then(|value| match value {
        None => Ok(false),
        Some(value) => {
            let file = shared
                .channel
                .directory()
                .join("asks")
                .join(format!("{}.answer", ask.id));
            write_private(&file, value.to_string().as_bytes())
                .map(|()| true)
                .map_err(|_| {
                    l10n::text(
                        "回答没能交给 DeepSeek Harness 插件。",
                        "The answer could not be left for the DeepSeek Harness plugin.",
                    )
                })
        }
    });
    match left {
        Ok(false) => {}
        Ok(true) => shared.requests.note_native_delivery(
            &request,
            AGENT.id,
            &session,
            "confirmed",
            None,
            crate::hooks::now_ms(),
        ),
        Err(reason) => {
            shared.requests.note_native_delivery(
                &request,
                AGENT.id,
                &session,
                "failed",
                Some(reason),
                crate::hooks::now_ms(),
            );
            shared.requests.settle_native(&request);
        }
    }
}
