//! One ephemeral owner for requests waiting on the user. Client adapters own
//! decoding/encoding; transports own connections. No request body is persisted.
use crate::l10n;
use agent_hud_agents::Request;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{mpsc, Arc, Mutex, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};

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
    /// The card of the request `id` while it waits, once `decision` is one it
    /// takes; nothing is answered. Lets an adapter act on the answer before
    /// the card goes, and keep it waiting when the client refuses.
    pub fn checked(&self, id: &str, decision: &Value, now_ms: i64) -> Result<Value, String> {
        self.expire(now_ms);
        let state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        let item =
            state.pending.iter().find(|p| p.id == id).ok_or_else(|| {
                l10n::text("这个请求已不再等待", "This request is no longer waiting")
            })?;
        let view = match &item.content {
            Content::Hook(request) => request.view(id, item.created_ms),
            Content::Native(view) => view.clone(),
        };
        validate_native_decision(&view, decision)?;
        Ok(view)
    }
    pub fn answer(&self, id: &str, decision: &Value, now_ms: i64) -> Result<(), String> {
        self.expire(now_ms);
        let (item, response) = {
            let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
            let index = state
                .pending
                .iter()
                .position(|p| p.id == id)
                .ok_or_else(|| {
                    l10n::text("这个请求已不再等待", "This request is no longer waiting")
                })?;
            let response = match &state.pending[index].content {
                Content::Hook(request) => {
                    validate_native_decision(
                        &request.view(id, state.pending[index].created_ms),
                        decision,
                    )?;
                    request.response(decision)?
                }
                Content::Native(view) => {
                    validate_native_decision(view, decision)?;
                    (decision.get("kind").and_then(Value::as_str) != Some("leave"))
                        .then(|| decision.clone())
                }
            };
            let item = state.pending.remove(index).expect("pending index exists");
            if matches!(&item.content, Content::Native(_)) {
                state.dismissed.insert(id.into());
            }
            state.revision = state.revision.wrapping_add(1);
            (item, response)
        };
        let delivered = item.sender.send(response).is_ok();
        self.wake();
        if delivered {
            Ok(())
        } else {
            Err(l10n::text(
                "客户端在回答送达前已停止等待",
                "The client stopped waiting before this answer was delivered",
            ))
        }
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
    /// The first time a waiting request expires or a delivery leaves the
    /// hour it is kept for; `views` and `deliveries` change then.
    pub fn next_deadline(&self) -> Option<i64> {
        let state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        let deliveries = state.deliveries.iter().filter_map(|row| {
            row["at"]
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|at| at.timestamp_millis().saturating_add(3_600_000))
        });
        state
            .pending
            .iter()
            .map(|item| item.deadline_ms)
            .chain(deliveries)
            .min()
    }
    pub fn revision(&self) -> u64 {
        self.0
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .revision
    }
    pub fn deadline(&self, id: &str) -> Option<i64> {
        self.0
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pending
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.deadline_ms)
    }
    pub fn waiting_at(&self, source: &str, session: &str) -> Option<i64> {
        self.0
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pending
            .iter()
            .filter(|item| match &item.content {
                Content::Hook(request) => request.source == source && request.session_id == session,
                Content::Native(view) => view["source"] == source && view["sessionId"] == session,
            })
            .map(|item| item.created_ms)
            .min()
    }
    pub fn attention(&self, source: &str, session: &str) -> Vec<Value> {
        self.0.state.lock().unwrap_or_else(PoisonError::into_inner).pending.iter().filter_map(|item| {
            let kind = match &item.content {
                Content::Hook(request) if request.source == source && request.session_id == session => request.kind.as_str(),
                Content::Native(view) if view["source"] == source && view["sessionId"] == session => view["kind"].as_str().unwrap_or("permission"),
                _ => return None,
            };
            Some(serde_json::json!({"id":item.id,"sessionId":session,"kind":kind,"at":chrono::DateTime::<chrono::Utc>::from_timestamp_millis(item.created_ms).map(|v|v.to_rfc3339())}))
        }).collect()
    }
    /// Native waiting flags may exist without a HUD-answerable request. Keep
    /// phase evidence here without fabricating a card or a transcript timestamp.
    pub fn set_native_waiting(&self, source: &str, sessions: &[String], _now_ms: i64) {
        let next: HashSet<String> = sessions
            .iter()
            .filter(|id| !id.is_empty())
            .cloned()
            .collect();
        let changed = {
            let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
            if state
                .native_waiting
                .get(source)
                .is_none_or(|old| *old != next)
                && (!next.is_empty() || state.native_waiting.contains_key(source))
            {
                if next.is_empty() {
                    state.native_waiting.remove(source);
                } else {
                    state.native_waiting.insert(source.into(), next);
                }
                state.revision = state.revision.wrapping_add(1);
                true
            } else {
                false
            }
        };
        if changed {
            self.wake();
        }
    }
    pub fn native_waiting_sessions(&self) -> HashMap<String, HashSet<String>> {
        self.0
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .native_waiting
            .clone()
    }
    /// Small per-snapshot index: presentation never locks this owner once for
    /// every historical transcript, or clones tool payloads just to sort sessions.
    pub fn session_attention(&self) -> HashMap<String, HashMap<String, Vec<Value>>> {
        let state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        let mut result: HashMap<String, HashMap<String, Vec<Value>>> = HashMap::new();
        for item in &state.pending {
            let (source, session, kind) = match &item.content {
                Content::Hook(request) => (
                    request.source.as_str(),
                    request.session_id.as_str(),
                    request.kind.as_str(),
                ),
                Content::Native(view) => {
                    match (view["source"].as_str(), view["sessionId"].as_str()) {
                        (Some(source), Some(session)) => (
                            source,
                            session,
                            view["kind"].as_str().unwrap_or("permission"),
                        ),
                        _ => continue,
                    }
                }
            };
            result.entry(source.into()).or_default().entry(session.into()).or_default().push(
                serde_json::json!({"id":item.id,"sessionId":session,"kind":kind,"at":chrono::DateTime::<chrono::Utc>::from_timestamp_millis(item.created_ms).map(|v|v.to_rfc3339())})
            );
        }
        result
    }
    pub fn views(&self, now_ms: i64) -> Vec<Value> {
        self.expire(now_ms);
        self.0
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pending
            .iter()
            .map(|item| match &item.content {
                Content::Hook(request) => request.view(&item.id, item.created_ms),
                Content::Native(view) => view.clone(),
            })
            .collect()
    }
    /// A native response is confirmed only by its source adapter's actual reply,
    /// not by enqueueing a decision here. Receipts are ephemeral and never resend.
    pub fn note_native_delivery(
        &self,
        id: &str,
        source: &str,
        session: &str,
        delivery: &str,
        reason: Option<String>,
        now_ms: i64,
    ) {
        if !matches!(delivery, "confirmed" | "failed" | "uncertain") {
            return;
        }
        let at = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(now_ms)
            .map(|date| date.to_rfc3339());
        {
            let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
            state.deliveries.retain(|row| row["id"] != id);
            state.deliveries.push_back(serde_json::json!({"id":id,"source":source,"sessionId":session,"state":delivery,"reason":reason,"at":at}));
            while state.deliveries.len() > 256 {
                state.deliveries.pop_front();
            }
            state.revision = state.revision.wrapping_add(1);
        }
        self.wake();
    }
    pub fn deliveries(&self, now_ms: i64) -> Vec<Value> {
        let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        let before = state.deliveries.len();
        state.deliveries.retain(|row| {
            row["at"]
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .is_some_and(|at| now_ms.saturating_sub(at.timestamp_millis()) < 3_600_000)
        });
        if before != state.deliveries.len() {
            state.revision = state.revision.wrapping_add(1);
        }
        state.deliveries.iter().cloned().collect()
    }
    /// Recent source turn contexts, including an explicitly absent reviewer.
    /// Unknown is distinct from an explicit context with no user reviewer.
    pub fn note_reviewer(
        &self,
        source: &str,
        session: &str,
        turn: &str,
        reviewer: Option<String>,
        at_ms: i64,
        now_ms: i64,
    ) {
        if now_ms.saturating_sub(at_ms) >= 86_400_000 {
            return;
        }
        let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        let key = (source.into(), session.into(), turn.into());
        if state.reviewers.get(&key).is_some_and(|old| old.at > at_ms) {
            return;
        }
        state
            .reviewers
            .retain(|_, value| now_ms.saturating_sub(value.at) < 86_400_000);
        state.reviewers.insert(
            key,
            Reviewer {
                value: reviewer,
                at: at_ms,
            },
        );
        if state.reviewers.len() > 512 {
            if let Some(oldest) = state
                .reviewers
                .iter()
                .min_by_key(|(_, v)| v.at)
                .map(|(key, _)| key.clone())
            {
                state.reviewers.remove(&oldest);
            }
        }
    }
    pub fn reviewer(
        &self,
        source: &str,
        session: &str,
        turn: &str,
        now_ms: i64,
    ) -> Option<Option<String>> {
        self.0
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .reviewers
            .get(&(source.into(), session.into(), turn.into()))
            .filter(|entry| now_ms.saturating_sub(entry.at) < 86_400_000)
            .map(|entry| entry.value.clone())
    }
}

fn validate_native_decision(view: &Value, decision: &Value) -> Result<(), String> {
    let kind = decision
        .get("kind")
        .and_then(Value::as_str)
        .ok_or("Decision kind is required")?;
    let request = view
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("permission");
    let allowed = match kind {
        "leave" => true,
        "allow" | "deny" => request == "permission",
        "always" => {
            request == "permission"
                && view.get("allowsAlways").and_then(Value::as_bool) == Some(true)
        }
        "answer" if request == "question" && view["answersQuestion"] != false => {
            return validate_native_answers(view, decision)
        }
        "approve_plan" => {
            request == "plan"
                && view["answersPlan"] != false
                && decision.get("acceptEdits").is_none_or(Value::is_boolean)
        }
        "keep_planning" => {
            request == "plan"
                && view["answersPlan"] != false
                && decision.get("message").is_none_or(Value::is_string)
        }
        _ => false,
    };
    if allowed {
        Ok(())
    } else {
        Err("This decision is not supported by this request".into())
    }
}

fn validate_native_answers(view: &Value, decision: &Value) -> Result<(), String> {
    let answers = decision["answers"]
        .as_object()
        .ok_or("Answers must be an object")?;
    let questions = view["questions"]
        .as_array()
        .ok_or("This request has no questions")?;
    for (key, answer) in answers {
        let question = questions
            .iter()
            .find(|question| question["answerKey"] == *key)
            .ok_or("Unknown answer key")?;
        let answer = answer.as_object().ok_or("Answer must be an object")?;
        let selected = match answer.get("selected") {
            Some(value) => value
                .as_array()
                .ok_or("Selected options must be an array")?
                .iter()
                .map(|value| value.as_str().ok_or("Selected options must be strings"))
                .collect::<Result<Vec<_>, _>>()?,
            None => vec![],
        };
        let options = question["options"].as_array();
        if selected.iter().any(|selected| {
            !options
                .is_some_and(|options| options.iter().any(|option| option["label"] == *selected))
        }) {
            return Err("Unknown question option".into());
        }
        if question["multiSelect"] != true && selected.len() > 1 {
            return Err("This question allows one choice".into());
        }
        let custom = match answer.get("custom") {
            None | Some(Value::Null) => "",
            Some(value) => value.as_str().ok_or("Custom answer must be text")?,
        };
        if selected.is_empty() && custom.trim().is_empty() {
            return Err("Answer cannot be empty".into());
        }
    }
    Ok(())
}
