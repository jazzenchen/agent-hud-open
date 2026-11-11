//! Taking hook events in: dropping evidence past its window, and keeping
//! the newest event of each kind a client's hooks report for a session.
use super::{valid, HookEvidence, COMPACTION, DAY, FUTURE};
use agent_hud_agents::{HookEvent, COMPLETION_SOURCES};

fn newer(slot: &mut Option<HookEvent>, event: &HookEvent) -> bool {
    if slot.as_ref().is_some_and(|old| {
        old.at_ms > event.at_ms || (old.at_ms == event.at_ms && old.data == event.data)
    }) {
        return false;
    }
    *slot = Some(event.clone());
    true
}
fn newer_observation(slot: &mut Option<HookEvent>, event: &HookEvent) -> bool {
    let mut merged = event.clone();
    if let Some(old) = slot.as_ref() {
        let same_turn = old.data["turnId"] == event.data["turnId"]
            || event.data["turnId"].is_null()
            || old.data["turnId"].is_null();
        if same_turn {
            for key in ["prompt", "message", "turnId", "startedAtMs"] {
                if merged.data[key].is_null() && !old.data[key].is_null() {
                    merged.data[key] = old.data[key].clone();
                }
            }
        }
    }
    newer(slot, &merged)
}

impl HookEvidence {
    pub(crate) fn ingest(&mut self, events: &[HookEvent], now: i64) -> bool {
        let mut changed = false;
        self.completions
            .retain(|_, at| *at > now.saturating_sub(30 * DAY));
        for entry in self
            .sessions
            .values_mut()
            .flat_map(|sessions| sessions.values_mut())
        {
            for (slot, retention) in [
                (&mut entry.attention, DAY),
                (&mut entry.compaction, COMPACTION),
                (&mut entry.completion, 30 * DAY),
                (&mut entry.observation, 30 * DAY),
            ] {
                if slot
                    .as_ref()
                    .is_some_and(|event| !valid(event, now, retention))
                {
                    *slot = None;
                    changed = true;
                }
            }
        }
        self.sessions.retain(|_, sessions| {
            sessions.retain(|_, s| {
                s.origin.is_some()
                    || s.delivery.is_some()
                    || s.attention.is_some()
                    || s.compaction.is_some()
                    || s.completion.is_some()
                    || s.observation.is_some()
            });
            !sessions.is_empty()
        });
        for event in events {
            if event.source.is_empty()
                || event.session_id.is_empty()
                || !event.data.is_object()
                || event.at_ms > now.saturating_add(FUTURE)
                // Origin is the latest persistent host observation, not a lease.
                // Its explicit no-target record must also survive across restarts,
                // and so must a delivery, which names a session's first prompt.
                || (!matches!(event.kind.as_str(), "origin" | "delivery")
                    && !valid(event, now, 30 * DAY))
            {
                continue;
            }
            // What each client's hooks report, from its descriptor.
            let hooks = agent_hud_agents::hooks::hooks_by_id(&event.source);
            let supported = match event.kind.as_str() {
                "origin" => hooks.is_some_and(|hooks| hooks.origin.is_some()),
                "delivery" => agent_hud_agents::by_id(&event.source).is_some(),
                "session-observation" | "session-forget" => {
                    agent_hud_agents::by_id(&event.source)
                        .is_some_and(|agent| agent.observer.is_some())
                        || hooks.is_some_and(|hooks| {
                            hooks.completion.as_ref().is_some_and(|c| c.begin.is_some())
                        })
                }
                "attention" => hooks.is_some_and(|hooks| hooks.attention),
                "compaction" => hooks.is_some_and(|hooks| hooks.compaction.is_some()),
                "completion" => COMPLETION_SOURCES.contains(&event.source.as_str()),
                _ => false,
            };
            if !supported {
                continue;
            }
            if event.kind == "attention" && !valid(event, now, DAY)
                || event.kind == "compaction" && !valid(event, now, COMPACTION)
            {
                continue;
            }
            if event.kind == "completion" {
                let Some(turn) = event.data["turnId"].as_str().filter(|s| !s.is_empty()) else {
                    continue;
                };
                let key = (event.source.clone(), event.session_id.clone(), turn.into());
                if self.completions.contains_key(&key) {
                    continue;
                }
                self.completions.insert(key, event.at_ms);
            }
            let entry = self
                .sessions
                .entry(event.source.clone())
                .or_default()
                .entry(event.session_id.clone())
                .or_default();
            changed |= match event.kind.as_str() {
                "origin" => newer(&mut entry.origin, event),
                "delivery" => newer(&mut entry.delivery, event),
                "attention" => newer(&mut entry.attention, event),
                "compaction" => newer(&mut entry.compaction, event),
                "completion" => newer(&mut entry.completion, event),
                "session-observation" => newer_observation(&mut entry.observation, event),
                "session-forget"
                    if entry
                        .observation
                        .as_ref()
                        .is_some_and(|old| old.data["turnId"] == event.data["turnId"]) =>
                {
                    entry.observation = None;
                    true
                }
                _ => false,
            };
        }
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }
}
