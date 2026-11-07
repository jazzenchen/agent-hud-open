//! Codex Desktop owner operations, shared by its Unix socket and Windows pipe.
//! The wire methods are the installed desktop app's own; no app-server is
//! started or resumed to stand in for a conversation already running.
use super::*;
use crate::clients::codex::desktop_io::{DesktopIpc, IpcError};
use serde_json::{json, Value};
use std::path::Path;

impl From<IpcError> for Failure {
    fn from(value: IpcError) -> Self {
        match value {
            IpcError::Unavailable => Self::Unavailable("Native owner socket".into()),
            IpcError::TimedOut => Self::TimedOut,
            IpcError::Protocol => Self::Protocol("Desktop IPC".into()),
            IpcError::Rejected(reason) => Self::Rejected(reason),
        }
    }
}

pub(super) fn base_receipt(
    target: &SessionTarget,
    request: &SendRequest,
    state: DeliveryState,
    turn: Option<String>,
    owner: Option<String>,
) -> Receipt {
    Receipt {
        provider: Provider::Codex,
        session_id: target.session_id.clone(),
        thread_id: target.thread_id.clone(),
        request_id: request.request_id.clone(),
        client_request_id: format!("agent-hud:{}", request.request_id),
        turn_id: turn,
        owner_client_id: owner,
        state,
        at_ms: millis(SystemTime::now()),
        reason: None,
    }
}
pub(super) fn deliver(
    data: &Path,
    target: &SessionTarget,
    request: &SendRequest,
    thread: &str,
    timeout: Duration,
) -> Result<Option<Receipt>> {
    let mut connection =
        match DesktopIpc::connect(&crate::clients::codex::desktop_io::endpoint(data), timeout) {
            Ok(value) => value,
            Err(IpcError::Unavailable) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
    let initialized = connection.request(
        "initialize",
        0,
        "initializing-client",
        None,
        json!({"clientType":"agent-hud"}),
        None,
    )?;
    let client = initialized["result"]["clientId"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| Failure::Protocol("Desktop initialization identity".into()))?
        .to_string();
    let discovery = match connection.request(
        "thread-owner-discovery",
        1,
        &client,
        None,
        json!({"hostId":"local","conversationId":thread}),
        None,
    ) {
        Ok(value) => value,
        Err(IpcError::Rejected(reason)) if reason == "no-client-found" => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let owner = discovery["handledByClientId"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| Failure::Protocol("Desktop owner identity".into()))?
        .to_string();
    if request.operation == Operation::Interrupt {
        return Err(Failure::Unsupported("The original Desktop adapter exposes no verified interrupt operation; CLI fallback is forbidden while Desktop owns the thread".into()));
    }
    let selected = if request.operation == Operation::Automatic {
        let state = desktop_snapshot(&mut connection, &client, &owner, thread)?;
        if super::current_desktop_turn(&state)?.is_some_and(|(_, status)| status == "inProgress") {
            Operation::Steer
        } else {
            Operation::Start
        }
    } else {
        if request.operation == Operation::Steer {
            let state = desktop_snapshot(&mut connection, &client, &owner, thread)?;
            let current = super::current_desktop_turn(&state)?;
            if !current.is_some_and(|(turn, status)| {
                status == "inProgress" && request.expected_turn_id.as_deref() == Some(turn.as_str())
            }) {
                return Err(Failure::Rejected(
                    "Desktop no longer owns the expected active turn".into(),
                ));
            }
        }
        request.operation
    };
    connection.renew(timeout);
    let message_id = format!("agent-hud:{}", request.request_id);
    let input = json!([{"type":"text","text":request.text,"text_elements":[]}]);
    let (method, version, params) = match selected {
        Operation::Start => (
            "thread-follower-start-turn",
            2,
            json!({"conversationId":thread,"turnStart":{"request":{"threadId":thread,"clientUserMessageId":message_id,"input":input},"context":{"inheritThreadSettings":true}}}),
        ),
        Operation::Steer => (
            "thread-follower-steer-turn",
            1,
            json!({"conversationId":thread,"clientUserMessageId":message_id,"input":input,"attachments":[],"restoreMessage":{"id":message_id,"text":request.text,"createdAt":millis(SystemTime::now()),"context":{"prompt":request.text,"addedFiles":[],"fileAttachments":[],"ideContext":null,"imageAttachments":[],"commentAttachments":[]}}}),
        ),
        _ => return Err(Failure::Unsupported("Desktop operation".into())),
    };
    let mut wrote = false;
    let response = connection
        .request(
            method,
            version,
            &client,
            Some(&owner),
            params,
            Some(&mut || wrote = true),
        )
        .map_err(|error| match &error {
            IpcError::Rejected(reason)
                if !matches!(
                    reason.as_str(),
                    "request-timeout" | "server-closed" | "client-disconnected"
                ) =>
            {
                Failure::from(error)
            }
            _ if wrote => Failure::AcceptanceUncertain,
            _ => Failure::from(error),
        })?;
    if response["handledByClientId"] != owner {
        return Err(Failure::AcceptanceUncertain);
    }
    let accepted = &response["result"]["result"];
    let turn = if selected == Operation::Start {
        accepted["turn"]["id"].as_str()
    } else {
        accepted["turnId"].as_str()
    }
    .filter(|v| !v.is_empty())
    .ok_or(Failure::AcceptanceUncertain)?;
    if request.operation == Operation::Steer && request.expected_turn_id.as_deref() != Some(turn) {
        return Err(Failure::AcceptanceUncertain);
    }
    Ok(Some(base_receipt(
        target,
        request,
        if selected == Operation::Steer {
            DeliveryState::Steered
        } else {
            DeliveryState::Started
        },
        Some(turn.into()),
        Some(owner),
    )))
}
fn desktop_snapshot(
    connection: &mut DesktopIpc,
    client: &str,
    owner: &str,
    thread: &str,
) -> Result<Value> {
    let follow = |value| json!({"type":"broadcast","method":"thread-stream-following-changed","version":1,"sourceClientId":client,"targetClientIds":[owner],"params":{"hostId":"local","conversationId":thread,"following":value}});
    connection.post(follow(true))?;
    let result = (|| loop {
        let value = connection.receive_with_limit(64 * 1024 * 1024)?;
        if connection.decline_discovery(&value)? {
            continue;
        }
        if value["type"] != "broadcast"
            || value["method"] != "thread-stream-state-changed"
            || value["sourceClientId"] != owner
            || value["params"]["hostId"] != "local"
            || value["params"]["conversationId"] != thread
        {
            continue;
        }
        if value["version"] != 11 {
            return Err(Failure::Protocol("Desktop stream version".into()));
        }
        let change = &value["params"]["change"];
        if change["type"] != "snapshot" {
            continue;
        }
        if !change["conversationState"].is_object() {
            return Err(Failure::Protocol("Desktop state".into()));
        }
        return Ok(change["conversationState"].clone());
    })();
    // No standing subscription or second state collector is retained by sending.
    let _ = connection.post(follow(false));
    result
}
