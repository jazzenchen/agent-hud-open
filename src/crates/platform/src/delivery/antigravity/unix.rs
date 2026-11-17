//! Antigravity on macOS and Linux: a message or an interrupt goes straight
//! to the running language server.
use super::*;

pub(crate) const OPERATIONS: &[Operation] = &[
    Operation::Automatic,
    Operation::Start,
    Operation::Steer,
    Operation::Enqueue,
    Operation::Interrupt,
];
pub(crate) const NOTICE: &str = "Connects to the running Antigravity local language server.";

pub(super) fn interrupt(cascade_id: &str) -> Result<()> {
    call(
        "CancelCascadeInvocation",
        json!({ "cascadeId": cascade_id }),
    )?;
    Ok(())
}

/// Sends a message in the configuration of the cascade's latest turn; the
/// note says why when it went in the default model instead.
pub(super) fn send(
    cascade_id: &str,
    text: &str,
    strategy: &str,
    model: Option<&str>,
) -> Result<Option<String>> {
    let (config, note) = resolve_cascade_config(Some(cascade_id), model);
    call(
        "SendUserCascadeMessage",
        message_body(cascade_id, text, strategy, config),
    )?;
    Ok(note)
}
