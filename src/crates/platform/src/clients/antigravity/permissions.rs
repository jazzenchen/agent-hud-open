//! Observes only approvals the existing native client is already asking about.
use super::service::{self as antigravity, Endpoint};
use agent_hud_agents::antigravity::AGENT;
use agent_hud_agents::{antigravity_permissions, AntigravityPermission};
use agent_hud_core::l10n::tr;
use agent_hud_core::requests::{Requests, Ticket};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::RecvTimeoutError,
        Arc, Condvar, Mutex, PoisonError,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
#[derive(Clone)]
struct Pending {
    endpoint: Endpoint,
    item: AntigravityPermission,
}
struct Shared {
    active: AtomicBool,
    pending: Mutex<HashMap<String, Pending>>,
    changed: Condvar,
    requests: Requests,
}
pub(crate) struct Observer {
    shared: Arc<Shared>,
}
impl Observer {
    pub fn start(requests: Requests) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            active: AtomicBool::new(true),
            pending: Mutex::new(HashMap::new()),
            changed: Condvar::new(),
            requests,
        });
        let owned = shared.clone();
        thread::Builder::new()
            .name("Antigravity approvals".into())
            .spawn(move || work(owned))
            .map_err(|_| "Antigravity approval observer could not start")?;
        Ok(Self { shared })
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        self.shared.active.store(false, Ordering::Release);
        self.shared.changed.notify_all();
        {
            let mut known = self
                .shared
                .pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            for id in known.keys() {
                self.shared.requests.settle_native(id);
            }
            known.clear();
        }
        self.shared
            .requests
            .set_native_waiting(AGENT.id, &[], crate::hooks::now_ms());
        // Blocking discovery/HTTP has its own bounded deadlines. Do not make a UI
        // disable/shutdown wait for that I/O; the invalidated generation checks its
        // active flag before publishing and again before writing any answer.
    }
}
fn read(endpoints: &[Endpoint], active: &AtomicBool) -> Result<Vec<Pending>, String> {
    let mut found = Vec::new();
    for endpoint in endpoints {
        if !active.load(Ordering::Acquire) {
            return Err("Observation stopped".into());
        }
        let body = endpoint.call("GetAllCascadeTrajectories", &json!({}))?;
        found.extend(
            antigravity_permissions(&body, &endpoint.identity(), crate::hooks::now_ms())?
                .into_iter()
                .map(|item| Pending {
                    endpoint: endpoint.clone(),
                    item,
                }),
        );
    }
    found.sort_by(|a, b| a.item.id.cmp(&b.item.id));
    Ok(found)
}
fn fetch(active: &AtomicBool) -> Result<Vec<Pending>, String> {
    let known = antigravity::endpoints(false)?;
    match read(&known, active) {
        Ok(found) => Ok(found),
        Err(error) => {
            if !active.load(Ordering::Acquire) {
                return Err(error);
            }
            let current = antigravity::endpoints(true)?;
            if current == known {
                return Err(error);
            }
            read(&current, active)
        }
    }
}
fn work(shared: Arc<Shared>) {
    let mut waiters: Vec<JoinHandle<()>> = Vec::new();
    while shared.active.load(Ordering::Acquire) {
        let fetched = fetch(&shared.active);
        let mut known = shared
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !shared.active.load(Ordering::Acquire) {
            break;
        }
        let mut tickets = Vec::new();
        let mut sessions = Vec::new();
        match fetched {
            Ok(pending) => {
                let ids = pending
                    .iter()
                    .map(|p| p.item.id.clone())
                    .collect::<HashSet<_>>();
                known.retain(|id, _| {
                    if ids.contains(id) {
                        true
                    } else {
                        shared.requests.settle_native(id);
                        false
                    }
                });
                for pending in pending {
                    sessions.push(pending.item.session_id.clone());
                    if let Some(ticket) = shared.requests.register_native(
                        pending.item.id.clone(),
                        pending.item.view.clone(),
                        crate::hooks::now_ms(),
                    ) {
                        tickets.push((pending.clone(), ticket));
                    }
                    known.insert(pending.item.id.clone(), pending);
                }
            }
            // Silence is unknown, not proof of absence. Withdraw cards without
            // clearing a dismissal until a complete successful read settles it.
            Err(_) => {
                for id in known.keys() {
                    shared.requests.cancel(id);
                }
            }
        }
        shared
            .requests
            .set_native_waiting(AGENT.id, &sessions, crate::hooks::now_ms());
        drop(known);
        waiters.retain(|thread| !thread.is_finished());
        for (pending, ticket) in tickets {
            let owned = shared.clone();
            let id = ticket.id.clone();
            match thread::Builder::new()
                .name("Antigravity approval answer".into())
                .spawn(move || answer(owned, pending, ticket))
            {
                Ok(worker) => waiters.push(worker),
                Err(_) => shared.requests.cancel(&id),
            }
        }
        let guard = shared
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if shared.active.load(Ordering::Acquire) {
            let _ = shared.changed.wait_timeout(guard, Duration::from_secs(2));
        }
    }
    for waiter in waiters {
        let _ = waiter.join();
    }
}
enum Reply {
    Failed(String),
    Uncertain(String),
    Stopped,
}
fn resolve(shared: &Shared, pending: &Pending, decision: &Value) -> Result<(), Reply> {
    let fail = Reply::Failed;
    if !shared.active.load(Ordering::Acquire) {
        return Err(Reply::Stopped);
    }
    let endpoints = antigravity::endpoints(true).map_err(|_| {
        fail(tr!(
            "Antigravity's waiting permission could not be revalidated"
        ))
    })?;
    if !endpoints.contains(&pending.endpoint) {
        return Err(fail(tr!("Antigravity's permission service changed")));
    }
    let current = pending
        .endpoint
        .call("GetAllCascadeTrajectories", &json!({}))
        .map_err(|_| {
            fail(tr!(
                "Antigravity's waiting permission could not be revalidated"
            ))
        })?;
    let current = antigravity_permissions(
        &current,
        &pending.endpoint.identity(),
        crate::hooks::now_ms(),
    )
    .map_err(|_| fail(tr!("Antigravity's permission summary changed")))?;
    if !current
        .iter()
        .any(|item| item.id == pending.item.id && item.permission == pending.item.permission)
    {
        return Err(fail(tr!("Antigravity's request was withdrawn or changed")));
    }
    let response = pending.item.response(decision).map_err(Reply::Failed)?;
    let Some(body) = response else {
        return Ok(());
    };
    if !shared.active.load(Ordering::Acquire) {
        return Err(Reply::Stopped);
    }
    pending
        .endpoint
        .call("HandleCascadeUserInteraction", &body)
        .map_err(|_| {
            Reply::Uncertain(tr!(
                "Antigravity did not confirm receipt of the permission answer"
            ))
        })?;
    Ok(())
}
fn answer(shared: Arc<Shared>, pending: Pending, ticket: Ticket) {
    while shared.active.load(Ordering::Acquire) {
        let decision = match ticket.response.recv_timeout(Duration::from_millis(100)) {
            Ok(Some(decision)) => decision,
            Ok(None) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => continue,
        };
        let delivery = match resolve(&shared, &pending, &decision) {
            Ok(()) => Some(("confirmed", None)),
            Err(Reply::Failed(reason)) => Some(("failed", Some(reason))),
            Err(Reply::Uncertain(reason)) => Some(("uncertain", Some(reason))),
            Err(Reply::Stopped) => None,
        };
        if shared.active.load(Ordering::Acquire) {
            if let Some((state, reason)) = delivery {
                shared.requests.note_native_delivery(
                    &pending.item.id,
                    AGENT.id,
                    &pending.item.session_id,
                    state,
                    reason,
                    crate::hooks::now_ms(),
                );
                // A fresh complete native snapshot may offer a failed answer again.
                // This clears only our local dismissal; it never resends a decision.
                if state == "failed" {
                    shared.requests.settle_native(&pending.item.id);
                }
            }
        }
        return;
    }
}
