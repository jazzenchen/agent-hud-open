//! Public native approvals/questions only. This observer shares generic Desktop
//! framing with other users, but never starts/resumes a conversation or engine.
// The Codex daemon's control socket is a Unix socket; Windows reads none.
#[cfg(unix)]
mod daemon;
#[cfg(windows)]
#[path = "desktop/daemon_windows.rs"]
mod daemon;
mod pending;
mod stream;
pub(crate) use daemon::daemon_tui_origin_for as daemon_tui_origin;

use super::desktop_io::{endpoint, DesktopIpc, IpcError, ShutdownHandle};
use crate::hooks::Wake;
use agent_hud_agents::codex::AGENT;
use agent_hud_core::l10n;
use agent_hud_core::requests::{Requests, Ticket};
use pending::Pending;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

struct ThreadState {
    owner: String,
    state: Value,
}
struct LivePending {
    item: Pending,
    ticket: Option<Ticket>,
}
struct State {
    reported: HashMap<String, Instant>,
    known: HashMap<String, ThreadState>,
    pending: HashMap<String, LivePending>,
    connection: Option<ShutdownHandle>,
    answer_connections: HashMap<String, ShutdownHandle>,
}
struct Shared {
    active: AtomicBool,
    state: Mutex<State>,
    changed: Condvar,
    requests: Requests,
    wake: Wake,
    data: PathBuf,
}

pub struct CodexDesktopObserver {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
impl CodexDesktopObserver {
    pub fn start(data_directory: PathBuf, requests: Requests, wake: Wake) -> Result<Self, String> {
        if !data_directory.is_absolute() {
            return Err("Codex data directory must be absolute".into());
        }
        let shared = Arc::new(Shared {
            active: AtomicBool::new(true),
            state: Mutex::new(State {
                reported: HashMap::new(),
                known: HashMap::new(),
                pending: HashMap::new(),
                connection: None,
                answer_connections: HashMap::new(),
            }),
            changed: Condvar::new(),
            requests,
            wake,
            data: data_directory,
        });
        let owned = shared.clone();
        let worker = thread::Builder::new()
            .name("Codex Desktop approvals".into())
            .spawn(move || stream::work(owned))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }
    pub fn update_sessions(&self, sessions: &[Value]) {
        let now = Instant::now();
        let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
        let mut changed = false;
        // Only threads Codex Desktop holds are followed: a session in flight
        // whose way back is its Desktop thread, sub-agents included (each is
        // its own session row); a CLI or daemon thread has no owner to ask,
        // and another app-server client's thread opens in Codex but is not
        // Desktop's (agent-hud-native-open CodexDesktopPermissionObserver.threadIDs).
        for session in sessions {
            let target = &session["navigationTarget"];
            if session["source"] != AGENT.id
                || !matches!(
                    target["kind"].as_str(),
                    Some("codex-thread" | "codexThread")
                )
                || !matches!(session["state"].as_str(), Some("running" | "waiting"))
                || agent_hud_agents::naming::clients::codex_other_client(session["client"].as_str())
            {
                continue;
            }
            let Some(id) = target["id"].as_str().filter(|id| valid_id(id)) else {
                continue;
            };
            changed |= state.reported.insert(id.into(), now).is_none();
        }
        state
            .reported
            .retain(|_, last| now.duration_since(*last) < Duration::from_secs(600));
        if changed {
            self.shared.changed.notify_all();
        }
    }
    pub fn stop(&mut self) {
        if !self.shared.active.swap(false, Ordering::AcqRel) {
            return;
        }
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(socket) = state.connection.take() {
                socket.shutdown();
            }
            for socket in state.answer_connections.values() {
                socket.shutdown();
            }
            state.answer_connections.clear();
            for id in state.pending.keys() {
                self.shared.requests.settle_native(id);
            }
            state.pending.clear();
            state.known.clear();
            state.reported.clear();
            self.shared.changed.notify_all();
        }
        self.shared.publish_waiting();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for CodexDesktopObserver {
    fn drop(&mut self) {
        self.stop();
    }
}
fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

impl Shared {
    fn wanted(&self) -> Vec<String> {
        let now = Instant::now();
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state
            .reported
            .retain(|_, at| now.duration_since(*at) < Duration::from_secs(600));
        let mut ids = state.reported.keys().cloned().collect::<Vec<_>>();
        ids.extend(
            state
                .pending
                .values()
                .map(|value| value.item.thread.clone()),
        );
        ids.sort();
        ids.dedup();
        ids
    }
    fn pause(&self, duration: Duration) {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if self.active.load(Ordering::Acquire) {
            let _ = self.changed.wait_timeout(state, duration);
        }
    }
    fn forget_connection(&self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.connection = None;
        state.known.clear();
        // Unknown/disconnected withdraws visible cards but does not settle dismissed
        // identities. Retaining pending prevents a reconnect from asking twice.
        for (id, pending) in &mut state.pending {
            self.requests.cancel(id);
            pending.ticket = None;
        }
        drop(state);
        self.publish_waiting();
        (self.wake)();
    }
    fn publish_waiting(&self) {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let mut sessions = if self.active.load(Ordering::Acquire) {
            state
                .known
                .iter()
                .filter_map(|(thread, known)| {
                    let status = &known.state["threadRuntimeStatus"];
                    (status["type"] == "active"
                        && status["activeFlags"].as_array().is_some_and(|flags| {
                            flags.iter().any(|flag| {
                                matches!(
                                    flag.as_str(),
                                    Some("waitingOnApproval" | "waitingOnUserInput")
                                )
                            })
                        }))
                    .then(|| thread.clone())
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        sessions.sort();
        // Keep this complete-list replacement ordered with disconnect/stop. The
        // Requests owner changes phase only; this creates no actionable request.
        self.requests
            .set_native_waiting(AGENT.id, &sessions, crate::hooks::now_ms());
    }
    fn reconcile(&self, thread: &str, owner: &str, state_value: Value) {
        if !self.active.load(Ordering::Acquire) {
            return;
        }
        if let Ok(turns) = pending::turns(&state_value, true) {
            for turn in turns {
                if let (Some(id), Some(params)) = (
                    turn["turnId"].as_str(),
                    turn.get("params").filter(|value| value.is_object()),
                ) {
                    self.requests.note_reviewer(
                        AGENT.id,
                        thread,
                        id,
                        params["approvalsReviewer"].as_str().map(str::to_owned),
                        crate::hooks::now_ms(),
                        crate::hooks::now_ms(),
                    );
                }
            }
        }
        let parsed = pending::parse(&state_value, thread, owner);
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if !self.active.load(Ordering::Acquire) {
            return;
        }
        state.known.insert(
            thread.into(),
            ThreadState {
                owner: owner.into(),
                state: state_value,
            },
        );
        let Ok(items) = parsed else {
            for (id, pending) in &mut state.pending {
                if pending.item.thread == thread {
                    self.requests.cancel(id);
                    pending.ticket = None;
                }
            }
            drop(state);
            self.publish_waiting();
            return;
        }; // Unknown withdraws cards; only an observed absence settles dismissals.
        let ids = items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let removed = state
            .pending
            .iter()
            .filter(|(_, value)| {
                value.item.thread == thread && !ids.contains(value.item.id.as_str())
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in removed {
            self.requests.settle_native(&id);
            state.pending.remove(&id);
        }
        for item in items {
            let ticket = self.requests.register_native(
                item.id.clone(),
                item.view.clone(),
                crate::hooks::now_ms(),
            );
            match state.pending.get_mut(&item.id) {
                Some(existing) => {
                    existing.item = item;
                    if ticket.is_some() {
                        existing.ticket = ticket;
                    }
                }
                None => {
                    state
                        .pending
                        .insert(item.id.clone(), LivePending { item, ticket });
                }
            }
        }
        drop(state);
        self.publish_waiting();
    }
    fn answers(self: &Arc<Self>) {
        let mut ready = Vec::new();
        {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            for live in state.pending.values_mut() {
                let Some(ticket) = live.ticket.as_ref() else {
                    continue;
                };
                match ticket.response.try_recv() {
                    Ok(Some(decision)) => {
                        ready.push((live.item.clone(), decision));
                        live.ticket = None;
                    }
                    Ok(None) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        live.ticket = None
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
            }
        }
        for (item, decision) in ready {
            if !self.active.load(Ordering::Acquire) {
                return;
            }
            let shared = self.clone();
            let _ = thread::Builder::new()
                .name("Codex Desktop answer".into())
                .spawn(move || {
                    let result = shared.resolve(&item, &decision);
                    shared
                        .state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .answer_connections
                        .remove(&item.id);
                    if shared.active.load(Ordering::Acquire) {
                        match result {
                            Ok(()) => shared.requests.note_native_delivery(
                                &item.id,
                                AGENT.id,
                                &item.thread,
                                "confirmed",
                                None,
                                crate::hooks::now_ms(),
                            ),
                            Err(ReplyFailure::Failed(reason)) => {
                                shared.requests.note_native_delivery(
                                    &item.id,
                                    AGENT.id,
                                    &item.thread,
                                    "failed",
                                    Some(reason),
                                    crate::hooks::now_ms(),
                                )
                            }
                            Err(ReplyFailure::Uncertain(reason)) => {
                                shared.requests.note_native_delivery(
                                    &item.id,
                                    AGENT.id,
                                    &item.thread,
                                    "uncertain",
                                    Some(reason),
                                    crate::hooks::now_ms(),
                                )
                            }
                            Err(ReplyFailure::Cancelled) => {}
                        }
                    }
                });
        }
    }
    fn resolve(&self, item: &Pending, decision: &Value) -> Result<(), ReplyFailure> {
        let (response, method, state_value) = {
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            let known = state.known.get(&item.thread).ok_or(IpcError::Unavailable)?;
            if known.owner != item.owner {
                return Err(IpcError::Unavailable.into());
            }
            let current = pending::parse(&known.state, &item.thread, &item.owner)
                .map_err(|_| IpcError::Protocol)?;
            if !current
                .iter()
                .any(|value| value.id == item.id && value.wire == item.wire)
            {
                return Err(IpcError::Unavailable.into());
            }
            let (method, response) = pending::answer(item, decision, &known.state)?;
            (response, method, known.state["cwd"].clone())
        };
        if response.is_null() {
            return Err(ReplyFailure::Cancelled);
        }
        let mut connection = DesktopIpc::connect(&endpoint(&self.data), Duration::from_secs(3))?;
        {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            if !self.active.load(Ordering::Acquire) {
                return Err(ReplyFailure::Cancelled);
            }
            state
                .answer_connections
                .insert(item.id.clone(), connection.shutdown_handle()?);
        }
        let initialized = connection.request(
            "initialize",
            0,
            "initializing-client",
            None,
            serde_json::json!({"clientType":"agent-hud"}),
            None,
        )?;
        let client = initialized["result"]["clientId"]
            .as_str()
            .ok_or(IpcError::Protocol)?
            .to_string();
        let discovery = connection.request(
            "thread-owner-discovery",
            1,
            &client,
            None,
            serde_json::json!({"hostId":"local","conversationId":item.thread}),
            None,
        )?;
        if !self.active.load(Ordering::Acquire) {
            return Err(ReplyFailure::Cancelled);
        }
        if discovery["handledByClientId"] != item.owner {
            return Err(IpcError::Unavailable.into());
        }
        let params = if method == pending::ELICITATION_METHOD {
            serde_json::json!({"conversationId":item.thread,"requestId":item.request_id,"response":response})
        } else {
            let text = response.as_str().ok_or(IpcError::Protocol)?;
            let id = super::desktop_io::request_id()?;
            let mut context = serde_json::json!({"prompt":text,"turnTrigger":"send_user_message_async_question","addedFiles":[],"fileAttachments":[],"ideContext":null,"imageAttachments":[]});
            let mut restore = serde_json::json!({"id":id,"text":text,"context":context,"createdAt":crate::hooks::now_ms()});
            if let Some(cwd) = state_value.as_str() {
                context["workspaceRoots"] = serde_json::json!([cwd]);
                restore["context"] = context;
                restore["cwd"] = serde_json::json!(cwd);
            }
            serde_json::json!({"conversationId":item.thread,"clientUserMessageId":id,"input":[{"type":"text","text":text,"text_elements":[]}],"attachments":[],"restoreMessage":restore})
        };
        // A reply attempt is never automatically retried. Requests retains its
        // dismissed identity until the owner actually settles this exact request.
        if !self.active.load(Ordering::Acquire) {
            return Err(ReplyFailure::Cancelled);
        }
        let mut wrote = false;
        let receipt = connection
            .request(
                method,
                1,
                &client,
                Some(&item.owner),
                params,
                Some(&mut || wrote = true),
            )
            .map_err(|error| {
                if !self.active.load(Ordering::Acquire) {
                    ReplyFailure::Cancelled
                } else {
                    match &error {
                        IpcError::Rejected(reason)
                            if !matches!(
                                reason.as_str(),
                                "request-timeout" | "server-closed" | "client-disconnected"
                            ) =>
                        {
                            ReplyFailure::Failed(error.to_string())
                        }
                        _ if wrote => ReplyFailure::Uncertain(error.to_string()),
                        _ => ReplyFailure::Failed(error.to_string()),
                    }
                }
            })?;
        if receipt["handledByClientId"] != item.owner || receipt["result"]["method"] != method {
            return Err(ReplyFailure::Uncertain(l10n::text(
                "Codex Desktop 的回复与请求不匹配",
                "Desktop reply identity did not match",
            )));
        }
        if method == pending::ELICITATION_METHOD {
            if receipt["result"]["result"]["ok"] != true {
                return Err(ReplyFailure::Uncertain(l10n::text(
                    "Codex Desktop 没有确认这个审批",
                    "Desktop did not confirm this approval",
                )));
            }
        } else if receipt["result"]["result"]["result"]["turnId"]
            != item.turn_id.as_deref().unwrap_or("")
        {
            return Err(ReplyFailure::Uncertain(l10n::text(
                "Codex Desktop 没有确认对应的提问轮次",
                "Desktop did not confirm the exact question turn",
            )));
        }
        Ok(())
    }
}
enum ReplyFailure {
    Failed(String),
    Uncertain(String),
    Cancelled,
}
impl From<IpcError> for ReplyFailure {
    fn from(error: IpcError) -> Self {
        Self::Failed(error.to_string())
    }
}

// The fixtures stand up a Unix socket in place of Codex Desktop.
#[cfg(all(test, unix))]
mod tests;
