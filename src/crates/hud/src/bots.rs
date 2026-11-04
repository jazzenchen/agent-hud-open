//! Bot directions: what each agent's Bot shows, decided once for every screen
//! (MarkDirector). Ported from src/packages/hud/src/bots/mark-director.ts.
//!
//! Director times are UTC seconds; a HUD window converts a delivered cue to its
//! own animation clock once.
mod facts;
mod follow;
mod wants;

use facts::{bot_queue, ended, inflight, limited, open_call, started, time, tool_seen};
use follow::{follow, is_working, tell};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use wants::{badge_of, want_of};

/// A request an agent waits on: a permission, a question or a plan.
#[derive(Clone)]
struct Request {
    id: String,
    session_id: String,
    kind: String,
    at: f64,
}

/// A session as the director reads it.
struct Facts {
    id: String,
    phase: String,
    /// The session's liveFacts; null when its client reports none.
    live: Value,
    observed_at: f64,
    last_event_at: f64,
    requests: Vec<Request>,
}

/// A mark's agent as a snapshot has it.
struct QueueMark {
    key: String,
    is_working: bool,
    sessions: Vec<Facts>,
    requests: Vec<Request>,
    exhausted_until: Option<f64>,
}

/// What a mark's facts ask it to show.
struct Want {
    look: String,
    owner: Option<String>,
    subagents: bool,
    holds: bool,
}

/// A working look waiting out the guard.
struct Wanted {
    look: String,
    since: f64,
}

/// An event a Bot plays once: "start", "failed" or "limit-hit".
#[derive(Clone, Copy)]
struct Cue {
    event: &'static str,
    at: f64,
}

/// What a mark shows, and since when.
struct Mark {
    look: Option<String>,
    owner: Option<String>,
    subagents: bool,
    holds: bool,
    since: f64,
    wanted: Option<Wanted>,
    /// When the working look changed in the last 30 s.
    changes: Vec<f64>,
    guarded: bool,
    resting_since: Option<f64>,
    /// The latest turn end told, by session.
    ended: HashMap<String, f64>,
    cue: Option<Cue>,
}

impl Default for Mark {
    fn default() -> Self {
        Self {
            look: None,
            owner: None,
            subagents: false,
            holds: false,
            since: f64::NEG_INFINITY,
            wanted: None,
            changes: vec![],
            guarded: false,
            resting_since: None,
            ended: HashMap::new(),
            cue: None,
        }
    }
}

/// Decides every mark's Bot from snapshots and time, for all HUD windows.
pub struct Director {
    started_at: f64,
    directions: Map<String, Value>,
    queue: Vec<QueueMark>,
    marks: HashMap<String, Mark>,
    looked_at: HashMap<String, f64>,
}

impl Director {
    /// A director that tells only of turns that end or begin after `started_at`.
    pub fn new(started_at: f64) -> Self {
        Self {
            started_at,
            directions: Map::new(),
            queue: vec![],
            marks: HashMap::new(),
            looked_at: HashMap::new(),
        }
    }

    /// Takes a new snapshot's agents and directs them at `now`.
    pub fn read(&mut self, snapshot: &Value, now: f64) {
        self.queue = bot_queue(snapshot, now);
        self.advance(now);
    }

    /// The user looked at these agents, or at every agent shown: a failure
    /// that ended before now wears no badge.
    pub fn looked(&mut self, keys: Option<&[String]>, now: f64) {
        match keys {
            Some(keys) => {
                for key in keys {
                    self.looked_at.insert(key.clone(), now);
                }
            }
            None => {
                for queue in &self.queue {
                    self.looked_at.insert(queue.key.clone(), now);
                }
            }
        }
        self.advance(now);
    }

    /// Brings every mark's direction up to `now`.
    pub fn advance(&mut self, now: f64) {
        let mut marks = std::mem::take(&mut self.marks);
        let mut directions = Map::new();
        for queue in &self.queue {
            let mut mark = marks.remove(&queue.key).unwrap_or_default();
            let want = want_of(queue, mark.owner.as_deref(), now);
            let resting = mark.resting_since;
            let resting_from = queue
                .sessions
                .iter()
                .map(|session| session.last_event_at)
                .fold(f64::NEG_INFINITY, f64::max);
            let rested = follow(&mut mark, &want, now, resting_from);
            for session in &queue.sessions {
                let outcome = session.live["outcome"].as_str().unwrap_or("");
                let Some(end) = ended(session) else { continue };
                let told = mark.ended.get(&session.id).copied();
                if !outcome.is_empty()
                    && end > self.started_at.max(told.unwrap_or(f64::NEG_INFINITY))
                {
                    mark.ended.insert(session.id.clone(), end);
                    match outcome {
                        "failed" => tell(&mut mark, "failed", now),
                        "limited" => tell(&mut mark, "limit-hit", now),
                        _ => {}
                    }
                }
            }
            mark.ended
                .retain(|id, _| queue.sessions.iter().any(|session| session.id == *id));
            let owned = queue
                .sessions
                .iter()
                .find(|session| mark.owner.as_ref() == Some(&session.id));
            let since = self.started_at.max(resting.unwrap_or(f64::NEG_INFINITY));
            if rested
                && queue.requests.is_empty()
                && owned.is_some_and(|owned| started(owned).unwrap_or(f64::NEG_INFINITY) > since)
            {
                tell(&mut mark, "start", now);
            }
            let look = mark.look.as_deref().unwrap_or("idle");
            let looked_at = self.looked_at.get(&queue.key).copied();
            directions.insert(
                queue.key.clone(),
                json!({
                    "state": {
                        "look": look,
                        "subagents": mark.subagents && is_working(look),
                        "badge": badge_of(queue, looked_at.unwrap_or(f64::NEG_INFINITY), now),
                        "hovered": false,
                        "holds": mark.holds && look == "thinking",
                        "card": "done",
                        "facing": null,
                    },
                    "cue": mark.cue.map(|cue| json!({"event": cue.event, "at": cue.at})),
                }),
            );
            self.marks.insert(queue.key.clone(), mark);
        }
        self.directions = directions;
    }

    /// When the directions next change with no new snapshot, if they will.
    pub fn next_change(&self, now: f64) -> Option<f64> {
        let mut times: Vec<f64> = vec![];
        let mut take = |value: f64| {
            if value > now && value.is_finite() {
                times.push(value);
            }
        };
        for queue in &self.queue {
            if let Some(mark) = self.marks.get(&queue.key) {
                if let Some(wanted) = &mark.wanted {
                    let guard = if mark.guarded { 12.0 } else { 6.0 };
                    take((wanted.since + 2.0).max(mark.since + guard));
                }
            }
            for session in &queue.sessions {
                take(session.last_event_at + 3600.0);
                let call = open_call(session);
                if let Some(call) = call.filter(|call| call["work"] == "shell") {
                    take(time(&call["at"]).unwrap_or(f64::INFINITY) + 20.0);
                }
                let compacting = time(&session.live["activity"]["compactingSince"]);
                take(compacting.unwrap_or(f64::INFINITY) + 300.0);
                if limited(session, now) {
                    take(time(&session.live["limitedUntil"]).unwrap_or(f64::INFINITY));
                }
                if inflight(session) {
                    if call.is_none() {
                        take(tool_seen(session).unwrap_or(f64::INFINITY) + 10.0);
                    }
                    take(session.observed_at + 180.0);
                }
                if session.live["outcome"] == "failed" {
                    take(ended(session).unwrap_or(f64::INFINITY) + 900.0);
                }
            }
            take(queue.exhausted_until.unwrap_or(f64::INFINITY));
        }
        times.into_iter().reduce(f64::min)
    }

    /// What the HUD windows draw: `{startedAt, directions: {mark: {state, cue}}}`.
    pub fn wire(&self) -> Value {
        json!({"startedAt": self.started_at, "directions": self.directions})
    }
}

#[cfg(test)]
mod tests;
