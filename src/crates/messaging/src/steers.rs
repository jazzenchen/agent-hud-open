//! `read_message`: the turn in progress a steer waits for reads it. The
//! client's hook only says that one waits (`agent_hud_platform::steers`). Also
//! how a send went, steers among them.
use super::*;
use agent_hud_platform::steers::Steered;

impl Messaging {
    pub(crate) fn read_steers(
        &mut self,
        caller: Option<&Caller>,
        host: &Host<'_>,
    ) -> Result<Value> {
        let language = language(host.snapshot);
        let Some(caller) = caller else {
            return Ok(refused(&words::no_caller(language)));
        };
        // A sub-agent's thread is its own: nothing is left for it, and what
        // was sent to the session is the session's to read.
        if caller.client != "codex" || is_sub_agent(host.snapshot, caller) {
            return Ok(json!({"messages":[],"note":words::nothing_to_read(language)}));
        }
        let read = self
            .clients
            .read_steers(&caller.session_id, caller.turn_id.as_deref());
        if read.is_empty() {
            return Ok(json!({"messages":[],"note":words::nothing_to_read(language)}));
        }
        for reading in &read {
            if let Some(message) = self
                .journal
                .messages_mut()
                .find(|message| message.id == reading.id)
            {
                message.state = "steered".into();
                message.reason = None;
                message.steered_into = Some(reading.turn.clone());
            }
        }
        self.journal.save();
        let messages = read
            .into_iter()
            .map(|reading| json!({"id":reading.id,"text":reading.text}))
            .collect::<Vec<_>>();
        Ok(json!({"messages":messages,"note":words::read_note(language)}))
    }

    /// Whether a steer would be left for the hooks of a Codex turn that is
    /// over: one that failed ran no hook at its end.
    pub(crate) fn no_codex_turn(&self, target: &Value) -> bool {
        target["source"] == "codex" && state(target) == "idle" && !self.clients.codex_takes_steer()
    }

    /// How a send went, as the journal keeps it, and what the sender reads.
    pub(crate) fn outcome(
        &self,
        receipt: &delivery::Result<delivery::Receipt>,
        steer: bool,
        (client, to): (&str, &str),
        language: Language,
    ) -> (&'static str, Option<String>) {
        match receipt {
            Ok(receipt) if receipt.state == DeliveryState::Steered => ("steered", None),
            // Left for the turn's own hooks: it reads the words itself.
            Ok(receipt) if steer && receipt.state == DeliveryState::Waiting => {
                ("queued", Some(words::steer_waits(language)))
            }
            Ok(receipt)
                if receipt
                    .reason
                    .as_deref()
                    .is_some_and(|reason| reason.contains(delivery::NO_TURN_TO_STEER)) =>
            {
                ("refused", Some(words::no_turn_to_steer(language)))
            }
            // Nothing that takes a steer answers for this Codex thread: say
            // what the sender can do instead of naming a socket (#178).
            Ok(receipt)
                if steer
                    && client == "codex"
                    && receipt.state == DeliveryState::Failed
                    && receipt
                        .reason
                        .as_deref()
                        .is_some_and(|reason| reason.starts_with("Unavailable(")) =>
            {
                ("refused", Some(self.codex_unanswered(to, language)))
            }
            Ok(receipt) => (
                delivered_or_refused(receipt.state),
                receipt.reason.as_deref().map(plain),
            ),
            Err(error) => ("refused", Some(plain(&error.to_string()))),
        }
    }

    /// Steers left for a turn's hooks whose turn ended without reading them;
    /// true when any did.
    pub(crate) fn missed_steers(&mut self) -> bool {
        let clients = self.clients.clone();
        let mut changed = false;
        for message in self.journal.messages_mut() {
            if !message.steer_waits || message.state != "queued" {
                continue;
            }
            match clients.steered(&message.to.session_id, &message.id) {
                Steered::Waiting => continue,
                Steered::Read(turn) => {
                    message.state = "steered".into();
                    message.reason = None;
                    message.steered_into = Some(turn);
                }
                Steered::Missed => {
                    message.state = "expired".into();
                    message.reason = Some(STEER_MISSED.into());
                }
            }
            changed = true;
        }
        changed
    }
}

/// Why a steer left for a turn's hooks never went in (`list_sent_messages`).
const STEER_MISSED: &str =
    "The turn ended before it read this, so it did not go in and was not queued";
