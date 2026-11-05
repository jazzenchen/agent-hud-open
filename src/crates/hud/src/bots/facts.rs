//! What the director reads of a session and of a mark's agent in a snapshot.
use super::*;
use crate::{
    presentation::{mark, QUEUE},
    vendors::vendors,
};

/// UTC seconds of an RFC 3339 date, to the millisecond as Date.parse reads it;
/// none when absent or unreadable.
pub(super) fn time(date: &Value) -> Option<f64> {
    let date = chrono::DateTime::parse_from_rfc3339(date.as_str()?).ok()?;
    Some(date.timestamp_millis() as f64 / 1000.0)
}

/// `value ?? fallback`.
pub(super) fn coalesce<'a>(value: &'a Value, fallback: &'a Value) -> &'a Value {
    if value.is_null() {
        fallback
    } else {
        value
    }
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

pub(super) fn inflight(session: &Facts) -> bool {
    matches!(session.phase.as_str(), "running" | "waiting")
}

pub(super) fn started(session: &Facts) -> Option<f64> {
    time(&session.live["turnStartedAt"])
}

pub(super) fn ended(session: &Facts) -> Option<f64> {
    time(&session.live["endedAt"])
}

pub(super) fn tool_seen(session: &Facts) -> Option<f64> {
    time(&session.live["activity"]["toolAt"])
}

pub(super) fn subagents(session: &Facts) -> bool {
    session.live["activity"]["delegating"] == true
}

pub(super) fn open_call(session: &Facts) -> Option<&Value> {
    Some(&session.live["activity"]["openCall"]).filter(|call| !call.is_null())
}

pub(super) fn compacting(session: &Facts, now: f64) -> bool {
    time(&session.live["activity"]["compactingSince"]).is_some_and(|at| now - at < 300.0)
}

pub(super) fn limited(session: &Facts, now: f64) -> bool {
    session.live["outcome"] == "limited"
        && time(&session.live["limitedUntil"]).unwrap_or(f64::INFINITY) > now
}

/// The model is retrying, or the session awaits it with no call, sub-agent or
/// request and nothing observed for 3 minutes.
pub(super) fn holds(session: &Facts, now: f64) -> bool {
    let activity = &session.live["activity"];
    activity["retrying"] == true
        || activity["awaitsModel"] == true
            && open_call(session).is_none()
            && !subagents(session)
            && session.requests.is_empty()
            && now - session.observed_at >= 180.0
}

/// Each mark's agent in a snapshot, in the queue's order (windows' agents, then
/// agents with a live session heard from in the last day, newest first): its sessions
/// heard from in the last day, its requests, and when its used-up windows reset.
pub(super) fn bot_queue(snapshot: &Value, now: f64) -> Vec<QueueMark> {
    vendors(snapshot, QUEUE, (now * 1000.0) as i64)
        .into_iter()
        .map(|vendor| {
            let requests: Vec<Request> = snapshot["approvals"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|item| mark(item["source"].as_str().unwrap_or("")) == vendor.key)
                .map(|item| Request {
                    id: text(&item["id"]),
                    session_id: text(coalesce(&item["sessionId"], &item["id"])),
                    kind: item["kind"].as_str().unwrap_or("permission").to_owned(),
                    at: time(&item["createdAt"]).unwrap_or(0.0),
                })
                .collect();
            let sessions: Vec<Facts> = vendor
                .sessions
                .iter()
                .filter(|session| {
                    let last =
                        coalesce(&session["liveFacts"]["lastEventAt"], &session["updatedAt"]);
                    time(last).unwrap_or(0.0) >= now - 86400.0
                })
                .map(|session| {
                    let id = text(&session["id"]);
                    let live = session["liveFacts"].clone();
                    let mut own: Vec<Request> = live["requests"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|item| Request {
                            id: text(&item["id"]),
                            session_id: text(&item["sessionId"]),
                            kind: text(&item["kind"]),
                            at: time(&item["at"]).unwrap_or(0.0),
                        })
                        .collect();
                    for request in requests.iter().filter(|request| request.session_id == id) {
                        if !own.iter().any(|item| item.id == request.id) {
                            own.push(request.clone());
                        }
                    }
                    Facts {
                        phase: text(&session["state"]),
                        observed_at: time(coalesce(&live["observedAt"], &session["updatedAt"]))
                            .unwrap_or(0.0),
                        last_event_at: time(coalesce(&live["lastEventAt"], &session["updatedAt"]))
                            .unwrap_or(0.0),
                        requests: own,
                        id,
                        live,
                    }
                })
                .collect();
            // Each request once, where it first appears, as its last listing has it.
            let mut merged: Vec<Request> = vec![];
            for item in requests
                .iter()
                .chain(sessions.iter().flat_map(|s| &s.requests))
            {
                match merged.iter_mut().find(|kept| kept.id == item.id) {
                    Some(kept) => *kept = item.clone(),
                    None => merged.push(item.clone()),
                }
            }
            let exhausted_until = vendor
                .quotas
                .iter()
                .filter(|quota| {
                    quota["status"] != "stale"
                        && quota["usedPercent"].as_f64().unwrap_or(0.0) >= 99.5
                })
                .filter_map(|quota| time(&quota["resetAt"]))
                .reduce(f64::max);
            QueueMark {
                key: vendor.key,
                is_working: vendor.working > 0,
                sessions,
                requests: merged,
                exhausted_until,
            }
        })
        .collect()
}
