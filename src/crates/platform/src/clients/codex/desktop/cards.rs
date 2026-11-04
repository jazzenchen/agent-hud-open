//! The cards a followed thread's state asks for, kept in step with each
//! state Codex Desktop streams for it and withdrawn while it is unknown.
use super::turns::turns;
use super::{pending, LivePending, Shared, ThreadState};
use agent_hud_agents::codex::AGENT;
use serde_json::Value;
use std::sync::{atomic::Ordering, PoisonError};

impl Shared {
    pub(super) fn reconcile(&self, thread: &str, owner: &str, state_value: Value) {
        if !self.active.load(Ordering::Acquire) {
            return;
        }
        if let Ok(turns) = turns(&state_value, true) {
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
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
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
}

pub(super) fn unknown(shared: &Shared, thread: &str) {
    let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
    state.known.remove(thread);
    for (id, live) in &mut state.pending {
        if live.item.thread == thread {
            shared.requests.cancel(id);
            live.ticket = None;
        }
    }
    drop(state);
    shared.publish_waiting();
}
