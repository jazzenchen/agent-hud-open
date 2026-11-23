//! Antigravity on Windows: messages and interrupts go to the running local
//! language server that opened the session
//! (`crate::clients::antigravity::service::message_session`).
use super::cascade_config::{choose, latest_turn_config};
use super::*;

pub(crate) const OPERATIONS: &[Operation] = &[
    Operation::Automatic,
    Operation::Start,
    Operation::Steer,
    Operation::Enqueue,
    Operation::Interrupt,
];
pub(crate) const NOTICE: &str = "Connects to the running Antigravity local language server for existing sessions and new-session starts.";

pub(super) fn interrupt(cascade_id: &str) -> Result<()> {
    use crate::clients::antigravity::service::message_session;
    let session = message_session(cascade_id).map_err(windows_message_error)?;
    session.interrupt().map_err(windows_message_error)?;
    Ok(())
}

/// Read a model configuration from the same verified owner that will receive
/// the message, and never retry an uncertain write on another service. A
/// configuration that cannot be read never stops the message: the note says
/// why it went in the default model.
pub(super) fn send(
    cascade_id: &str,
    text: &str,
    strategy: &str,
    model: Option<&str>,
) -> Result<Option<String>> {
    use crate::clients::antigravity::service::message_session;
    let session = message_session(cascade_id).map_err(windows_message_error)?;
    let (config, note) = choose(
        latest_turn_config(cascade_id, |method, body| session.read(method, &body)),
        model.map(str::to_owned).or_else(super::picker::picked),
        || session.user_status().ok(),
    );
    session
        .send(&message_body(cascade_id, text, strategy, config))
        .map_err(windows_message_error)?;
    Ok(note)
}

fn windows_message_error(error: crate::clients::antigravity::service::MessageError) -> Failure {
    use crate::clients::antigravity::service::MessageError;
    match error {
        MessageError::Unavailable(reason) => Failure::Unavailable(reason),
        MessageError::Invalid(reason) => Failure::InvalidInput(reason),
        MessageError::Uncertain => Failure::AcceptanceUncertain,
    }
}
