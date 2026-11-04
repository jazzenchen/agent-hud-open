//! Latest client-reported evidence beside the transcript. This is not another
//! transcript reducer or usage ledger: origins never imply work, attention never
//! creates an actionable approval, and callbacks never mint token/cost facts.
use crate::naming::session_title::{self, TitleEvidence};
use crate::naming::surfaces;
use agent_hud_agents::{HookEvent, COMPLETION_SOURCES};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
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
fn iso(at: i64) -> Value {
    DateTime::<Utc>::from_timestamp_millis(at)
        .map(|d| json!(d.to_rfc3339()))
        .unwrap_or(Value::Null)
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
fn facts(session: &mut Value) -> &mut Value {
    if !session["liveFacts"].is_object() {
        let observed = session["updatedAt"].clone();
        session["liveFacts"] = json!({"turnId":null,"turnStartedAt":null,"outcome":null,"endedAt":null,"limitedUntil":null,"observedAt":observed,"lastEventAt":observed,"activity":null,"requests":[]});
    }
    &mut session["liveFacts"]
}
fn advance(session: &mut Value, at: i64) {
    let at = at
        .max(date(&session["updatedAt"]).unwrap_or(at))
        .max(date(&session["liveFacts"]["lastEventAt"]).unwrap_or(at));
    let timestamp = iso(at);
    if timestamp.is_null() {
        return;
    }
    session["updatedAt"] = timestamp.clone();
    let facts = facts(session);
    facts["observedAt"] = iso(at.max(date(&facts["observedAt"]).unwrap_or(at)));
    facts["lastEventAt"] = timestamp;
}
/// The Claude desktop surface an `entrypoint` names, as hook origins name hosts.
fn claude_desktop_host(entrypoint: &str) -> Option<&'static str> {
    match entrypoint {
        "claude-desktop" | "claude-desktop-3p" => Some("desktop"),
        "local-agent" => Some("cowork"),
        _ => None,
    }
}
/// Open's rule (ClaudeCodeProvider's navigation target): a Claude log written
/// after the hook origin, by another surface, wins over that origin. Hooks can
/// be off while a session resumes elsewhere. The desktop app has its own exact
/// tab identity, so a newer desktop log keeps the desktop index's target; a
/// terminal pane cannot be guessed from a log, so any other surface has none.
/// None when the origin stands.
fn claude_moved_on(
    source: &str,
    origin: &HookEvent,
    client: Option<&str>,
    last_log_line_ms: Option<i64>,
    current: &Value,
) -> Option<Value> {
    let entrypoint = client.filter(|e| !e.is_empty())?;
    if source != "claude" || last_log_line_ms? <= origin.at_ms {
        return None;
    }
    let host = origin.data["host"].as_str().unwrap_or("");
    let same = match (entrypoint, claude_desktop_host(entrypoint)) {
        ("cli", _) => host == "cli",
        (_, Some(desktop)) => host == desktop,
        _ => host == "cli" && origin.data["navigationTarget"].is_null(),
    };
    if same {
        return None;
    }
    let kind = match claude_desktop_host(entrypoint) {
        Some("desktop") => "claude-desktop",
        Some(_) => "claude-cowork",
        None => return Some(Value::Null),
    };
    Some(if current["kind"] == kind {
        current.clone()
    } else {
        Value::Null
    })
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
                "session-observation" => newer(&mut entry.observation, event),
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
    pub(crate) fn presentation_activity_at(
        &self,
        source: &str,
        session: &str,
        base_at: Option<i64>,
        in_flight: bool,
        last_line: Option<i64>,
        now: i64,
    ) -> Option<i64> {
        let base = self.activity_at(source, session, base_at, now);
        if source != "claude" || !in_flight {
            return base;
        }
        let attention = self
            .sessions
            .get(source)
            .and_then(|sessions| sessions.get(session))
            .and_then(|e| e.attention.as_ref())
            .filter(|e| {
                valid(e, now, DAY) && last_line.or(base_at).is_none_or(|line| e.at_ms > line)
            })
            .map(|e| e.at_ms);
        base.into_iter().chain(attention).max()
    }
    /// Stateless evidence cannot decide whether attention belongs to an in-flight
    /// turn. Only a completion may advance this clock without seeing session phase;
    /// overlay applies eligible attention once the source's phase is available.
    pub(crate) fn activity_at(
        &self,
        source: &str,
        session: &str,
        base_at: Option<i64>,
        now: i64,
    ) -> Option<i64> {
        let completion = self
            .sessions
            .get(source)
            .and_then(|sessions| sessions.get(session))
            .and_then(|e| e.completion.as_ref())
            .filter(|e| valid(e, now, 30 * DAY))
            .map(|e| e.at_ms);
        let observation = self
            .sessions
            .get(source)
            .and_then(|sessions| sessions.get(session))
            .and_then(|e| e.observation.as_ref())
            .filter(|e| valid(e, now, 30 * DAY))
            .map(|e| e.at_ms);
        base_at
            .into_iter()
            .chain(completion)
            .chain(observation)
            .max()
    }
    /// Apply to a fresh source-derived presentation value, never to the parser
    /// head or an already-overlaid cached view. Expiration restores source facts.
    /// `client` is the client the session's log last named, as Claude Code's
    /// `entrypoint`; none for a session with no log.
    pub(crate) fn overlay(
        &self,
        session: &mut Value,
        now: i64,
        last_log_line_ms: Option<i64>,
        client: Option<&str>,
    ) {
        let (Some(source), Some(id)) = (session["source"].as_str(), session["id"].as_str()) else {
            return;
        };
        let Some(entry) = self
            .sessions
            .get(source)
            .and_then(|sessions| sessions.get(id))
        else {
            return;
        };
        if let Some(origin) = entry
            .origin
            .as_ref()
            .filter(|e| e.data["hostVerified"] == true)
        {
            let host = origin.data["host"].as_str();
            if matches!(host, Some("cli" | "desktop" | "cowork" | "daemon")) {
                // A verified origin without a destination intentionally clears an
                // older one, unless a newer log shows the session went elsewhere.
                let moved = claude_moved_on(
                    source,
                    origin,
                    client,
                    last_log_line_ms,
                    &session["navigationTarget"],
                );
                session["origin"] = json!({"host":host,"at":iso(origin.at_ms)});
                session["navigationTarget"] =
                    moved.unwrap_or_else(|| origin.data["navigationTarget"].clone());
            }
        }
        let last_event =
            date(&session["liveFacts"]["lastEventAt"]).or_else(|| date(&session["updatedAt"]));
        if let Some(observed) = entry
            .observation
            .as_ref()
            .filter(|e| valid(e, now, 30 * DAY) && last_event.is_none_or(|at| e.at_ms >= at))
        {
            let held = asked(entry, now).map(|_| "waiting");
            if let Some(state) = held.or_else(|| observation_state(observed, now)) {
                session["state"] = json!(state);
                advance(session, observed.at_ms);
                let facts = facts(session);
                facts["turnId"] = observed.data["turnId"].clone();
                facts["turnStartedAt"] = observed.data["startedAtMs"]
                    .as_i64()
                    .map(iso)
                    .unwrap_or(Value::Null);
                facts["outcome"] = observed.data["outcome"].clone();
                facts["endedAt"] = if state == "idle" {
                    iso(observed.at_ms)
                } else {
                    Value::Null
                };
                // An observation without a destination keeps the one the log gave.
                if !observed.data["navigationTarget"].is_null() {
                    session["navigationTarget"] = observed.data["navigationTarget"].clone();
                }
            }
        }

        if let Some(completion) = entry.completion.as_ref().filter(|e| {
            valid(e, now, 30 * DAY)
                && last_event.is_none_or(|at| e.at_ms > at)
                && date(&session["liveFacts"]["turnStartedAt"]).is_none_or(|at| e.at_ms >= at)
        }) {
            session["state"] = json!("idle");
            advance(session, completion.at_ms);
            let facts = facts(session);
            facts["turnId"] = completion.data["turnId"].clone();
            facts["outcome"] = json!("completed");
            facts["endedAt"] = iso(completion.at_ms);
            facts["activity"] = Value::Null;
            facts["requests"] = json!([]);
            if facts["message"].is_null() && !completion.data["message"].is_null() {
                facts["message"] = completion.data["message"].clone();
            }
        }
        let own_line = last_log_line_ms.or(last_event);
        if let Some(attention) = entry
            .attention
            .as_ref()
            .filter(|e| valid(e, now, DAY) && own_line.is_none_or(|at| e.at_ms > at))
        {
            // SessionPhase.transcriptTurn only changes a turn still in flight. A Stop
            // followed by a notification cannot reopen the completed turn.
            if matches!(session["state"].as_str(), Some("running" | "waiting")) {
                session["state"] = json!("waiting");
                advance(session, attention.at_ms);
                facts(session)["attention"] =
                    json!({"at":iso(attention.at_ms),"message":attention.data["message"]});
            }
        }
        if let Some(compaction) = entry.compaction.as_ref().filter(|e| {
            valid(e, now, COMPACTION)
                && e.data["trigger"] == "auto"
                && own_line.is_none_or(|at| e.at_ms > at)
        }) {
            let facts = facts(session);
            if !facts["activity"].is_object() {
                facts["activity"] = json!({"work":null,"openCall":null,"toolAt":null,"delegating":false,"awaitsModel":false,"retrying":false,"compactingSince":null});
            }
            facts["activity"]["compactingSince"] = iso(compaction.at_ms);
        }
    }
    /// Explicitly completed clients with no migrated transcript reader can still
    /// present their completion. tokens=0 is a required wire sentinel, labelled
    /// unavailable below; it is never entered into provider totals or the ledger.
    pub(crate) fn extra_sessions(
        &self,
        now: i64,
        days: i64,
        existing: &HashSet<Key>,
    ) -> Vec<Value> {
        let mut result = vec![];
        for (source, sessions) in &self.sessions {
            for (id, evidence) in sessions {
                if existing.contains(&(source.clone(), id.clone())) {
                    continue;
                }
                let Some(event) = evidence
                    .observation
                    .as_ref()
                    .or(evidence.completion.as_ref())
                    .filter(|e| valid(e, now, days.clamp(0, 30) * DAY))
                else {
                    continue;
                };
                let cwd = event.data["workspace"].as_str();
                let mut naming = TitleEvidence::default();
                naming.name(0, event.data["title"].as_str());
                let label =
                    session_title::display(&event.source, &event.session_id, &naming, cwd, None);
                let at = iso(event.at_ms);
                let mut session = json!({"id":event.session_id,"source":event.source,"title":label,"model":event.data["model"].as_str().unwrap_or("Unknown"),"cwd":cwd,"updatedAt":at,"lastMessageAt":at,"state":"idle","tokens":0,"cost":null,"contextPercent":null,"logPath":null,"parentId":null,"usageStatus":"unavailable","evidence":"completion-hook","liveFacts":{"turnId":event.data["turnId"],"turnStartedAt":null,"outcome":"completed","endedAt":at,"limitedUntil":null,"observedAt":at,"lastEventAt":at,"activity":null,"requests":[]}});
                self.overlay(&mut session, now, Some(event.at_ms), None);
                // No log names its prompts: the reports do, else the client's
                // one interface.
                let mut origins =
                    surfaces::from_reports(&self.reports(&event.source, &event.session_id));
                let only = surfaces::from_log(&event.source, None, None);
                origins.latest = origins.latest.or_else(|| origins.init.clone());
                origins.init.get_or_insert_with(|| only.clone());
                origins.latest.get_or_insert(only);
                session["origins"] = origins.json();
                result.push(session);
            }
        }
        result.sort_unstable_by(|a, b| {
            b["updatedAt"]
                .as_str()
                .cmp(&a["updatedAt"].as_str())
                .then_with(|| a["source"].as_str().cmp(&b["source"].as_str()))
                .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
        });
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(state: &str, at: i64) -> HookEvent {
        HookEvent {
            source: "pi".into(),
            session_id: "pi:session".into(),
            kind: "session-observation".into(),
            at_ms: at,
            data: json!({"state":state,"turnId":"turn","startedAtMs":at-1000,"outcome":null}),
        }
    }

    #[test]
    fn a_cursor_prompt_makes_its_session_run_until_the_stop_hook() {
        let at = 1_792_470_000_000i64;
        let event = |kind: &str, at: i64, data: Value| HookEvent {
            source: "cursor".into(),
            session_id: "cursor:c1".into(),
            kind: kind.into(),
            at_ms: at,
            data,
        };
        let mut evidence = HookEvidence::default();
        assert!(evidence.ingest(
            &[event(
                "session-observation",
                at,
                json!({"state":"running","turnId":"g1","startedAtMs":at,"outcome":null}),
            )],
            at,
        ));
        let state = |evidence: &HookEvidence, now: i64| {
            let mut session = json!({"source":"cursor","id":"cursor:c1","state":"idle","updatedAt":iso(at - 60_000)});
            evidence.overlay(&mut session, now, None, None);
            session["state"].as_str().unwrap().to_owned()
        };
        assert_eq!(state(&evidence, at + 1_000), "running");
        // A prompt without a folder keeps the destination the log gave.
        let folder = json!({"kind":"cursor-agent","id":"c1","cwd":"/p"});
        let mut session = json!({"source":"cursor","id":"cursor:c1","state":"idle","updatedAt":iso(at - 60_000),"navigationTarget":folder});
        evidence.overlay(&mut session, at + 1_000, None, None);
        assert_eq!(session["navigationTarget"], folder);
        assert!(evidence.ingest(
            &[event("completion", at + 5_000, json!({"turnId":"g1"}))],
            at + 5_000,
        ));
        assert_eq!(state(&evidence, at + 6_000), "idle");
    }

    #[test]
    fn a_turn_that_asked_for_the_user_waits_past_its_heartbeat() {
        let at = 1_792_470_000_000i64;
        let event = |kind: &str, at: i64, data: Value| HookEvent {
            source: "trae".into(),
            session_id: "trae:c1".into(),
            kind: kind.into(),
            at_ms: at,
            data,
        };
        let running = json!({"state":"running","turnId":"c1","startedAtMs":at,"outcome":null,"title":"Fix the login"});
        let mut evidence = HookEvidence::default();
        evidence.ingest(&[event("session-observation", at, running.clone())], at);
        let asked = at + 10_000;
        evidence.ingest(
            &[event(
                "attention",
                asked,
                json!({"message":"RunCommand needs your confirmation"}),
            )],
            asked,
        );
        let row = |evidence: &HookEvidence, now: i64| {
            evidence.extra_sessions(now, 1, &HashSet::new())[0].clone()
        };
        let waiting = row(&evidence, at + PI_HEARTBEAT + 60_000);
        assert_eq!(waiting["state"], "waiting");
        assert_eq!(waiting["title"], "Fix the login");
        assert_eq!(
            waiting["liveFacts"]["attention"]["message"],
            "RunCommand needs your confirmation"
        );
        // The answer runs the tool: the turn goes on.
        let answered = at + PI_HEARTBEAT + 90_000;
        evidence.ingest(&[event("session-observation", answered, running)], answered);
        assert_eq!(row(&evidence, answered + 1_000)["state"], "running");
        // Unanswered, it is quiet once the wait is over, and that is a change due.
        let mut evidence = HookEvidence::default();
        evidence.ingest(
            &[
                event(
                    "session-observation",
                    at,
                    json!({"state":"running","turnId":"c1","startedAtMs":at,"outcome":null}),
                ),
                event("attention", asked, json!({"message":"Allow?"})),
            ],
            asked,
        );
        assert_eq!(
            evidence.next_change(at + PI_HEARTBEAT + 1),
            Some(asked + ASKED)
        );
        assert_eq!(row(&evidence, asked + ASKED)["state"], "idle");
    }

    #[test]
    fn a_running_observation_falls_due_when_its_heartbeat_lapses() {
        let at = 1_792_470_000_000i64;
        let mut evidence = HookEvidence::default();
        evidence.ingest(&[observation("running", at)], at);
        let due = evidence.next_change(at + 1).unwrap();
        assert_eq!(due, at + PI_HEARTBEAT);
        assert_eq!(
            evidence.clock_fingerprint(due - 1),
            evidence.clock_fingerprint(at + 1)
        );
        assert_ne!(
            evidence.clock_fingerprint(due),
            evidence.clock_fingerprint(at + 1)
        );
    }

    #[test]
    fn pi_heartbeat_expires_at_two_minutes_and_invalidates_session_revision() {
        let at = 1_792_470_000_000i64;
        let directory = tempfile::tempdir().unwrap();
        for state in ["running", "waiting"] {
            let mut core = crate::AgentHudCore::new(vec![], directory.path().join(state));
            core.hook_events.ingest(&[observation(state, at)], at);
            let before = core
                .hook_events
                .extra_sessions(at + PI_HEARTBEAT - 1, 1, &HashSet::new());
            assert_eq!(before[0]["state"], state);
            assert!(before[0]["liveFacts"]["endedAt"].is_null());
            core.refresh_sessions_revision(at + PI_HEARTBEAT - 1);
            let revision = core.sessions_revision;

            let after = core
                .hook_events
                .extra_sessions(at + PI_HEARTBEAT, 1, &HashSet::new());
            assert_eq!(after[0]["state"], "idle");
            assert_eq!(after[0]["liveFacts"]["endedAt"], iso(at));
            assert!(after[0]["liveFacts"]["outcome"].is_null());
            core.refresh_sessions_revision(at + PI_HEARTBEAT);
            assert_ne!(core.sessions_revision, revision);

            let renewed = at + PI_HEARTBEAT;
            core.hook_events
                .ingest(&[observation(state, renewed)], renewed);
            let rows = core.hook_events.extra_sessions(renewed, 1, &HashSet::new());
            assert_eq!(rows[0]["state"], state);
            assert!(rows[0]["liveFacts"]["endedAt"].is_null());
        }
    }

    #[test]
    fn completed_observation_keeps_its_recorded_outcome_after_heartbeat_age() {
        let at = 1_792_470_000_000i64;
        let mut event = observation("completed", at);
        event.data["outcome"] = json!("completed");
        let mut evidence = HookEvidence::default();
        evidence.ingest(&[event], at);
        let rows = evidence.extra_sessions(at + PI_HEARTBEAT, 1, &HashSet::new());
        assert_eq!(rows[0]["state"], "idle");
        assert_eq!(rows[0]["liveFacts"]["outcome"], "completed");
        assert_eq!(rows[0]["liveFacts"]["endedAt"], iso(at));
    }
}
