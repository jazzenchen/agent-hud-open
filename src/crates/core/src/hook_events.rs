//! Latest client-reported evidence beside the transcript. This is not another
//! transcript reducer or usage ledger: origins never imply work, attention never
//! creates an actionable approval, and callbacks never mint token/cost facts.
use crate::naming::surfaces;
use agent_hud_agents::HookEvent;
use chrono::DateTime;
use serde_json::Value;
use std::collections::HashMap;

mod hook_only_sessions;
mod ingest;
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
impl HookEvidence {
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
