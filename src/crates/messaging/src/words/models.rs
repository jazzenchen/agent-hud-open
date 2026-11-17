//! Naming the model a session runs (`list_models`, and `model` on a send or
//! a start): why a named one was refused.
use agent_hud_core::l10n::{fill, tr_in, Language};

/// A message into a session of this client cannot switch its model.
pub fn model_per_message(language: Language, client: &str) -> String {
    fill(
        &tr_in(language, "A message to a {{client}} session cannot choose its model; the session goes on in its own. Leave model out, or start a new session in that model with start_session."),
        &[("client", &super::client(client))],
    )
}
/// A new session of this client cannot start in a named model here: its
/// desktop app runs the first message in the model chosen there (Codex).
pub fn model_at_start(language: Language, client: &str) -> String {
    fill(
        &tr_in(language, "{{client}}'s desktop app runs a new session's first message in the model chosen in the app, so a start here cannot name one. Leave model out."),
        &[("client", &super::client(client))],
    )
}
/// The client offers no model by that name.
pub fn unknown_model(language: Language, client: &str, model: &str) -> String {
    fill(
        &tr_in(
            language,
            "{{client}} offers no model named {{model}}; list_models lists the ones it does.",
        ),
        &[("client", &super::client(client)), ("model", &model)],
    )
}
/// The client's models could not be read, so a named one cannot be checked.
pub fn models_unread(language: Language, client: &str, reason: &str) -> String {
    fill(
        &tr_in(
            language,
            "Agent HUD could not read the models {{client}} offers: {{reason}}",
        ),
        &[("client", &super::client(client)), ("reason", &reason)],
    )
}
