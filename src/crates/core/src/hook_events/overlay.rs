//! Laying hook evidence over the sessions the logs show: their state, their
//! activity time, and sessions only hooks have seen.
use super::{asked, date, observation_state, valid, HookEvidence, Key, COMPACTION, DAY};
use crate::iso;
use crate::naming::session_title::{self, TitleEvidence};
use crate::naming::surfaces;
use agent_hud_agents::HookEvent;
use serde_json::{json, Value};
use std::collections::HashSet;

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
        let log_event = date(&session["liveFacts"]["lastEventAt"]);
        let last_event = log_event.or_else(|| date(&session["updatedAt"]));
        if let Some(observed) = entry.observation.as_ref().filter(|e| {
            valid(e, now, 30 * DAY)
                && (last_event.is_none_or(|at| e.at_ms >= at)
                    || (log_event.is_none()
                        && entry
                            .completion
                            .as_ref()
                            .is_some_and(|c| valid(c, now, 30 * DAY) && c.at_ms >= e.at_ms)))
        }) {
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
                if !observed.data["prompt"].is_null() {
                    facts["prompt"] = observed.data["prompt"].clone();
                }
                if !observed.data["message"].is_null() {
                    facts["message"] = observed.data["message"].clone();
                }
                // Which of Agent HUD's messages a stop hook handed over.
                if !observed.data["delivered"].is_null() {
                    facts["delivered"] = observed.data["delivered"].clone();
                }
                // An observation without a destination keeps the one the log gave.
                if !observed.data["navigationTarget"].is_null() {
                    session["navigationTarget"] = observed.data["navigationTarget"].clone();
                }
            }
        }

        if let Some(completion) = entry.completion.as_ref().filter(|e| {
            valid(e, now, 30 * DAY)
                && (last_event.is_none_or(|at| e.at_ms > at)
                    || (log_event.is_none() && last_event.is_none_or(|at| e.at_ms + 60_000 >= at))
                    || session["evidence"] == "completion-hook")
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
            if !completion.data["message"].is_null() {
                facts["message"] = completion.data["message"].clone();
            }
        }
        // When a hook of this session last reported: proof that its client
        // runs Agent HUD's hooks, which is what a queued message waits for.
        let hook_at = [&entry.observation, &entry.completion, &entry.attention]
            .into_iter()
            .flatten()
            .filter(|e| valid(e, now, 30 * DAY))
            .map(|e| e.at_ms)
            .max();
        // The processes its newest hook ran from (`platform::hooks::entry`).
        let hook_pids = [&entry.observation, &entry.completion]
            .into_iter()
            .flatten()
            .filter(|e| valid(e, now, DAY) && e.data["pids"].is_array())
            .max_by_key(|e| e.at_ms)
            .map(|e| e.data["pids"].clone());
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
        // What agent messages need of the hooks, kept beside the live facts
        // because a client's live status can be turned off for display
        // (`hud::presentation`), which drops those and shows it idle: when a
        // hook last reported, the processes it ran from, and the state the
        // hooks left the session in.
        if let Some(at) = hook_at {
            session["hooks"] = json!({"at":iso(at),"pids":hook_pids.unwrap_or(Value::Null),"state":session["state"]});
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
