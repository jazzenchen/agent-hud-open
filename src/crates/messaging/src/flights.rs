//! The paper plane: one flight for each pass, and one back once an answer reads;
//! one too for the person's own words from the clipboard window.
use super::*;
use serde::Serialize;

impl Messaging {
    /// Throws the plane back once for each message whose answer arrived: the
    /// collector's live facts say a turn ended (nothing is read while one
    /// runs), the message's own turn is found and ended, and its reply reads
    /// from the recipient's transcript. Its own turn ended with nothing to read:
    /// given up a minute on, not flown. Its own turn not found yet: looked for
    /// again, until the two-hour cutoff. A turn that ended long after is not
    /// flown; a message answered with send_message is already settled (its
    /// reply flew). A reply's plain answer flies nothing: it reaches no one,
    /// only a reply to the reply does, with its own plane. True when the
    /// journal changed.
    pub(crate) fn fly_answers(&mut self, host: &Host<'_>, now: i64) -> bool {
        let mut flights = Vec::new();
        let mut changed = false;
        let replies = self.journal.replies();
        for message in self.journal.messages_mut() {
            if message.answered
                || message.to.session_id.is_empty()
                || matches!(message.state.as_str(), "refused" | "expired" | "uncertain")
                // Its turn has not read it yet.
                || (message.steer_waits && message.steered_into.is_none())
            {
                continue;
            }
            if now - message.sent_at_ms > 2 * 3_600_000 {
                message.answered = true;
                changed = true;
                continue;
            }
            let Some(session) = find_in(host.snapshot, &message.to.client, &message.to.session_id)
            else {
                continue;
            };
            let live = &session["liveFacts"];
            let began = live["turnStartedAt"]
                .as_str()
                .map(transcript::at)
                .unwrap_or(0);
            // Live facts that name this message as the turn that ended are
            // final even while the session's next turn already runs.
            let ours = live["delivered"].as_str() == Some(message.id.as_str());
            if (!ours && session["state"] == "running")
                || live["endedAt"].is_null()
                || began < message.sent_at_ms - 5_000
            {
                continue;
            }
            let Some(found) = transcript::answer(host, message) else {
                continue;
            };
            // Where the turn was found: the next look starts there.
            if message.turn.as_ref() != Some(&found.turn) {
                message.turn = Some(found.turn.clone());
                changed = true;
            }
            if !found.done {
                continue;
            }
            // Asked and not answered with a reply: Agent HUD follows up a
            // minute after this turn ended (`follow_ups`).
            if message.asked
                && message.ended_at_ms.is_none()
                && message.replied_by.is_none()
                && !replies.contains(&message.id)
            {
                message.ended_at_ms = Some(ended_at(&found, live, now));
                changed = true;
            }
            if replies.contains(&message.id) {
                message.answered = true;
                changed = true;
                continue;
            }
            if found.answer.is_some() {
                message.answered = true;
                flights.push(message.clone());
                continue;
            }
            // Nothing to read a minute after its own turn ended (when that
            // is not known, the session's latest turn, which ended after it):
            // no plane, and no more looks.
            if now - ended_at(&found, live, now) > ANSWER_WAIT_MS {
                message.answered = true;
                changed = true;
            }
        }
        for message in &flights {
            let left = self.left_of(&message.chain, &message.from.session_id);
            (self.fly)(flight(
                &message.to,
                &message.from,
                same_session(&left, &message.to.session_id),
                false,
                false,
            ));
        }
        changed || !flights.is_empty()
    }
    /// The session on the left of a chain's lane: the one that sent first.
    pub(crate) fn left_of(&self, chain: &str, otherwise: &str) -> String {
        self.journal
            .find(chain)
            .map(|first| first.from.session_id.clone())
            .unwrap_or_else(|| otherwise.to_owned())
    }
}

/// A flight every HUD draws: the paper plane leaves one agent's mark and
/// reaches another agent's, or falls on the way. Agents are named by their
/// source, as sessions name them (`claude`, `codex`, …). The HUD draws nothing
/// for it outside the flight itself.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Flight {
    /// Flights between the same two sessions share a lane: one is in the air
    /// at a time, and more that come meanwhile ride along as a count.
    pub lane: String,
    pub from: String,
    pub to: String,
    /// Where marks have no place of their own (around the notch), the sender
    /// stands on the left (the chain's first sender); otherwise on the right.
    pub from_left: bool,
    /// The receiver is new: it appears first, then catches it.
    pub arrives: bool,
    /// It was refused: it falls halfway and the sender is left puzzled.
    pub falls: bool,
}

/// The person as a flight's sender: their own words from the clipboard
/// window leave from its end of the dock, the bubble, not from an agent's
/// mark. Around the notch that end sits on the tail's side, the right.
pub const PERSON: &str = "person";

/// The person's words on their way to `to`, a session there is or one that
/// appears (`arrives`).
pub(crate) fn person_flight(to: &Party, arrives: bool) -> Flight {
    let person = Party {
        client: PERSON.into(),
        session_id: String::new(),
        title: None,
    };
    flight(&person, to, false, arrives, false)
}

/// One flight from `from`'s mark to `to`'s.
pub(crate) fn flight(
    from: &Party,
    to: &Party,
    from_left: bool,
    arrives: bool,
    falls: bool,
) -> Flight {
    let mut ends = [
        format!("{}:{}", from.client, from.session_id),
        format!("{}:{}", to.client, to.session_id),
    ];
    ends.sort();
    Flight {
        lane: ends.join("|"),
        from: from.client.clone(),
        to: to.client.clone(),
        from_left,
        arrives,
        falls,
    }
}

/// When the message's own turn ended: as its conversation says, else as the
/// session's latest turn says (it ended after), else now.
fn ended_at(found: &transcript::Found, live: &Value, now: i64) -> i64 {
    found
        .ended_at
        .as_ref()
        .and_then(Value::as_str)
        .or(live["endedAt"].as_str())
        .map(transcript::at)
        .filter(|at| *at > 0)
        .unwrap_or(now)
}
