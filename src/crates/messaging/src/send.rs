//! `send_message`, and the chain and pass a message joins.
use super::*;

impl Messaging {
    pub(crate) fn send(
        &mut self,
        caller: &Caller,
        arguments: &Value,
        host: &Host<'_>,
        now: i64,
    ) -> Result<Value> {
        let language = language(host.snapshot);
        let text = arguments["text"].as_str().ok_or("text is required")?;
        let reply_to = arguments["reply_to"].as_str().filter(|id| !id.is_empty());
        let reply_hint = arguments["reply_hint"].as_bool().unwrap_or(true);
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
        if let Some(reason) = check_text(text, language) {
            return Ok(refused(&reason));
        }
        // A reply goes to the very session that sent its parent, in that
        // session's client; a `to` names a session of whichever client has it.
        let target = match (named, &replied) {
            (Some(to), _) => find(host.snapshot, to),
            (None, Some(parent)) => {
                find_in(host.snapshot, &parent.from.client, &parent.from.session_id)
            }
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
        // Only Codex takes a message into a turn in progress (Open #68).
        if steer && Provider::parse(client) != Some(Provider::Codex) {
            return Ok(refused(&words::steer_unsupported(language, client)));
        }
        // A model only when named, and only where a message can switch it.
        let model = match self.named_model(client, arguments, true, language) {
            Ok(model) => model,
            Err(reason) => return Ok(refused(&reason)),
        };
        let sender = party(host.snapshot, &caller.client, &caller.session_id);
        let recipient = party(host.snapshot, client, to);
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
        let form = self.form();
        let from = sender.clone();
        let delivered = words::compose(
            language, form, &from, client, &id, reply_to, reply_hint, text,
        );
        let delivered = if steer {
            words::steered(language, form, delivered)
        } else {
            delivered
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
            to: party(host.snapshot, client, to),
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
        };
        // A reply that went back to its parent's sender settles the parent:
        // get_message says it was replied to, and this reply's plane is the
        // one back, so the end of the turn that sent it throws no second.
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
        Ok(json!({"id":id,"state":state,"reason":reason,"pass":count}))
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
        match parent {
            Some(parent) => (Some(parent.chain.clone()), parent.count + 1),
            None => (None, 1),
        }
    }

    pub(crate) fn form(&self) -> PromptForm {
        if self.preferences.as_user {
            PromptForm::Own
        } else {
            PromptForm::Labelled
        }
    }
}
