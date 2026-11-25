//! `send_message`: checks, delivery and the journal's record of what was sent.
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
        // A reply goes back to whoever sent the message it answers.
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
            if self.no_codex_turn(target) {
                return Ok(refused(&words::no_turn_to_steer(language)));
            }
        }
        // A model only when named, and only where a message can switch it.
        let model = match self.named_model(client, arguments, true, language) {
            Ok(model) => model,
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
        });
        if let Ok(receipt) = &receipt {
            self.clients.delivered(receipt, &caller.surface(), now);
        }
        let (state, reason) = self.outcome(&receipt, steer, (client, to), language);
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
            steer_waits: receipt
                .as_ref()
                .is_ok_and(|receipt| steer && receipt.state == DeliveryState::Waiting),
            call: self.calling.clone(),
            asked: ask && !own_steer,
            ended_at_ms: None,
            checked: false,
            followed: false,
            follows: None,
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
                // A reply to a follow-up answers the message it follows up.
                if let Some(followed) = parent.follows.clone() {
                    if let Some(original) = self.journal.messages_mut().find(|m| m.id == followed) {
                        original.answered = true;
                        original.replied_by.get_or_insert_with(|| id.clone());
                    }
                }
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
}
