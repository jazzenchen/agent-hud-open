//! What was sent, kept a week: who to whom, the chain and pass, where the
//! answer is. Never the words: only their fingerprint.
use agent_hud_platform::delivery::{self, PromptForm};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const KEPT_MS: i64 = 7 * 86_400_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Send,
    Start,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Party {
    pub client: String,
    /// Empty for a session a start has not named yet.
    pub session_id: String,
    #[serde(default)]
    pub title: Option<String>,
}
/// The turn a message began in its recipient, and the transcript page it was
/// found on, where the next read starts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub id: Option<String>,
    pub cursor: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub kind: Kind,
    /// The first message's id: a chain is passes with no word from the user.
    pub chain: String,
    /// This message's pass in its chain, from 1.
    pub count: u32,
    pub from: Party,
    pub to: Party,
    pub form: PromptForm,
    #[serde(default)]
    pub reply_to: Option<String>,
    /// The local clients' request identity: the message id for a send, the
    /// start's own for a start.
    pub request_id: String,
    pub sent_at_ms: i64,
    /// Where the recipient's transcript ended when it was sent.
    #[serde(default)]
    pub cursor: Option<u64>,
    /// SHA-256 of the words delivered, trimmed: recognises them in the
    /// recipient's transcript when they carry no label.
    pub fingerprint: String,
    pub state: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub turn: Option<Turn>,
    /// The plane was thrown back for its answer (or it is too old to be, or
    /// its turn ended with nothing to read).
    #[serde(default)]
    pub answered: bool,
    /// The message its recipient answered it with (send_message reply_to):
    /// that message's id, never its words.
    #[serde(default)]
    pub replied_by: Option<String>,
    /// The turn in progress a steer went into (`delivery: steer`). It began
    /// before the send, so its answer is looked for there, not in a turn
    /// that began after.
    #[serde(default)]
    pub steered_into: Option<String>,
    /// A steer left for the turn's own hooks, which that turn reads itself
    /// (`read_message`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub steer_waits: bool,
    /// The relay's own name for the tool call that sent it, when it gave
    /// one: a caller whose answer was lost asks what became of the call by
    /// it, and a call that comes twice sends once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<String>,
    /// Its words ended with the reply line: the sender waits for an answer,
    /// and Agent HUD follows up when none comes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub asked: bool,
    /// When its own turn ended with no reply, while a follow-up may be due.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at_ms: Option<i64>,
    /// The check steered into its turn while that turn still ran was sent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub checked: bool,
    /// The follow-up queued after its turn ended with no reply was sent, or
    /// is no longer owed: that end is too long ago, or its client was off.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub followed: bool,
    /// On a follow-up: the message it follows up. Agent HUD sent it on the
    /// sender's behalf; it is no pass of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follows: Option<String>,
}
impl Message {
    /// Its turn ended with no reply and the follow-up is still to send.
    pub fn follow_up_due(&self) -> bool {
        self.asked && !self.followed && self.replied_by.is_none() && self.ended_at_ms.is_some()
    }
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    messages: Vec<Message>,
}

pub struct Journal {
    path: PathBuf,
    /// Oldest first.
    messages: Vec<Message>,
}
impl Journal {
    pub fn load(path: PathBuf) -> Self {
        let messages = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<File>(&bytes).ok())
            .unwrap_or_default()
            .messages;
        Self { path, messages }
    }
    pub fn save(&self) {
        let _ = delivery::atomic_json(
            &self.path,
            &File {
                messages: self.messages.clone(),
            },
        );
    }
    pub fn add(&mut self, message: Message) {
        self.messages.push(message);
        self.save();
    }
    pub fn find(&self, id: &str) -> Option<&Message> {
        self.messages.iter().find(|message| message.id == id)
    }
    /// Every message kept, oldest first.
    pub fn messages(&self) -> impl DoubleEndedIterator<Item = &Message> {
        self.messages.iter()
    }
    /// The message the relay's call `call` sent.
    pub fn by_call(&self, call: &str) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|message| message.call.as_deref() == Some(call))
    }
    /// What `session` was sent and did not refuse, newest first.
    pub fn received<'a>(
        &'a self,
        client: &'a str,
        session: &'a str,
    ) -> impl Iterator<Item = &'a Message> + 'a {
        self.messages.iter().rev().filter(move |message| {
            message.to.client == client
                && super::same_session(&message.to.session_id, session)
                && message.state != "refused"
        })
    }
    /// Messages not yet answered nor ended without one, or owed a follow-up.
    pub fn pending(&self) -> bool {
        self.messages.iter().any(|message| {
            (!message.answered || message.follow_up_due())
                && !matches!(message.state.as_str(), "refused" | "expired" | "uncertain")
        })
    }
    /// The ids of replies: messages that went back to the sender of the
    /// message they answer (send_message with reply_to).
    pub fn replies(&self) -> std::collections::BTreeSet<String> {
        self.messages
            .iter()
            .filter(|message| {
                message
                    .reply_to
                    .as_deref()
                    .and_then(|parent| self.find(parent))
                    .is_some_and(|parent| {
                        super::same_identity(
                            (&parent.from.client, &parent.from.session_id),
                            (&message.to.client, &message.to.session_id),
                        )
                    })
            })
            .map(|message| message.id.clone())
            .collect()
    }
    pub fn messages_mut(&mut self) -> impl Iterator<Item = &mut Message> {
        self.messages.iter_mut()
    }
    /// Forgets messages older than a week; true when any went.
    pub fn prune(&mut self, now: i64) -> bool {
        let before = self.messages.len();
        self.messages
            .retain(|message| message.sent_at_ms >= now - KEPT_MS);
        self.messages.len() != before
    }
}
