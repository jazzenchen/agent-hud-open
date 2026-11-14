//! `get_message`: where a message stands, and its answer once its turn ended.
use super::*;

impl Messaging {
    pub(crate) fn get(&mut self, id: &str, host: &Host<'_>, now: i64) -> Result<Value> {
        let clients = self.clients.clone();
        let language = language(host.snapshot);
        let Some(message) = self.journal.find(id).cloned() else {
            return Ok(
                json!({"id":id,"state":"unknown","reason":words::unknown_message(language)}),
            );
        };
        let mut message = message;
        let mut state = message.state.clone();
        let mut reason = message.reason.clone();
        if message.kind == Kind::Send {
            if let Ok(Some(receipt)) = clients.receipt(
                provider(&message.to.client),
                &message.to.session_id,
                &message.request_id,
            ) {
                state = delivered(receipt.state).to_owned();
                reason = receipt.reason.as_deref().map(plain);
            }
        }
        let mut answer = Value::Null;
        let mut ended_at = Value::Null;
        if !message.to.session_id.is_empty() && !matches!(state.as_str(), "refused") {
            if let Some(found) = transcript::answer(host, &message) {
                message.turn = Some(found.turn.clone());
                state = if found.done { "done" } else { "started" }.into();
                if found.done {
                    answer = found.answer.map(Value::String).unwrap_or(Value::Null);
                    ended_at = found.ended_at.unwrap_or(Value::Null);
                    reason = None;
                }
            }
        }
        // An answer thrown back earlier, which the session's later turn hides now.
        if answer.is_null() && !matches!(state.as_str(), "refused") {
            if let Some((text, ended)) = self.answers.get(&message.id) {
                state = "done".into();
                answer = Value::String(text.to_owned());
                ended_at = ended.cloned().unwrap_or(Value::Null);
                reason = None;
            }
        }
        // Answered with send_message: the reply is on its way as a message of
        // its own. Its words are not kept, so only its id is given.
        // A turn that ended with nothing to read says less than the reply.
        if let Some(reply) = message
            .replied_by
            .as_deref()
            .filter(|_| state != "done" || answer.is_null())
        {
            state = "replied".into();
            reason = Some(words::replied(language, reply));
        }
        if state == "taken" || state == "queued" {
            // A queued message whose session never took it within the plugin's hour.
            if Provider::parse(&message.to.client).is_some_and(Provider::queues_in_channel)
                && now - message.sent_at_ms > 3_600_000
            {
                state = "expired".into();
            }
        }
        if state != message.state || reason != message.reason || message.turn.is_some() {
            message.state = state.clone();
            message.reason = reason.clone();
            self.journal.replace(message.clone());
        }
        let to = find(host.snapshot, &message.to.session_id);
        Ok(json!({
            "id": message.id,
            "state": state,
            "reason": reason,
            "to": {"client": message.to.client, "sessionId": (!message.to.session_id.is_empty()).then_some(&message.to.session_id), "title": to.map(|session| session["title"].clone())},
            "kind": if message.kind == Kind::Start {"start"} else {"send"},
            "reply": message.replied_by,
            "answer": answer,
            "endedAt": ended_at,
            "sentAt": chrono::DateTime::<chrono::Utc>::from_timestamp_millis(message.sent_at_ms).map(|at| at.to_rfc3339()),
        }))
    }
}
