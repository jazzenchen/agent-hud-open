//! Steering: a message that goes into the recipient's turn in progress
//! (`delivery: steer`), what its recipient reads and why one was refused.
use agent_hud_core::l10n::{fill, tr_in, Language};
use agent_hud_platform::delivery::PromptForm;

/// The words delivered, marked on their first line as going into the turn
/// in progress, so the recipient reads them as a correction to the work at
/// hand and not as a task of its own. As the user's own, the text stays bare.
pub fn steered(language: Language, form: PromptForm, delivered: String) -> String {
    if form == PromptForm::Own {
        return delivered;
    }
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
/// list_sessions: an idle Codex session, or one no Codex has open.
pub fn steer_idle(language: Language) -> String {
    tr_in(
        language,
        "No turn is in progress in an open Codex; a steer would be refused.",
    )
}
