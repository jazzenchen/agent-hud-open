//! Following up a message that asked for a reply, on the sender's behalf, so
//! a sender never waits on a recipient that finished without answering.
//! Twice at most for each message, and neither is a pass of its own:
//! - while the message's own turn still runs `RUNNING_CHECK_MS` after it was
//!   sent, a short check steered into that turn (Codex and DeepSeek Harness,
//!   the clients that take a steer), saying the sender waits for a reply;
//! - a minute after its own turn ended with no reply, a follow-up queued as
//!   the recipient's next turn, asking for that reply now. Only while that
//!   end is recent (`FOLLOW_UP_LATEST_MS`): one that ended longer ago, as
//!   while messages were off or the app was closed, or whose client the user
//!   turned off, is let go, not sent late.
//!
//! Both are labelled as Agent HUD's, name the message they follow up, and ask
//! for the reply with `reply_to` that message, so the reply settles it.
use super::*;

/// How long the message's own turn runs before Agent HUD steers a check in.
pub(crate) const RUNNING_CHECK_MS: i64 = 10 * 60_000;
/// How long after the message's own turn ended a follow-up may still go:
/// five minutes past the minute it waits for the reply.
pub(crate) const FOLLOW_UP_LATEST_MS: i64 = ANSWER_WAIT_MS + 5 * 60_000;

impl Messaging {
    /// Sends the checks and follow-ups now due. True when the journal changed.
    pub(crate) fn follow_ups(&mut self, host: &Host<'_>, now: i64) -> bool {
        if !self.preferences.enabled {
            return false;
        }
        let replies = self.journal.replies();
        let mut due = Vec::new();
        let mut let_go = Vec::new();
        for message in self.journal.messages() {
            if !message.asked || message.follows.is_some() || message.replied_by.is_some() {
                continue;
            }
            let owed = !replies.contains(&message.id)
                && !message.to.session_id.is_empty()
                && !matches!(message.state.as_str(), "refused" | "expired" | "uncertain")
                && self.allows(&message.to.client);
            if message.follow_up_due() {
                let waited = now - message.ended_at_ms.unwrap_or(now);
                if !owed || waited > FOLLOW_UP_LATEST_MS {
                    let_go.push(message.id.clone());
                } else if waited > ANSWER_WAIT_MS {
                    due.push((message.id.clone(), false));
                }
            } else if owed
                && !message.checked
                && !message.answered
                && message.ended_at_ms.is_none()
                && message.steered_into.is_none()
                && now - message.sent_at_ms >= RUNNING_CHECK_MS
                && self.own_turn_runs(host, message)
            {
                due.push((message.id.clone(), true));
            }
        }
        // Owed no more: marked followed, so nothing waits on it.
        for kept in self
            .journal
            .messages_mut()
            .filter(|m| let_go.contains(&m.id))
        {
            kept.followed = true;
        }
        let changed = !due.is_empty() || !let_go.is_empty();
        for (id, steer) in due {
            let Some(message) = self.journal.find(&id).cloned() else {
                continue;
            };
            let follow_up = self.follow_up(host, &message, steer, now);
            if let Some(kept) = self.journal.messages_mut().find(|m| m.id == id) {
                if steer {
                    kept.checked = true;
                } else {
                    kept.followed = true;
                }
            }
            self.journal.add(follow_up);
        }
        changed
    }

    /// Whether the user lets messages go into this client's sessions.
    fn allows(&self, client: &str) -> bool {
        match client {
            "claude" => self.preferences.claude,
            "deepseek" => self.preferences.deepseek,
            _ => true,
        }
    }

    /// Whether the recipient takes a steer and is running the message's own
    /// turn now, not waiting on an approval or a question.
    fn own_turn_runs(&self, host: &Host<'_>, message: &Message) -> bool {
        if !Provider::parse(&message.to.client).is_some_and(Provider::takes_steer) {
            return false;
        }
        let Some(session) = find_in(host.snapshot, &message.to.client, &message.to.session_id)
        else {
            return false;
        };
        if session["state"] != "running" {
            return false;
        }
        let live = &session["liveFacts"];
        if live["delivered"].as_str() == Some(message.id.as_str())
            || live["prompt"]
                .as_str()
                .is_some_and(|prompt| transcript::is_words(prompt, message))
        {
            return true;
        }
        // Read the conversation only for a turn that began after the send,
        // which a queued message's own turn did: an older one is not it.
        let began = live["turnStartedAt"]
            .as_str()
            .map(transcript::at)
            .unwrap_or(0);
        began >= message.sent_at_ms - transcript::SLACK_MS
            && transcript::answer(host, message).is_some_and(|found| !found.done)
    }

    /// Sends the check (`steer`) or the follow-up for `message`, and the
    /// journal's record of it: the message's chain and pass, as it is no
    /// pass of its own, and settled from the start, as no answer to it is
    /// waited for but the reply to the message it follows up.
    fn follow_up(&self, host: &Host<'_>, message: &Message, steer: bool, now: i64) -> Message {
        let language = language(host.snapshot);
        let id = message_id();
        let client = message.to.client.as_str();
        let delivered = words::follow_up(language, client, &id, &message.id, steer);
        let receipt = self.clients.send(SendRequest {
            provider: provider(client),
            session_id: message.to.session_id.clone(),
            request_id: id.clone(),
            text: delivered.clone(),
            form: PromptForm::Labelled,
            operation: if steer {
                Operation::Steer
            } else {
                Operation::Enqueue
            },
            expected_turn_id: None,
            model: None,
        });
        let surface = agent_hud_core::naming::surfaces::Surface::new(
            agent_hud_core::naming::surfaces::Category::Mcp,
            &message.from.client,
        );
        if let Ok(receipt) = &receipt {
            self.clients.delivered(receipt, &surface, now);
        }
        let (state, reason) = match &receipt {
            Ok(receipt) if receipt.state == DeliveryState::Steered => ("steered", None),
            Ok(receipt) => (
                delivered_or_refused(receipt.state),
                receipt.reason.as_deref().map(plain),
            ),
            Err(error) => ("refused", Some(plain(&error.to_string()))),
        };
        let steered_into = receipt
            .as_ref()
            .ok()
            .filter(|receipt| receipt.state == DeliveryState::Steered)
            .and_then(|receipt| receipt.turn_id.clone());
        Message {
            id: id.clone(),
            kind: Kind::Send,
            chain: message.chain.clone(),
            count: message.count,
            from: message.from.clone(),
            to: message.to.clone(),
            form: PromptForm::Labelled,
            reply_to: None,
            request_id: id,
            sent_at_ms: now,
            cursor: None,
            fingerprint: transcript::fingerprint(&delivered),
            state: state.into(),
            reason,
            turn: None,
            answered: true,
            replied_by: None,
            steered_into,
            steer_waits: receipt
                .as_ref()
                .is_ok_and(|receipt| steer && receipt.state == DeliveryState::Waiting),
            call: None,
            asked: false,
            ended_at_ms: None,
            checked: false,
            followed: false,
            follows: Some(message.id.clone()),
        }
    }
}
