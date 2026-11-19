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
