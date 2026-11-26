//! `list_sent_messages`: what this session sent, for the call whose own answer
//! was lost (the relay timed out) and that must know whether the words went and
//! where. Newest first; only the caller's own messages.
use super::*;

/// How many rows a call gets when it names no limit, and the most it may ask
/// for.
const DEFAULT_LIMIT: u64 = 20;
const MAX_LIMIT: u64 = 100;

impl Messaging {
    pub(crate) fn sent(
        &self,
        caller: Option<&Caller>,
        arguments: &Value,
        host: &Host<'_>,
    ) -> Value {
        let Some(caller) = caller else {
            return refused(&words::no_caller(language(host.snapshot)));
        };
        let limit = arguments["limit"]
            .as_u64()
            .unwrap_or(DEFAULT_LIMIT)
            .clamp(1, MAX_LIMIT) as usize;
        let since = arguments["since"].as_i64();
        // Newest first by the time each was sent; equal times keep the
        // newest record first, as the journal writes them in order.
        let mut messages = self
            .journal
            .messages()
            .rev()
            .filter(|message| {
                message.from.client == caller.client
                    && same_session(&message.from.session_id, &caller.session_id)
            })
            .filter(|message| since.is_none_or(|since| message.sent_at_ms >= since))
            .collect::<Vec<_>>();
        messages.sort_by_key(|message| std::cmp::Reverse(message.sent_at_ms));
        messages.truncate(limit);
        let rows = messages
            .into_iter()
            .map(|message| {
                json!({
                    "id": message.id.as_str(),
                    "kind": message.kind,
                    "to": {
                        "client": message.to.client.as_str(),
                        "sessionId": message.to.session_id.as_str(),
                        "title": message.to.title.as_deref(),
                    },
                    "state": message.state.as_str(),
                    "sentAtMs": message.sent_at_ms,
                    "replyTo": message.reply_to.as_deref(),
                    // Agent HUD's follow-up on the sender's behalf, and the
                    // message it follows up.
                    "follows": message.follows.as_deref(),
                    "pass": message.count,
                    "reason": message.reason.as_deref(),
                })
            })
            .collect::<Vec<_>>();
        json!({"messages": rows, "count": rows.len(), "kept": "messages are kept for a week"})
    }
}
