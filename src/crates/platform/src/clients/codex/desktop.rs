//! Public native approvals/questions only. This observer shares generic Desktop
//! framing with other users, but never starts/resumes a conversation or engine.
mod cards;
// The Codex daemon's control socket is a Unix socket; Windows reads none.
#[cfg(unix)]
mod daemon;
#[cfg(windows)]
#[path = "desktop/daemon_windows.rs"]
mod daemon;
mod pending;
mod reply;
mod stream;
mod turns;
pub(crate) use daemon::daemon_tui_port_for as daemon_tui_port;

use super::desktop_io::{endpoint, DesktopIpc, IpcError, ShutdownHandle};
use crate::hooks::Wake;
use agent_hud_agents::codex::AGENT;
use agent_hud_core::requests::{Requests, Ticket};
use pending::Pending;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, PoisonError,
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
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
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
            let Some(id) = target["id"]
                .as_str()
                .filter(|id| crate::hooks::origins::is_uuid(id))
            else {
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
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
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

impl Shared {
    fn wanted(&self) -> Vec<String> {
        let now = Instant::now();
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
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
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if self.active.load(Ordering::Acquire) {
            let _ = self.changed.wait_timeout(state, duration);
        }
    }
    fn forget_connection(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
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
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
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
}

// The fixtures stand up a Unix socket in place of Codex Desktop.
#[cfg(all(test, unix))]
mod tests;
