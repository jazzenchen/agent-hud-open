//! Latest client-reported evidence beside the transcript. This is not another
//! transcript reducer or usage ledger: origins never imply work, attention never
//! creates an actionable approval, and callbacks never mint token/cost facts.
use crate::naming::surfaces;
use agent_hud_agents::{HookEvent, COMPLETION_SOURCES};
use chrono::DateTime;
use serde_json::Value;
use std::collections::HashMap;

mod overlay;

const DAY: i64 = 86_400_000;
const COMPACTION: i64 = 300_000;
const FUTURE: i64 = 60_000;
const PI_HEARTBEAT: i64 = 120_000;
/// How long a notification holds a hook-reported turn waiting for the user:
/// the span after which a quiet session counts as abandoned.
const ASKED: i64 = 1_800_000;

type Key = (String, String);
#[derive(Default)]
pub(crate) struct HookEvidence {
    revision: u64,
    sessions: HashMap<String, HashMap<String, Evidence>>,
    completions: HashMap<(String, String, String), i64>,
}
#[derive(Default)]
struct Evidence {
    origin: Option<HookEvent>,
    /// A prompt Agent HUD itself sent the session (`naming::surfaces`).
    delivery: Option<HookEvent>,
    attention: Option<HookEvent>,
    compaction: Option<HookEvent>,
    completion: Option<HookEvent>,
    observation: Option<HookEvent>,
}
fn date(value: &Value) -> Option<i64> {
    value
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp_millis())
}
fn valid(event: &HookEvent, now: i64, retention: i64) -> bool {
    event.at_ms > now.saturating_sub(retention) && event.at_ms <= now.saturating_add(FUTURE)
}
fn observation_state(event: &HookEvent, now: i64) -> Option<&'static str> {
    // Only Pi reports in-flight observer snapshots; its extension renews them
    // every 15 seconds. OpenCode observations already carry terminal states.
    match event.data["state"].as_str()? {
        "running" | "waiting" if now.saturating_sub(event.at_ms) >= PI_HEARTBEAT => Some("idle"),
        "running" => Some("running"),
        "waiting" => Some("waiting"),
        "completed" | "ended" => Some("idle"),
        _ => None,
    }
}
/// The notification that holds a turn its hooks report waiting: one newer
/// than the running observation, within `ASKED` of now. A turn that asked
/// for the user reports nothing until they answer, so its heartbeat does
/// not lapse meanwhile (TRAE's permission prompt).
fn asked(evidence: &Evidence, now: i64) -> Option<&HookEvent> {
    let observed = evidence.observation.as_ref()?;
    evidence.attention.as_ref().filter(|attention| {
        observed.data["state"] == "running"
            && attention.at_ms > observed.at_ms
            && valid(attention, now, ASKED)
    })
}
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
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
    /// What a session's hooks and Agent HUD's deliveries say about where its
    /// prompts were typed: the latest of each, and the first each recorded.
    pub(crate) fn reports(&self, source: &str, session: &str) -> surfaces::Reports {
        let Some(entry) = self
            .sessions
            .get(source)
            .and_then(|sessions| sessions.get(session))
        else {
            return surfaces::Reports::default();
        };
        let report = |event: &Option<HookEvent>, first: bool| {
            event
                .as_ref()
                .and_then(|e| surfaces::Report::from_record(&e.data, e.at_ms, first))
        };
        surfaces::Reports {
            hook: report(&entry.origin, false),
            first_hook: report(&entry.origin, true),
            delivery: report(&entry.delivery, false),
            first_delivery: report(&entry.delivery, true),
        }
    }
    pub(crate) fn has_evidence(&self, source: &str, session: &str) -> bool {
        self.sessions
            .get(source)
            .is_some_and(|sessions| sessions.contains_key(session))
    }
    /// Only small evidence flags are observed here. Source heads own their own
    /// clock/phase fingerprint; no transcript or full session is serialized.
    /// The first time after `now` at which what `clock_fingerprint` reads can
    /// change with no new event: an observation's heartbeat lapsing, or an
    /// attention, compaction or completion leaving its window.
    pub(crate) fn next_change(&self, now: i64) -> Option<i64> {
        let mut times = Vec::new();
        for evidence in self.sessions.values().flat_map(HashMap::values) {
            if let Some(event) = &evidence.observation {
                if matches!(event.data["state"].as_str(), Some("running" | "waiting")) {
                    times.push(event.at_ms.saturating_add(PI_HEARTBEAT));
                }
            }
            if let Some(attention) = asked(evidence, now) {
                times.push(attention.at_ms.saturating_add(ASKED));
            }
            for (event, retention) in [
                (&evidence.attention, DAY),
                (&evidence.compaction, COMPACTION),
                (&evidence.completion, 30 * DAY),
            ] {
                if let Some(event) = event {
                    // Valid from FUTURE before its time, and until its window ends.
                    times.push(event.at_ms.saturating_sub(FUTURE));
                    times.push(event.at_ms.saturating_add(retention));
                }
            }
        }
        times.into_iter().filter(|at| *at > now).min()
    }

    pub(crate) fn clock_fingerprint(&self, now: i64) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut result = self.revision.wrapping_mul(0x9e3779b97f4a7c15);
        for (source, sessions) in &self.sessions {
            for (id, evidence) in sessions {
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                source.hash(&mut hash);
                id.hash(&mut hash);
                evidence.origin.is_some().hash(&mut hash);
                evidence
                    .observation
                    .as_ref()
                    .map(|e| (&e.data, e.at_ms, observation_state(e, now)))
                    .hash(&mut hash);
                asked(evidence, now).is_some().hash(&mut hash);
                evidence
                    .attention
                    .as_ref()
                    .is_some_and(|e| valid(e, now, DAY))
                    .hash(&mut hash);
                evidence
                    .compaction
                    .as_ref()
                    .is_some_and(|e| valid(e, now, COMPACTION) && e.data["trigger"] == "auto")
                    .hash(&mut hash);
                evidence
                    .completion
                    .as_ref()
                    .is_some_and(|e| valid(e, now, 30 * DAY))
                    .hash(&mut hash);
                result = result.wrapping_add(hash.finish());
            }
        }
        result
    }
}

#[cfg(test)]
mod tests;
