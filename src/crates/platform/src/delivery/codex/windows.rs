//! Codex on Windows: the desktop app's owner pipe, and Codex's queue through
//! a short-lived app-server. Windows has no shared CLI daemon, so a thread
//! only a terminal CLI holds takes no message here, and nothing is
//! interrupted.
use super::*;

/// Agent HUD sends to Codex on this host.
pub(crate) const SUPPORTED: bool = true;
/// What a send may ask for: start and steer through the desktop app, and
/// Codex's queue. Interrupt is not offered.
pub(crate) const OPERATIONS: &[Operation] = &[
    Operation::Automatic,
    Operation::Start,
    Operation::Steer,
    Operation::Enqueue,
];

pub(super) fn deliver(
    data: &Path,
    target: &SessionTarget,
    request: &SendRequest,
    timeout: Duration,
    queue: Option<&QueueRoute>,
) -> Result<Receipt> {
    let thread = target
        .thread_id
        .as_deref()
        .filter(|thread| valid_uuid(thread))
        .ok_or_else(|| Failure::InvalidInput("Codex thread identity".into()))?;
    if request.operation != Operation::Enqueue {
        return super::desktop::deliver(data, target, request, thread, timeout)?.ok_or_else(|| {
            Failure::Unavailable(
                "No active Codex Desktop owner; Windows CLI owner control is unavailable".into(),
            )
        });
    }
    let route = queue.ok_or_else(|| Failure::Unavailable("Codex executable".into()))?;
    let message_id = format!("agent-hud:{}", request.request_id);
    enqueue_stdio(route, data, thread, &request.text, &message_id, timeout)?;
    Ok(Receipt {
        provider: Provider::Codex,
        session_id: target.session_id.clone(),
        thread_id: target.thread_id.clone(),
        request_id: request.request_id.clone(),
        client_request_id: message_id,
        turn_id: None,
        owner_client_id: None,
        state: DeliveryState::Queued,
        at_ms: millis(SystemTime::now()),
        reason: Some("Queued until Codex opens the thread".into()),
    })
}

/// Windows has no shared CLI daemon to start a thread in: the caller starts
/// it through a short-lived app-server, with or without the desktop app.
pub(crate) fn start_in_daemon(
    _data: &Path,
    _folder: &Path,
    _request: &StartRequest,
    _message_id: &str,
    _timeout: Duration,
) -> Option<Result<StartReceipt>> {
    None
}

/// Queues a new thread's first message through a short-lived app-server.
pub(crate) fn queue_created(
    route: &QueueRoute,
    data: &Path,
    thread: &str,
    text: &str,
    message_id: &str,
    timeout: Duration,
) -> Result<()> {
    enqueue_stdio(route, data, thread, text, message_id, timeout)
}
