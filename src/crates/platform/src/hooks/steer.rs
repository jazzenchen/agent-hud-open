//! A client's steer hooks (`agent_hud_agents::hooks::steer`): they tell the
//! session's turn in progress that a message waits for it (`crate::steers`).
//! One runs in the background of the turn until a message comes, then ends
//! with the notice, which the client adds to the turn's next model request;
//! the next tool call starts another. The hook at the turn's end carries the
//! turn on when a message is still unread.
use super::rewake::Process;
use crate::steers::Steers;
use agent_hud_agents::hooks::steer::{answer, call, Call, Kind};
use serde_json::Value;
use std::{io::Write, path::Path, time::Duration};

/// How long one waits. A turn that fails runs no hook at its end, so its
/// waiting hook leaves by itself; a longer turn's next tool call starts one.
const LIFETIME: Duration = Duration::from_secs(3_600);

/// How long a hook of the tool call that reads the messages leaves them to
/// it, so the turn is not told of what it is reading.
const SETTLE: Duration = Duration::from_secs(1);

/// How often a waiting hook looks for a message, its turn and its client.
const LOOK: Duration = Duration::from_millis(250);

/// What the turn is told. It names only ids Agent HUD made: a title or a
/// word of another agent here would carry the weight the client gives a
/// hook's context. Kept short: the client shows the user what the hook at
/// the turn's end says.
fn notice(ids: &[String], again: bool) -> String {
    let (what, it) = match ids.len() {
        1 => ("A message from another agent".to_owned(), "it"),
        many => (format!("{many} messages from other agents"), "them"),
    };
    let ids = ids.join(", ");
    let when = if again {
        "came into this turn before its context was compacted"
    } else {
        "came into this turn"
    };
    format!("[Agent HUD] {what} {when} ({ids}). Read {it} with mcp__agent_hud__read_message before you go on, and tell the user in one line what came in.")
}

/// Runs `source`'s steer hook for `payload`.
pub(super) fn run(data: &Path, source: &str, payload: &Value, output: &mut dyn Write) -> i32 {
    let Some(call) = call(source, payload) else {
        return 0;
    };
    let Some(client) = super::rewake::client() else {
        return 0;
    };
    let me = Process::of(std::process::id() as i32);
    if let Some(notice) = handle(data, &call, (me, client.process), LOOK) {
        let _ = serde_json::to_writer(&mut *output, &answer(&call, &notice));
        let _ = output.write_all(b"\n");
        let _ = output.flush();
    }
    0
}

/// What the hook tells the turn, if anything; a waiting hook returns once
/// there is something to tell or nothing to wait for.
fn handle(
    data: &Path,
    call: &Call,
    (me, client): (Process, Process),
    every: Duration,
) -> Option<String> {
    let steers = Steers::new(data);
    let session = call.session.as_str();
    let now = super::now_ms();
    let told = |ids: Vec<String>, again| (!ids.is_empty()).then(|| notice(&ids, again));
    match call.kind {
        Kind::Interrupted => {
            steers.end(session, call.turn.as_deref(), true, now);
            None
        }
        Kind::Compacted => told(steers.tell_again(session), true),
        Kind::End => {
            let turn = call.turn.as_deref()?;
            let notice = told(steers.tell_at_end(session, turn), false);
            if notice.is_none() {
                steers.end(session, Some(turn), false, now);
            }
            notice
        }
        Kind::Wait => {
            let turn = call.turn.as_deref()?;
            if !steers.begin(session, turn, (client, me), now) || !steers.stand(session, turn, me) {
                return None;
            }
            let began = std::time::Instant::now();
            let settled = began + if call.reading { SETTLE } else { Duration::ZERO };
            let until = began + LIFETIME;
            let notice = loop {
                let over = !client.runs()
                    || std::time::Instant::now() >= until
                    || !steers.stands(session, turn, me)
                    || steers.in_progress(session).as_deref() != Some(turn);
                if over {
                    break None;
                }
                if std::time::Instant::now() >= settled {
                    if let Some(notice) = told(steers.tell(session, turn), false) {
                        break Some(notice);
                    }
                }
                std::thread::sleep(every);
            };
            steers.stand_down(session, turn, me);
            notice
        }
    }
}

#[cfg(all(test, unix))]
#[path = "steer_tests_unix.rs"]
mod tests;
