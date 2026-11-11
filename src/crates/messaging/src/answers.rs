//! Answers read when their plane was thrown back, kept in memory only: the
//! journal keeps no words. A client Agent HUD reads only through its latest
//! turn (`liveFacts`: TRAE, Cursor, ZCode, Pi, OpenCode) shows a message's
//! answer only until its next turn ends; `get_message` then gives the one
//! kept here.
use serde_json::Value;
use std::collections::VecDeque;

/// Answers kept, the newest.
const KEPT: usize = 64;

#[derive(Default)]
pub(crate) struct Answers(VecDeque<(String, String, Option<Value>)>);

impl Answers {
    pub(crate) fn keep(&mut self, message: &str, answer: &str, ended_at: Option<Value>) {
        self.0.retain(|(id, ..)| id != message);
        self.0
            .push_back((message.to_owned(), answer.to_owned(), ended_at));
        while self.0.len() > KEPT {
            self.0.pop_front();
        }
    }

    /// The answer to `message` and when its turn ended.
    pub(crate) fn get(&self, message: &str) -> Option<(&str, Option<&Value>)> {
        self.0
            .iter()
            .find(|(id, ..)| id == message)
            .map(|(_, answer, ended_at)| (answer.as_str(), ended_at.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_newest_answers_are_kept() {
        let mut answers = Answers::default();
        answers.keep("m-1", "first", Some(json!("2026-11-12T00:00:00Z")));
        answers.keep("m-1", "again", None);
        assert_eq!(answers.get("m-1"), Some(("again", None)));
        for n in 0..KEPT {
            answers.keep(&format!("m-x{n}"), "x", None);
        }
        assert_eq!(answers.get("m-1"), None);
        assert_eq!(answers.get("m-x0"), Some(("x", None)));
    }
}
