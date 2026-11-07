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

pub(super) fn send(cascade_id: &str, text: &str, strategy: &str) -> Result<()> {
    call(
        "SendUserCascadeMessage",
        message_body(cascade_id, text, strategy, true),
    )?;
    Ok(())
}
