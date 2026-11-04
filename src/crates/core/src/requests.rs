//! One ephemeral owner for requests waiting on the user. Client adapters own
//! decoding/encoding; transports own connections. No request body is persisted.
use agent_hud_agents::Request;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{mpsc, Arc, Mutex, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};

mod answer;
mod deliveries;
mod views;

type Wake = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct Requests(Arc<Shared>);

struct Shared {
    state: Mutex<State>,
    wake: Mutex<Option<Wake>>,
}
struct State {
    epoch: u128,
    sequence: u64,
    revision: u64,
    hold_ms: u64,
    pending: VecDeque<Pending>,
    dismissed: HashSet<String>,
    reviewers: HashMap<(String, String, String), Reviewer>,
    deliveries: VecDeque<Value>,
    native_waiting: HashMap<String, HashSet<String>>,
}
struct Reviewer {
    value: Option<String>,
    at: i64,
}
// Only a handful of requests wait at a time, so the larger variant stays inline.
#[allow(clippy::large_enum_variant)]
enum Content {
    Hook(Request),
    Native(Value),
}
struct Pending {
    id: String,
    content: Content,
    created_ms: i64,
    deadline_ms: i64,
    sender: mpsc::Sender<Option<Value>>,
}
pub struct Ticket {
    pub id: String,
    pub response: mpsc::Receiver<Option<Value>>,
    pub deadline_ms: i64,
}

impl Default for Requests {
    fn default() -> Self {
        Self(Arc::new(Shared {
            state: Mutex::new(State {
                epoch: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                sequence: 0,
                revision: 0,
                hold_ms: 600_000,
                pending: VecDeque::new(),
                dismissed: HashSet::new(),
                reviewers: HashMap::new(),
                deliveries: VecDeque::new(),
                native_waiting: HashMap::new(),
            }),
            wake: Mutex::new(None),
        }))
    }
}

impl Requests {
    pub fn set_waker(&self, wake: Wake) {
        *self.0.wake.lock().unwrap_or_else(PoisonError::into_inner) = Some(wake);
    }
    fn wake(&self) {
        let wake = self
            .0
            .wake
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(wake) = wake {
            wake();
        }
    }
    pub fn register(&self, request: Request, now_ms: i64) -> Ticket {
        let (sender, response) = mpsc::channel();
        let ticket = {
            let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
            state.sequence = state.sequence.wrapping_add(1);
            let id = format!("hook:{}:{}:{}", request.source, state.epoch, state.sequence);
            let deadline_ms = now_ms.saturating_add(state.hold_ms as i64);
            state.pending.push_back(Pending {
                id: id.clone(),
                content: Content::Hook(request),
                created_ms: now_ms,
                deadline_ms,
                sender,
            });
            state.revision = state.revision.wrapping_add(1);
            Ticket {
                id,
                response,
                deadline_ms,
            }
        };
        self.wake();
        ticket
    }
    /// Native adapters provide the standardized display and receive the user's
    /// standardized decision. Their source-specific reply remains in the adapter.
    pub fn register_native(&self, id: String, mut view: Value, now_ms: i64) -> Option<Ticket> {
        let (sender, response) = mpsc::channel();
        let ticket = {
            let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
            if state.dismissed.contains(&id) || state.pending.iter().any(|p| p.id == id) {
                return None;
            }
            let deadline_ms = now_ms.saturating_add(state.hold_ms as i64);
            view["id"] = Value::String(id.clone());
            state.pending.push_back(Pending {
                id: id.clone(),
                content: Content::Native(view),
                created_ms: now_ms,
                deadline_ms,
                sender,
            });
            state.revision = state.revision.wrapping_add(1);
            Ticket {
                id,
                response,
                deadline_ms,
            }
        };
        self.wake();
        Some(ticket)
    }
    /// Gives a waiting native request its adapter's newer card, such as a
    /// reason it can only be answered in the client that changed; its wait
    /// and its ticket stay.
    pub fn refresh_native(&self, id: &str, mut view: Value) {
        let changed = {
            let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
            view["id"] = Value::String(id.into());
            match state.pending.iter_mut().find(|p| p.id == id) {
                Some(Pending {
                    content: Content::Native(shown),
                    ..
                }) if *shown != view => {
                    *shown = view;
                    state.revision = state.revision.wrapping_add(1);
                    true
                }
                _ => false,
            }
        };
        if changed {
            self.wake();
        }
    }
    pub fn cancel(&self, id: &str) {
        let removed = {
            let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
            let item = state
                .pending
                .iter()
                .position(|p| p.id == id)
                .and_then(|index| state.pending.remove(index));
            if item.is_some() {
                state.revision = state.revision.wrapping_add(1);
            }
            item
        };
        if let Some(item) = removed {
            let _ = item.sender.send(None);
            self.wake();
        }
    }
    /// Explicit source settlement removes a dismissal as well as its live card.
    pub fn settle_native(&self, id: &str) {
        self.cancel(id);
        self.0
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .dismissed
            .remove(id);
    }
    pub fn expire(&self, now_ms: i64) {
        let expired = {
            let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
            let mut expired = Vec::new();
            let mut index = 0;
            while index < state.pending.len() {
                if state.pending[index].deadline_ms <= now_ms {
                    let item = state.pending.remove(index).expect("pending index exists");
                    if matches!(&item.content, Content::Native(_)) {
                        state.dismissed.insert(item.id.clone());
                    }
                    expired.push(item);
                } else {
                    index += 1;
                }
            }
            if !expired.is_empty() {
                state.revision = state.revision.wrapping_add(1);
            }
            expired
        };
        if !expired.is_empty() {
            for item in expired {
                let _ = item.sender.send(None);
            }
            self.wake();
        }
    }
    pub fn clear(&self) {
        let (pending, changed) = {
            let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
            let changed = !state.pending.is_empty() || !state.native_waiting.is_empty();
            if changed {
                state.revision = state.revision.wrapping_add(1);
            }
            state.native_waiting.clear();
            state.dismissed.clear();
            (std::mem::take(&mut state.pending), changed)
        };
        for item in pending {
            let _ = item.sender.send(None);
        }
        if changed {
            self.wake();
        }
    }
    pub fn set_hold_ms(&self, hold_ms: u64) {
        let changed = {
            let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
            if state.hold_ms == hold_ms {
                false
            } else {
                state.hold_ms = hold_ms;
                for item in &mut state.pending {
                    item.deadline_ms = item.created_ms.saturating_add(hold_ms as i64);
                }
                true
            }
        };
        if changed {
            self.wake();
        }
    }
}
