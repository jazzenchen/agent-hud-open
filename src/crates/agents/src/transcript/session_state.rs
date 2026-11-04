//! The one state machine every agent's live log goes through: running,
//! waiting, idle or unverified, when that next changes on its own, and the
//! facts and times a session shows.
use super::live::{Call, LiveLog, Work};
use serde_json::{json, Value};

const QUIET_MS: i64 = 120_000;
const ABANDONED_MS: i64 = 1_800_000;

impl LiveLog {
    pub fn session_state(&self, now: i64, source: &str, subagents_at: Option<i64>) -> &'static str {
        let Some(last) = self.last_event().filter(|_| !self.is_internal) else {
            // Claude and Codex record every turn, so a session of theirs with no event is unknown.
            // Sources read from a database (Antigravity) or whose session never prompted (DeepSeek)
            // carry no lifecycle: nothing says work is in flight, so they are idle, not out of date,
            // and their mark must not rest as a faded "no news" logo.
            return if self.is_internal
                || source == "codex"
                || crate::transcript::claude_lines(source)
            {
                "unverified"
            } else {
                "idle"
            };
        };
        if self
            .activity
            .as_ref()
            .and_then(|activity| activity.standing_question.as_ref())
            .is_some_and(|call| now - call.at < ABANDONED_MS)
        {
            return "waiting";
        }
        let turn = self.turns.back();
        let fresh_child = subagents_at.is_some_and(|at| now - at < ABANDONED_MS);
        let in_flight = match turn {
            Some(turn) if !turn.running => fresh_child,
            Some(turn) => {
                now - last
                    < if crate::transcript::claude_lines(source) && turn.started.is_none() {
                        QUIET_MS
                    } else {
                        ABANDONED_MS
                    }
                    || fresh_child
            }
            None => now - last < QUIET_MS || fresh_child,
        };
        if !in_flight {
            return "idle";
        }
        if self.requests(now).next().is_some() {
            "waiting"
        } else {
            "running"
        }
    }

    /// The first time after `now` at which `session_state`, or the requests
    /// this log shows, can change with no new event: a question or request
    /// lapsing, a turn going quiet or abandoned, a sub-agent going stale.
    pub fn next_change(&self, now: i64, source: &str, subagents_at: Option<i64>) -> Option<i64> {
        let calls = self.activity.iter().flat_map(|activity| {
            activity
                .calls
                .iter()
                .filter(|call| call.request.is_some())
                .chain(activity.standing_question.iter())
        });
        let mut times = calls
            .map(|call| call.at + ABANDONED_MS)
            .chain(subagents_at.map(|at| at + ABANDONED_MS))
            .collect::<Vec<_>>();
        if let Some(last) = self.last_event().filter(|_| !self.is_internal) {
            match self.turns.back() {
                Some(turn) if !turn.running => {}
                Some(turn)
                    if !(crate::transcript::claude_lines(source) && turn.started.is_none()) =>
                {
                    times.push(last + ABANDONED_MS)
                }
                _ => times.push(last + QUIET_MS),
            }
        }
        times.into_iter().filter(|at| *at > now).min()
    }

    fn requests(&self, now: i64) -> impl Iterator<Item = &Call> {
        self.activity
            .iter()
            .flat_map(|activity| {
                activity
                    .calls
                    .iter()
                    .chain(activity.standing_question.iter())
            })
            .filter(move |call| call.request.is_some() && now - call.at < ABANDONED_MS)
    }

    pub fn clock_fingerprint(&self, now: i64) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for call in self.requests(now) {
            call.id.hash(&mut hash);
        }
        hash.finish()
    }

    pub fn facts(
        &self,
        id: &str,
        source: &str,
        now: i64,
        subagents_at: Option<i64>,
    ) -> Option<Value> {
        let turn = self.turns.back();
        let observed = self.last_event().into_iter().chain(subagents_at).max()?;
        let child_working = subagents_at.is_some();
        let requests = self.requests(now).map(|call| json!({"id":format!("log:{source}:{id}:{}",call.id),"sessionId":id,"kind":call.request,"at":crate::iso(call.at)})).collect::<Vec<_>>();
        let activity = self.activity.as_ref().map(|activity| {
            let open = activity.calls.iter().rev().find(|call| call.work.is_some_and(Work::shown)).or_else(|| activity.calls.last());
            json!({"work":activity.work,"openCall":open.map(|call| json!({"work":call.work,"at":crate::iso(call.at)})),"toolAt":activity.tool_at.and_then(crate::iso),"delegating":subagents_at.is_some() || activity.calls.iter().any(|call|call.work==Some(Work::Agents)),"awaitsModel":activity.awaits_model(),"retrying":activity.retrying,"compactingSince":null})
        });
        let turn_id = turn.and_then(|turn| {
            turn.id.clone().or_else(|| {
                (!turn.running && turn.outcome.as_deref() == Some("completed"))
                    .then(|| format!("completed-{}", turn.observed))
            })
        });
        Some(
            json!({"turnId":turn_id,"turnStartedAt":turn.and_then(|turn|turn.started).and_then(crate::iso),"outcome":turn.filter(|turn|!turn.running && !child_working).and_then(|turn|turn.outcome.as_ref()),"endedAt":turn.filter(|turn|!turn.running && !child_working).and_then(|turn|crate::iso(turn.observed)),"message":turn.and_then(|turn|turn.message.as_ref()),"limitedUntil":turn.and_then(|turn|turn.limited_until).and_then(crate::iso),"observedAt":crate::iso(observed),"lastEventAt":crate::iso(observed),"activity":activity,"requests":requests}),
        )
    }

    pub fn activity_at(&self, fallback: Option<i64>, child: Option<i64>) -> Option<i64> {
        self.last_event()
            .into_iter()
            .chain(child)
            .max()
            .or(fallback)
    }

    pub fn started_at(&self) -> Option<i64> {
        self.started_at
            .or_else(|| self.turns.front().and_then(|turn| turn.started))
    }

    pub fn last_event(&self) -> Option<i64> {
        self.last_event
            .into_iter()
            .chain(self.turns.back().map(|turn| turn.observed))
            .max()
    }
}

#[cfg(test)]
#[path = "session_state_tests.rs"]
mod tests;
