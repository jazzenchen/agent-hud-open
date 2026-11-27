//! Codex on macOS and Linux: the desktop app's owner socket first, then the
//! shared CLI daemon's control socket (`app-server-control.sock`), which only
//! these hosts have.
use super::desktop::{base_receipt, deliver as desktop};
use super::*;
use daemon::Cli;
use serde_json::{json, Value};
use std::path::Path;

mod daemon;
mod tui;

/// Agent HUD sends to Codex on this host.
pub(crate) const SUPPORTED: bool = true;
/// A new thread can start in a named model: the shared CLI daemon runs its
/// first turn in it (`start_in_daemon`), when no desktop app takes it.
pub(crate) const MODEL_AT_START: bool = true;
/// What a send may ask for: start, steer and interrupt through the desktop
/// app or the CLI daemon, and Codex's queue.
pub(crate) const OPERATIONS: &[Operation] = &[
    Operation::Automatic,
    Operation::Start,
    Operation::Steer,
    Operation::Enqueue,
    Operation::Interrupt,
];

/// Whether the desktop app's owner socket or the shared CLI daemon answers a
/// connection, closed at once. A socket file left behind by an app that no
/// longer listens on it refuses the connection (#178).
pub(super) fn steer_owner(data: &Path) -> bool {
    use crate::clients::codex::desktop_io::{endpoint, LocalSocket};
    let quick = Duration::from_millis(300);
    LocalSocket::connect(&endpoint(data), quick).is_ok()
        || LocalSocket::connect(
            &data.join("app-server-control/app-server-control.sock"),
            quick,
        )
        .is_ok()
}

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
        .ok_or_else(|| Failure::Unavailable("Known target has no Codex thread identity".into()))?;
    if !valid_uuid(thread) {
        return Err(Failure::InvalidInput("Codex thread identity".into()));
    }
    if request.operation == Operation::Enqueue {
        return enqueue(data, target, request, thread, timeout, queue);
    }
    // Only a proven absence before mutation permits another route. A timeout,
    // malformed owner reply, or uncertain acceptance must never fall through.
    let direct = desktop(data, target, request, thread, timeout).and_then(|receipt| {
        if let Some(receipt) = receipt {
            Ok(receipt)
        } else {
            tui::cli(data, target, request, thread, timeout)
        }
    });
    match direct {
        Err(Failure::Unavailable(_))
            if matches!(request.operation, Operation::Automatic | Operation::Start) =>
        {
            enqueue(data, target, request, thread, timeout, queue)
        }
        result => result,
    }
}
/// Codex's queue takes the input without loading the thread: whichever app
/// has the thread open sends it as a new turn once the thread is idle, after
/// a turn in progress. It goes through the shared daemon when one runs, else
/// through a short-lived app-server when the host gave a route. A thread no
/// app has open is woken in the Codex app.
fn enqueue(
    data: &Path,
    target: &SessionTarget,
    request: &SendRequest,
    thread: &str,
    timeout: Duration,
    queue: Option<&QueueRoute>,
) -> Result<Receipt> {
    let params = json!({"threadId":thread,"clientUserMessageId":format!("agent-hud:{}", request.request_id),"input":[{"type":"text","text":request.text,"text_elements":[]}]});
    let accepted = |value: Value| {
        if value["queuedSubmission"].is_object() {
            Ok(())
        } else {
            Err(Failure::AcceptanceUncertain)
        }
    };
    match queue_in_daemon(data, params.clone(), timeout) {
        Ok(value) => accepted(value)?,
        Err(Failure::Unavailable(_)) if queue.is_some() => {
            let route = queue.unwrap();
            let mut server =
                process::StdioServer::open(&route.executable, data, &route.environment, timeout)?;
            let value = server.mutation("thread/queue/add", params)?;
            server.finish();
            accepted(value)?;
        }
        Err(Failure::Unavailable(_)) => {
            return Err(Failure::Unavailable(
                "No Codex command line or shared daemon to queue through".into(),
            ))
        }
        Err(error) => return Err(error),
    }
    let mut receipt = base_receipt(target, request, DeliveryState::Queued, None, None);
    let applications = queue
        .map(|route| route.applications.as_slice())
        .unwrap_or(&[]);
    receipt.reason = super::queued_reason(data, thread, applications);
    Ok(receipt)
}

/// Starts a thread in the shared CLI daemon, for a computer without the Codex
/// desktop app.
pub(crate) fn start_in_daemon(
    data: &Path,
    folder: &Path,
    request: &StartRequest,
    message_id: &str,
    timeout: Duration,
) -> Option<Result<StartReceipt>> {
    Some(create_cli(data, folder, request, message_id, timeout))
}
fn create_cli(
    data: &Path,
    folder: &Path,
    request: &StartRequest,
    message_id: &str,
    timeout: Duration,
) -> Result<StartReceipt> {
    let mut connection = Cli::connect(
        &data.join("app-server-control/app-server-control.sock"),
        timeout,
    )?;
    let value = connection.request("thread/start", super::thread_start(folder, request), true)?;
    let thread = value["thread"]["id"]
        .as_str()
        .filter(|id| valid_uuid(id))
        .ok_or(Failure::AcceptanceUncertain)?
        .to_string();
    let session = value["thread"]["sessionId"]
        .as_str()
        .filter(|id| valid_uuid(id))
        .ok_or(Failure::AcceptanceUncertain)?
        .to_string();
    let mut turn = json!({"threadId":thread,"clientUserMessageId":message_id,"input":[{"type":"text","text":request.text,"text_elements":[]}]});
    if let Some(model) = &request.model {
        turn["model"] = json!(model);
    }
    if let Some(effort) = &request.reasoning_effort {
        turn["effort"] = json!(effort);
    }
    let first = connection.request("turn/start", turn, true);
    // Creating again would duplicate the session even when its first turn failed.
    let reason = match first {
        Ok(value)
            if value["turn"]["id"]
                .as_str()
                .is_some_and(|id| !id.is_empty()) =>
        {
            None
        }
        Ok(_) => Some("The thread exists but its first turn acknowledgement was invalid".into()),
        Err(error) => Some(format!(
            "The thread exists; its first turn did not confirm: {error}"
        )),
    };
    Ok(StartReceipt {
        provider: Provider::Codex,
        request_id: request.request_id.clone(),
        session_id: Some(session),
        thread_id: Some(thread),
        state: if request.reasoning_effort.is_some() && reason.is_some() {
            "uncertain"
        } else {
            "started"
        }
        .into(),
        reason,
        job_id: None,
        handoff_pending: false,
    })
}

/// Queues a new thread's first message: through the shared daemon when one
/// runs, else through a short-lived app-server.
pub(crate) fn queue_created(
    route: &QueueRoute,
    data: &Path,
    thread: &str,
    text: &str,
    message_id: &str,
    timeout: Duration,
) -> Result<()> {
    match enqueue_created(data, thread, text, message_id, timeout) {
        Err(Failure::Unavailable(_)) => {
            super::enqueue_stdio(route, data, thread, text, message_id, timeout)
        }
        result => result,
    }
}
fn enqueue_created(
    data: &Path,
    thread: &str,
    text: &str,
    message_id: &str,
    timeout: Duration,
) -> Result<()> {
    let value = queue_in_daemon(
        data,
        json!({"threadId":thread,"clientUserMessageId":message_id,"input":[{"type":"text","text":text,"text_elements":[]}]}),
        timeout,
    )?;
    if value["queuedSubmission"].is_object() {
        Ok(())
    } else {
        Err(Failure::AcceptanceUncertain)
    }
}

/// Adds to Codex's queue through the shared daemon. The daemon runs a Codex
/// of its own (`codex app-server daemon version` names it), and a build
/// without the queue refuses the method by name before queueing anything
/// (Open #94); such a daemon counts as none, so the message takes the
/// short-lived app-server instead.
fn queue_in_daemon(data: &Path, params: Value, timeout: Duration) -> Result<Value> {
    let mut daemon = Cli::connect(
        &data.join("app-server-control/app-server-control.sock"),
        timeout,
    )?;
    match daemon.request("thread/queue/add", params, true) {
        Err(Failure::Rejected(reason))
            if reason.contains("thread/queue/add")
                && (reason.contains("unknown variant") || reason.contains("not support")) =>
        {
            Err(Failure::Unavailable(reason))
        }
        result => result,
    }
}
