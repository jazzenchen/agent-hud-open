//! What a session is doing between its turns' ends: its open calls, the work
//! they do, and the question the Claude desktop app asks.
use super::live::newer;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Work {
    Shell,
    Command,
    Edit,
    Read,
    Agents,
    Question,
}

impl Work {
    pub(super) fn shown(self) -> bool {
        !matches!(self, Self::Agents | Self::Question)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Call {
    pub id: String,
    pub work: Option<Work>,
    pub at: i64,
    pub request: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopAskOption {
    pub label: String,
    pub message: Option<String>,
}

/// A question the Claude desktop app asks in its own window. Its words, the
/// recap and the options are kept only while the app runs, as a turn's
/// message is: a checkpoint keeps where the call's line starts, and a later
/// run reads the words again from that one line (agent-hud-native-open
/// SessionActivity.Ask, ClaudeTranscriptStore.reworded).
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopAsk {
    pub id: String,
    pub response: Option<String>,
    pub kind: String,
    #[serde(skip)]
    pub question: String,
    #[serde(skip)]
    pub recap: Option<String>,
    #[serde(skip)]
    pub options: Vec<DesktopAskOption>,
    pub at: i64,
    /// Where the line of the call that asks it starts in its log.
    #[serde(default)]
    pub offset: Option<u64>,
}

impl DesktopAsk {
    /// Whether its words were read in this run.
    pub fn is_worded(&self) -> bool {
        !self.question.is_empty()
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Activity {
    pub calls: Vec<Call>,
    pub work: Option<Work>,
    pub tool_at: Option<i64>,
    pub line: Option<String>,
    pub retrying: bool,
    /// Only the desktop's explicit next-step question can survive a successful result.
    pub standing_question: Option<Call>,
    #[serde(default)]
    pub desktop_ask: Option<DesktopAsk>,
    pub latest: Option<Work>,
}

impl Activity {
    pub fn start(&mut self) {
        *self = Self::default();
        self.line = Some("prompt".into());
    }
    pub fn end(&mut self) {
        self.calls.clear();
        self.latest = None;
    }
    pub fn open(&mut self, call: Call) {
        self.calls.retain(|existing| existing.id != call.id);
        newer(&mut self.tool_at, call.at);
        if call.work.is_some_and(Work::shown) {
            self.latest = call.work;
        }
        self.calls.push(call);
        self.line = Some("call".into());
    }
    pub fn close(&mut self, id: &str, at: i64, failed: bool) {
        self.calls.retain(|call| call.id != id);
        if failed
            && self
                .standing_question
                .as_ref()
                .is_some_and(|ask| ask.id == id)
        {
            self.standing_question = None;
        }
        if self
            .desktop_ask
            .as_ref()
            .is_some_and(|ask| ask.id == id && (ask.kind == "offer" || failed))
        {
            self.desktop_ask = None;
        }
        newer(&mut self.tool_at, at);
        self.line = Some("result".into());
    }
    pub fn settle(&mut self) {
        if let Some(call) = self
            .calls
            .iter()
            .rev()
            .find(|call| call.work.is_some_and(Work::shown))
        {
            self.work = call.work;
        } else if self.latest.is_some() {
            self.work = self.latest;
        }
        self.latest = None;
    }
    pub(super) fn awaits_model(&self) -> bool {
        !matches!(self.line.as_deref(), Some("call" | "answer" | "error"))
    }
}
