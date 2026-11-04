//! How a mark moves toward what it wants, and the events it plays.
use super::*;

/// bot-state.ts `isWorking`.
pub(super) fn is_working(look: &str) -> bool {
    matches!(look, "compacting" | "thinking" | "plain") || look.starts_with("tool-")
}

/// bot-state.ts `isCalm`.
fn is_calm(look: &str) -> bool {
    matches!(look, "no-news" | "idle" | "asleep")
}

/// bot-state.ts `eventRank`.
fn event_rank(event: &str) -> u8 {
    match event {
        "failed" => 2,
        "limit-hit" => 1,
        _ => 0,
    }
}

/// Moves a mark toward what its facts want. A change to or from attention, rest
/// or compaction, or to another session, shows at once; another working look
/// shows once wanted 2 s and 6 s after the last change, 12 s while 4 changes in
/// 30 s guard it. True when work follows a rest: asleep, or idle 5 minutes.
pub(super) fn follow(mark: &mut Mark, want: &Want, now: f64, resting_from: f64) -> bool {
    let Some(shown) = mark.look.clone() else {
        show(mark, want, now);
        if is_calm(&want.look) {
            mark.resting_since = Some(resting_from);
        }
        return false;
    };
    if mark.guarded && now - mark.changes.last().copied().unwrap_or(now) >= 30.0 {
        mark.guarded = false;
        mark.changes.clear();
    }
    if want.look == shown {
        mark.wanted = None;
        show(mark, want, now);
        return false;
    }
    if !is_working(&want.look)
        || !is_working(&shown)
        || want.look == "compacting"
        || shown == "compacting"
        || want.owner != mark.owner
    {
        let rested = shown == "asleep"
            || shown == "idle" && now - mark.resting_since.unwrap_or(now) >= 300.0;
        show(mark, want, now);
        return is_working(&want.look) && rested;
    }
    let since = match &mark.wanted {
        Some(wanted) if wanted.look == want.look => wanted.since,
        _ => {
            mark.wanted = Some(Wanted {
                look: want.look.clone(),
                since: now,
            });
            now
        }
    };
    if now - since >= 2.0 && now - mark.since >= if mark.guarded { 12.0 } else { 6.0 } {
        show(mark, want, now);
    } else {
        mark.subagents = want.subagents;
        mark.holds = want.holds;
    }
    false
}

/// Shows what a mark wants. A new working look counts as a change; a calm look
/// after any other begins a rest.
fn show(mark: &mut Mark, want: &Want, now: f64) {
    if is_working(&want.look) {
        mark.owner = want.owner.clone();
        if mark.look.as_deref() != Some(want.look.as_str()) {
            mark.since = now;
            mark.changes.retain(|at| now - at < 30.0);
            mark.changes.push(now);
            if mark.changes.len() >= 4 {
                mark.guarded = true;
            }
        }
    } else if is_calm(&want.look) {
        mark.owner = None;
        if !mark.look.as_deref().is_some_and(is_calm) {
            mark.resting_since = Some(now);
        }
    }
    mark.look = Some(want.look.clone());
    mark.wanted = None;
    mark.subagents = want.subagents;
    mark.holds = want.holds;
}

/// Plays an event, unless a weightier one went out under 3 s ago.
pub(super) fn tell(mark: &mut Mark, event: &'static str, now: f64) {
    if mark
        .cue
        .is_some_and(|cue| now - cue.at < 3.0 && event_rank(cue.event) > event_rank(event))
    {
        return;
    }
    mark.cue = Some(Cue { event, at: now });
}
