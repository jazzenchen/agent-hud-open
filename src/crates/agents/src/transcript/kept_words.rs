//! What a checkpoint of a log's live state keeps: the head presentation
//! needs, and the words it leaves out that this run holds in memory.
use super::live::LiveLog;

impl LiveLog {
    /// Presentation needs only the latest turn and current activity. Parser-only
    /// ownership and completed-response history stay in the full checkpoint.
    pub fn head(&self) -> Self {
        let mut head = self.clone();
        head.turns = self.turns.back().cloned().into_iter().collect();
        head.completed_responses.clear();
        head.owned_claude_id = None;
        head.inherited_through = None;
        head
    }

    /// Takes back the words `earlier`, this log as this run last held it,
    /// kept in memory, where a checkpoint read back left them out: each
    /// turn's message, the running turn's final answer and the desktop ask's
    /// words, each only for the same turn or ask. Whether any came back.
    pub fn keep_words(&mut self, earlier: &LiveLog) -> bool {
        let mut kept = false;
        for turn in self.turns.iter_mut().filter(|turn| turn.message.is_none()) {
            if let Some(message) = earlier
                .turns
                .iter()
                .rev()
                .find(|old| old.id == turn.id && old.started == turn.started)
                .and_then(|old| old.message.clone())
            {
                turn.message = Some(message);
                kept = true;
            }
        }
        let same_turn =
            |log: &LiveLog| log.turns.back().map(|turn| (turn.id.clone(), turn.started));
        if self.final_answer.is_none()
            && earlier.final_answer.is_some()
            && same_turn(self) == same_turn(earlier)
        {
            self.final_answer = earlier.final_answer.clone();
            kept = true;
        }
        if let (Some(ask), Some(old)) = (
            self.activity
                .as_mut()
                .and_then(|activity| activity.desktop_ask.as_mut()),
            earlier
                .activity
                .as_ref()
                .and_then(|activity| activity.desktop_ask.as_ref()),
        ) {
            if ask.id == old.id && !ask.is_worded() && old.is_worded() {
                ask.question = old.question.clone();
                ask.recap = old.recap.clone();
                ask.options = old.options.clone();
                kept = true;
            }
        }
        kept
    }

    /// Feeds the words a checkpoint leaves out to `state`, so a change of
    /// words alone is a change of the session's facts.
    pub fn hash_words(&self, state: &mut impl std::hash::Hasher) {
        use std::hash::Hash;
        self.turns
            .back()
            .and_then(|turn| turn.message.as_ref())
            .hash(state);
        self.final_answer.hash(state);
        if let Some(ask) = self
            .activity
            .as_ref()
            .and_then(|activity| activity.desktop_ask.as_ref())
        {
            ask.question.hash(state);
            ask.recap.hash(state);
            for option in &ask.options {
                option.label.hash(state);
                option.message.hash(state);
            }
        }
    }
}
