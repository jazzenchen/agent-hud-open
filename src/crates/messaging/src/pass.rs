//! The chain and pass a message joins, and how it goes in: its form and
//! whether it asks for a reply.
use super::*;

impl Messaging {
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
    /// one (a steer) and one that asks because the sender passed
    /// `require_reply`: the recipient has to know whom it answers. "Ask for
    /// replies": Never asks nothing; When needed asks when the sender passes
    /// `require_reply`; Always asks with every labelled message except a reply,
    /// which asks only with `require_reply`: an answer ends the exchange, or
    /// two agents would answer each other until the pass limit. A message
    /// that goes as the user's own words asks nothing: it has no reply line.
    pub(crate) fn shape(&self, arguments: &Value, labelled: bool) -> (PromptForm, bool) {
        let required = arguments["require_reply"] == true;
        let ask = match self.preferences.replies {
            Replies::Never => false,
            Replies::WhenNeeded => required,
            Replies::Always => arguments["reply_to"].as_str().is_none() || required,
        };
        let own = self.preferences.as_user && !labelled && !(required && ask);
        if own {
            (PromptForm::Own, false)
        } else {
            (PromptForm::Labelled, ask)
        }
    }
}
