//! Receipts of native answers as their clients confirm them, and the
//! reviewer each recent turn named.
use super::{Requests, Reviewer};
use serde_json::Value;
use std::sync::PoisonError;

impl Requests {
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
