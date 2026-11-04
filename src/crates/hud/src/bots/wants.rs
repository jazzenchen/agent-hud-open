//! What a mark's facts ask it to show, and the badge it wears.
use super::facts::{
    coalesce, compacting, ended, holds, inflight, limited, started, subagents, time, tool_seen,
};
use super::*;
use std::cmp::Ordering;

/// A JavaScript comparator's number as an ordering: NaN, like 0, decides nothing.
fn order(difference: f64) -> Ordering {
    difference.partial_cmp(&0.0).unwrap_or(Ordering::Equal)
}

/// The look a session's activity asks for: its open call's tool (a shell call
/// 20 s on is a long command), a tool seen under 10 s ago, else thinking.
fn look_of(session: &Facts, now: f64) -> &'static str {
    let activity = &session.live["activity"];
    if activity.is_null() {
        return "plain";
    }
    if activity["retrying"] == true {
        return "thinking";
    }
    let call = &activity["openCall"];
    if call["work"] == "shell" && now - time(&call["at"]).unwrap_or(now) >= 20.0 {
        return "tool-long-command";
    }
    let tool = match coalesce(&call["work"], &activity["work"]).as_str() {
        Some("shell" | "command") => "tool-command",
        Some("edit") => "tool-edit",
        Some("read") => "tool-read",
        _ => "thinking",
    };
    if tool == "thinking"
        || !call.is_null()
        || now - tool_seen(session).unwrap_or(f64::NEG_INFINITY) < 10.0
    {
        return tool;
    }
    "thinking"
}

/// The session a look follows: the current owner while it qualifies, else the
/// latest observed, then the latest turn, then the smaller id.
fn owner_of<'a>(sessions: &[&'a Facts], current: Option<&str>) -> Option<&'a Facts> {
    let owner = sessions
        .iter()
        .copied()
        .find(|session| Some(session.id.as_str()) == current);
    owner.or_else(|| {
        sessions.iter().copied().min_by(|a, b| {
            let turn = |session: &Facts| started(session).unwrap_or(f64::NEG_INFINITY);
            order(b.observed_at - a.observed_at)
                .then(order(turn(b) - turn(a)))
                .then_with(|| a.id.cmp(&b.id))
        })
    })
}

/// What a mark's facts ask it to show: the weightiest request, a compaction,
/// the owner's work (a tooling session's when the owner only thinks), else how
/// the agent rests.
pub(super) fn want_of(mark: &QueueMark, owner: Option<&str>, now: f64) -> Want {
    let rank = |kind: &str| match kind {
        "permission" => 2.0,
        "question" => 1.0,
        "plan" => 0.0,
        _ => f64::NAN,
    };
    let request = mark.requests.iter().min_by(|a, b| {
        order(rank(&b.kind) - rank(&a.kind))
            .then(order(a.at - b.at))
            .then_with(|| a.id.cmp(&b.id))
    });
    if let Some(request) = request {
        return Want {
            look: request.kind.clone(),
            owner: Some(request.session_id.clone()),
            subagents: false,
            holds: false,
        };
    }
    let compacts: Vec<&Facts> = mark
        .sessions
        .iter()
        .filter(|s| compacting(s, now))
        .collect();
    if let Some(compact) = owner_of(&compacts, owner) {
        return Want {
            look: "compacting".into(),
            owner: Some(compact.id.clone()),
            subagents: subagents(compact),
            holds: false,
        };
    }
    let busy: Vec<&Facts> = mark.sessions.iter().filter(|s| inflight(s)).collect();
    if let Some(mut session) = owner_of(&busy, owner) {
        let mut look = look_of(session, now);
        if look == "thinking" {
            let tooling: Vec<&Facts> = busy
                .iter()
                .copied()
                .filter(|s| look_of(s, now).starts_with("tool-"))
                .collect();
            if let Some(tooling) = owner_of(&tooling, None) {
                session = tooling;
                look = look_of(tooling, now);
            }
        }
        return Want {
            look: look.into(),
            owner: Some(session.id.clone()),
            subagents: subagents(session),
            holds: holds(session, now),
        };
    }
    let look = if mark.is_working {
        "plain"
    } else if mark.sessions.iter().any(|s| s.phase == "unverified") {
        "no-news"
    } else if mark
        .sessions
        .iter()
        .all(|s| now - s.last_event_at >= 3600.0)
    {
        "asleep"
    } else {
        "idle"
    };
    Want {
        look: look.into(),
        owner: None,
        subagents: false,
        holds: false,
    }
}

/// A failure under 15 minutes old that ended after the last look, with no turn
/// begun since; else a limit with no turn begun since, or a window used up.
pub(super) fn badge_of(mark: &QueueMark, looked_at: f64, now: f64) -> Option<&'static str> {
    let prompted = mark
        .sessions
        .iter()
        .map(|session| started(session).unwrap_or(f64::NEG_INFINITY))
        .fold(f64::NEG_INFINITY, f64::max);
    let failed = mark.sessions.iter().any(|session| {
        let end = ended(session).unwrap_or(f64::NEG_INFINITY);
        session.live["outcome"] == "failed"
            && now - end < 900.0
            && end > looked_at
            && prompted <= end
    });
    if failed {
        return Some("failed");
    }
    let limit = mark.sessions.iter().any(|session| {
        limited(session, now) && prompted <= ended(session).unwrap_or(f64::INFINITY)
    }) || mark.exhausted_until.unwrap_or(f64::NEG_INFINITY) > now;
    limit.then_some("limited")
}
