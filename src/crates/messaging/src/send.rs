//! `send_message`, and the chain and pass a message joins.
use super::*;

impl Messaging {
    pub(crate) fn send(
        &mut self,
        caller: &Caller,
        arguments: &Value,
        host: &Host<'_>,
        extras: &[Value],
        now: i64,
    ) -> Result<Value> {
        let language = language(host.snapshot);
        let text = arguments["text"].as_str().ok_or("text is required")?;
        if is_sub_agent(host.snapshot, caller) {
            return Ok(refused(&words::from_sub_agent(language)));
        }
        let reply_to = arguments["reply_to"].as_str().filter(|id| !id.is_empty());
        // Queued by default; a steer goes into the recipient's turn in progress.
        let steer = match arguments["delivery"].as_str() {
            None | Some("queue") => false,
            Some("steer") => true,
            Some(_) => return Err("delivery is queue or steer".into()),
        };
        let replied = reply_to
            .and_then(|parent| self.journal.find(parent))
            .cloned();
        if let Some(parent) = reply_to {
            if replied.as_ref().is_none_or(|message| {
                !same_identity(
                    (&message.to.client, &message.to.session_id),
                    (&caller.client, &caller.session_id),
                )
            }) {
                return Ok(refused(&words::unknown_reply(language, parent)));
            }
        }
        let named = arguments["to"].as_str().filter(|to| !to.is_empty());
        if named.is_none() && replied.is_none() {
            return Err("to is required, unless reply_to names a message you received".into());
        }
        let mut owned_extras;
        let extras = if let Some(row) = replied
            .as_ref()
            .and_then(|parent| self.reply_extra(host, extras, &parent.from))
        {
            owned_extras = extras.to_vec();
            owned_extras.push(row);
            self.sync_targets(host, &owned_extras);
            owned_extras.as_slice()
        } else {
            extras
        };
        // A reply and a target that name different sessions: the caller has
        // two answers to where this goes. Sending it to `to` and filing it as
        // a reply to the other party's message was the wrong one, silently.
        if let (Some(target), Some(parent)) = (named, replied.as_ref()) {
            if let Some(found) = find_any(host.snapshot, extras, target) {
                let found_id = found["id"].as_str().unwrap_or(target);
                let found_client = found["source"].as_str().unwrap_or_default();
                if !same_identity(
                    (found_client, found_id),
                    (parent.from.client.as_str(), parent.from.session_id.as_str()),
                ) {
                    return Ok(refused(&words::conflicting_targets(
                        language,
                        found_id,
                        &parent.from.session_id,
                    )));
                }
            }
        }
        if let Some(reason) = check_text(text, language) {
            return Ok(refused(&reason));
        }
        // A reply goes to the very session that sent its parent, in that
        // session's client; a `to` names a session of whichever client has it.
        let target = match (named, &replied) {
            (Some(to), _) => find_any(host.snapshot, extras, to),
            (None, Some(parent)) => find_in_any(
                host.snapshot,
                extras,
                &parent.from.client,
                &parent.from.session_id,
            ),
            (None, None) => None,
        };
        let Some(target) = target else {
            return Ok(refused(&words::unknown(language)));
        };
        let to = target["id"].as_str().unwrap_or_default();
        let client = target["source"].as_str().unwrap_or_default();
        if same_identity((client, to), (&caller.client, &caller.session_id)) {
            return Ok(refused(&words::to_self(language)));
        }
        if Provider::parse(client).is_none() {
            return Ok(refused(&words::other_client(language)));
        }
        if !target["parentId"].is_null() {
            return Ok(refused(&words::child(language)));
        }
        // Only a client that takes a message into a turn in progress gets a
        // steer (Open #68); a reply is always queued, and a session waiting on
        // an approval or a question would not read one until it is answered.
        if steer {
            if !Provider::parse(client).is_some_and(Provider::takes_steer) {
                return Ok(refused(&words::steer_unsupported(language, client)));
            }
            if reply_to.is_some() {
                return Ok(refused(&words::steer_reply(language)));
            }
            if state(target) == "waitingForApproval" {
                return Ok(refused(&words::steer_waiting(language)));
            }
        }
        let (model, reasoning_effort) =
            match self.send_options(client, to, arguments, steer, language) {
                Ok(options) => options,
                Err(reason) => return Ok(refused(&reason)),
            };
        let sender = party_any(host.snapshot, extras, &caller.client, &caller.session_id);
        let recipient = party_any(host.snapshot, extras, client, to);
        // One that cannot take it now is refused before anything is queued.
        if let (Value::Bool(false), reason) = self.reachable(
            client,
            to,
            &self.loaded(),
            (target, host.snapshot),
            language,
        ) {
            (self.fly)(flight(&sender, &recipient, true, false, true));
            return Ok(refused(&reason));
        }
        let (chain, count) = self.next_pass(caller, host, replied.as_ref());
        let left = chain
            .as_deref()
            .map(|chain| self.left_of(chain, &caller.session_id))
            .unwrap_or_else(|| caller.session_id.clone());
        let from_left = same_session(&left, &caller.session_id);
        if let Some(limit) = self.preferences.limit.filter(|limit| count > *limit) {
            (self.fly)(flight(&sender, &recipient, from_left, false, true));
            return Ok(refused(&words::limit(language, limit)));
        }
        let id = message_id();
        // Another agent's words never read as the user's in the middle of a
        // turn: a steer carries Agent HUD's label even under "Send as me".
        let (form, ask) = self.shape(arguments, steer);
        // A steer the agent chose itself ends with its own last line, which
        // asks for no reply, in place of the reply line.
        let own_steer = steer && self.preferences.steer_on_own;
        let from = sender.clone();
        let delivered = words::compose(
            language,
            form,
            ask && !own_steer,
            &from,
            client,
            &id,
            reply_to,
            text,
        );
        let delivered = match (steer, own_steer) {
            (true, true) => {
                words::steer_last_line(language, client, &id, words::steered(language, delivered))
            }
            (true, false) => words::steered(language, delivered),
            _ => delivered,
        };
        // Where the recipient's transcript ends now: its answer is after this.
        // The store finds the answer by when it was sent instead.
        let cursor = match transcript::stored(host, client) {
            Some(_) => None,
            None => transcript::end(host.query, to),
        };
        let provider = provider(client);
        let receipt = self.clients.send(SendRequest {
            provider,
            session_id: to.into(),
            request_id: id.clone(),
            text: delivered.clone(),
            form,
            // A steer names no turn: it goes into the one in progress, if any.
            operation: if steer {
                Operation::Steer
            } else {
                Operation::Enqueue
            },
            expected_turn_id: None,
            model,
            reasoning_effort,
        });
        if let Ok(receipt) = &receipt {
            self.clients.delivered(receipt, &caller.surface(), now);
        }
        let (state, reason) = match &receipt {
            Ok(receipt) if receipt.state == DeliveryState::Steered => ("steered", None),
            Ok(receipt)
                if receipt
                    .reason
                    .as_deref()
                    .is_some_and(|reason| reason.contains(delivery::NO_TURN_TO_STEER)) =>
            {
                ("refused", Some(words::no_turn_to_steer(language)))
            }
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
        let message = Message {
            id: id.clone(),
            kind: Kind::Send,
            chain: chain.unwrap_or_else(|| id.clone()),
            count,
            from,
            to: party_any(host.snapshot, extras, client, to),
            form,
            reply_to: reply_to.map(str::to_owned),
            request_id: id.clone(),
            sent_at_ms: now,
            cursor,
            fingerprint: transcript::fingerprint(&delivered),
            state: state.into(),
            reason: reason.clone(),
            turn: None,
            answered: false,
            replied_by: None,
            steered_into,
            call: self.calling.clone(),
        };
        // A reply that went back to its parent's sender settles the parent:
        // the journal names this reply, and its plane is the one back, so the
        // end of the turn that sent it throws no second.
        if state != "refused" {
            if let Some(parent) = replied
                .filter(|parent| {
                    same_identity((&parent.from.client, &parent.from.session_id), (client, to))
                })
                .and_then(|parent| self.journal.messages_mut().find(|m| m.id == parent.id))
            {
                parent.answered = true;
                parent.replied_by.get_or_insert_with(|| id.clone());
            }
        }
        self.journal.add(message);
        (self.fly)(flight(
            &sender,
            &recipient,
            from_left,
            false,
            state == "refused",
        ));
        Ok(json!({"state":state,"reason":reason}))
    }

    /// The chain the caller's message joins and its pass in it. The caller's
    /// current turn began with one of Agent HUD's messages: the chain goes
    /// on. It began with the user's own words: a new chain, the first pass.
    /// `answers` is the message a reply answers.
    pub(crate) fn next_pass(
        &self,
        caller: &Caller,
        host: &Host<'_>,
        answers: Option<&Message>,
    ) -> (Option<String>, u32) {
        let received = self
            .journal
            .received(&caller.client, &caller.session_id)
            .collect::<Vec<_>>();
        if received.is_empty() {
            return (None, 1);
        }
        let parent = match transcript::current_prompt(host, caller) {
            Some(Some(prompt)) => received
                .iter()
                .find(|message| transcript::is_message(&prompt, message))
                .copied(),
            Some(None) => None,
            // The transcript could not be read: without a sign of the user,
            // the message it answers, else the newest it received, goes on
            // counting.
            None => answers.or(received.first().copied()),
        };
        // A steer taken in the caller's turn in progress began no turn, so
        // the prompt that began it names an earlier sender, or the user: the
        // steer is that turn's parent too, and the later pass counts on.
        let turn = caller.turn_id.clone().or_else(|| {
            find_in(host.snapshot, &caller.client, &caller.session_id)
                .and_then(|session| session["liveFacts"]["turnId"].as_str())
                .map(str::to_owned)
        });
        let steered = turn.and_then(|turn| {
            received
                .iter()
                .find(|message| message.steered_into.as_deref() == Some(turn.as_str()))
                .copied()
        });
        let parent = match (parent, steered) {
            (Some(parent), Some(steered)) => Some(if steered.count > parent.count {
                steered
            } else {
                parent
            }),
            (parent, steered) => parent.or(steered),
        };
        match parent {
            Some(parent) => (Some(parent.chain.clone()), parent.count + 1),
            None => (None, 1),
        }
    }

    /// How a message goes in, and whether its last line asks for a reply.
    /// The user's "Send as me" makes it their own words, except a `labelled`
    /// one (a steer). With "Ask for replies only when needed" the sender's
    /// `require_reply` decides the reply line, and a message that needs a
    /// reply is labelled under "Send as me" too: the recipient has to know
    /// whom it answers. Without it every labelled message asks.
    pub(crate) fn shape(&self, arguments: &Value, labelled: bool) -> (PromptForm, bool) {
        let ask = !self.preferences.require_reply || arguments["require_reply"] == true;
        let own = self.preferences.as_user && !labelled && !(self.preferences.require_reply && ask);
        if own {
            (PromptForm::Own, false)
        } else {
            (PromptForm::Labelled, ask)
        }
    }
}
