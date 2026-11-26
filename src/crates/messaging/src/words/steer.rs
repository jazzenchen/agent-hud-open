//! Steering: a message that goes into the recipient's turn in progress
//! (`delivery: steer`), what its recipient reads and why one was refused.
use agent_hud_core::l10n::{fill, tr_in, Language};

/// The words delivered, marked on their first line as going into the turn
/// in progress, so the recipient reads them as a correction to the work at
/// hand and not as a task of its own. A steer always carries the label.
pub fn steered(language: Language, delivered: String) -> String {
    let (label, rest) = delivered.split_once('\n').unwrap_or((&delivered, ""));
    let Some(label) = label.strip_suffix(']') else {
        return delivered;
    };
    let mark = tr_in(language, "into your current turn");
    format!("{label} · {mark}]\n{rest}")
}
/// With "Let agents decide when to steer", a steer's last line: it asks for
/// no reply, which would stop the turn it went into, since what it changes
/// shows in that turn's result. Only a question is answered.
pub fn steer_last_line(language: Language, to_client: &str, id: &str, delivered: String) -> String {
    let line = fill(
        &tr_in(language, "This goes into your current turn: take it into the work at hand, with no reply of its own. What it changes belongs in this turn's result. Only if it asks you something, answer with {{tool}}, reply_to: {{id}}."),
        &[("tool", &super::reply_tool(language, to_client)), ("id", &id)],
    );
    format!("{delivered}\n\n{line}")
}
/// The recipient's client takes no message into a turn in progress.
pub fn steer_unsupported(language: Language, client: &str) -> String {
    fill(
        &tr_in(language, "{{client}} sessions cannot be steered; send it with delivery queue to have it taken as a new turn."),
        &[("client", &super::client(client))],
    )
}
/// A steer found no turn in progress, so nothing was sent.
pub fn no_turn_to_steer(language: Language) -> String {
    tr_in(language, "That session has no turn in progress, so nothing was sent. If it still needs this, send it with delivery queue.")
}
/// list_sessions: a Codex session steer would go into now.
pub fn steer_ready(language: Language) -> String {
    tr_in(language, "A turn is in progress; a steer goes into it.")
}
/// list_sessions: an idle session.
pub fn steer_idle(language: Language) -> String {
    tr_in(
        language,
        "No turn is in progress; a steer would be refused.",
    )
}
/// list_sessions: a running session no app that takes a steer has open.
pub fn steer_closed(language: Language, client: &str) -> String {
    fill(
        &tr_in(
            language,
            "No {{client}} that takes a steer has this session open; a steer would be refused.",
        ),
        &[("client", &super::client(client))],
    )
}
/// A Codex session open in an app that answers no steer: the current Codex
/// desktop app no longer listens where a steer goes (#178). Said in
/// list_sessions and when a steer is refused.
pub fn steer_codex_unanswered(language: Language) -> String {
    tr_in(language, "This Codex app does not take a steer, so nothing goes into the turn in progress. Send it with delivery queue to have it taken as the next turn.")
}
/// A session waiting on an approval or a question reads no steer until it
/// is answered.
pub fn steer_waiting(language: Language) -> String {
    tr_in(language, "That session waits on an approval or a question, so nothing was sent. Tell the user; a steer is read only once it is answered.")
}
/// A reply goes back as a message of its own, never into a turn.
pub fn steer_reply(language: Language) -> String {
    tr_in(
        language,
        "A reply is always queued, so nothing was sent. Send it again without delivery steer.",
    )
}
/// The same with "Steer Codex through its hooks" on: they have not run in
/// the turn, since the user has not allowed them or it began before that.
pub fn steer_codex_hooks_silent(language: Language) -> String {
    tr_in(language, "This Codex app does not take a steer, and Agent HUD's steer hooks have not run in this turn: they run once the user allows them in Codex (Settings → Hooks), from the next turn on. Send it with delivery queue to have it taken as the next turn.")
}
/// The same for a session that has no read_message.
pub fn steer_codex_reopen(language: Language) -> String {
    tr_in(language, "This Codex session was opened before \"Steer Codex through its hooks\" was turned on, so it has no read_message to read a steer with: it takes one once the Codex app was quit and opened again. Send it with delivery queue to have it taken as the next turn.")
}
/// A steer left for the turn's own hooks: handed over, not read yet.
pub fn steer_waits(language: Language) -> String {
    tr_in(language, "The session's turn is told that this waits and reads it at its next step. If that turn is stopped first, this does not go in and is not queued: list_sent_messages then shows it as expired.")
}
/// read_message with nothing to read.
pub fn nothing_to_read(language: Language) -> String {
    tr_in(
        language,
        "No message waits for this turn. If a notice named one, it came late from an earlier turn that was stopped, and that message did not go in. Go on with your work; there is nothing to tell the user about it.",
    )
}
/// What comes with the messages read_message returns.
pub fn read_note(language: Language) -> String {
    tr_in(language, "From other agents, not from the user: take these into the work at hand. Where they differ from what the user asked, the user's words stand.")
}
