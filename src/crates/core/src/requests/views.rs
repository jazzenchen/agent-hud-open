//! What waits on the user, as the HUD and the hosts read it: the cards, the
//! sessions they belong to, and when they next change.
use super::{Content, Requests};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    sync::PoisonError,
};

impl Requests {
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
}
