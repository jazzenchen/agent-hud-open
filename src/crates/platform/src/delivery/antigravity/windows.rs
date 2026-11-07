//! Antigravity on Windows: a message goes to the service that opened its
//! session (`crate::clients::antigravity::service::message_session`).
//! Interrupting is not adapted yet; an adapter would send
//! `CancelCascadeInvocation` to that same service, as `MessageSession::send`
//! does for a message.
use super::cascade_config::{default_cascade_config_from_status, trajectory_cascade_config};
use super::*;

/// What a message may ask for: no interrupt.
pub(crate) const OPERATIONS: &[Operation] = &[
    Operation::Automatic,
    Operation::Start,
    Operation::Steer,
    Operation::Enqueue,
];
pub(crate) const NOTICE: &str = "Sends to an existing uniquely identified session on its running Antigravity local language server. Interrupt and new-session start remain unavailable on Windows.";

/// Interrupting is not adapted on Windows.
pub(super) fn interrupt(_cascade_id: &str) -> Result<()> {
    Err(Failure::Unsupported(
        "Interrupting Antigravity sessions is unavailable on Windows".into(),
    ))
}

/// Read a model configuration from the same verified owner that will receive
/// the message, and never retry an uncertain write on another service.
pub(super) fn send(cascade_id: &str, text: &str, strategy: &str) -> Result<()> {
    use crate::clients::antigravity::service::message_session;
    let session = message_session(cascade_id).map_err(windows_message_error)?;
    let config = session
        .trajectory()
        .ok()
        .as_ref()
        .and_then(trajectory_cascade_config)
        .or_else(|| {
            session
                .user_status()
                .ok()
                .as_ref()
                .and_then(default_cascade_config_from_status)
        });
    session
        .send(&message_body_with_config(
            cascade_id, text, strategy, config,
        ))
        .map(|_| ())
        .map_err(windows_message_error)
}

fn windows_message_error(error: crate::clients::antigravity::service::MessageError) -> Failure {
    use crate::clients::antigravity::service::MessageError;
    match error {
        MessageError::Unavailable(reason) => Failure::Unavailable(reason),
        MessageError::Invalid(reason) => Failure::InvalidInput(reason),
        MessageError::Uncertain => Failure::AcceptanceUncertain,
    }
}
