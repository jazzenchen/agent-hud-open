//! How words reach one of a client's sessions as its user's next message, as
//! a click on the client's own question sends them. The host carries the
//! words (core's `SessionReplies`); core decides which questions take them.

pub struct Replies {
    /// What carries the words into the session.
    pub carrier: &'static str,
    /// The setting that turns answering the client's questions here on and
    /// off; on unless it is `false`.
    pub setting: &'static str,
}

/// Why words would not reach a session now: the reason, in the app's
/// language, and the step that would let Agent HUD answer there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unreachable {
    pub note: String,
    /// None when nothing on this Mac would change it, such as a button that
    /// works only in the client's own window.
    pub fix: Option<AnswerFix>,
}

impl Unreachable {
    pub fn new(note: String, fix: Option<AnswerFix>) -> Self {
        Self { note, fix }
    }
}

/// The step that lets Agent HUD answer a client's questions, as a card names
/// it (`answerFix`) for the HUD and the phone to word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnswerFix {
    /// The client's Agent HUD plugin is not installed: Settings → Agents.
    InstallPlugin,
    /// Installed, but this session has not loaded it: `/reload-plugins` there.
    ReloadPlugins,
    /// The user turned answering off in Settings → Agents.
    TurnOnAnswers,
}

impl AnswerFix {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InstallPlugin => "installPlugin",
            Self::ReloadPlugins => "reloadPlugins",
            Self::TurnOnAnswers => "turnOnAnswers",
        }
    }
}
