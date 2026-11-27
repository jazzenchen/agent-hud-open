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

pub(super) fn start(body: Value, request: &StartRequest) -> Result<StartReceipt> {
    use crate::clients::antigravity::service::desktop_start_endpoint;
    let endpoint = desktop_start_endpoint()
        .map_err(Failure::Rejected)?
        .ok_or_else(|| without_desktop(&body))?;
    start_with_endpoint(body, request, |method, params| {
        endpoint.call(method, params)
    })
}

fn without_desktop(body: &Value) -> Failure {
    let reason = "Open and sign in to Antigravity Desktop to start a conversation".into();
    if body.get("projectEnvConfig").is_some() {
        // A CLI start would lose the requested project's sidebar placement.
        Failure::Rejected(reason)
    } else {
        Failure::Unavailable(reason)
    }
}

fn start_with_endpoint(
    body: Value,
    request: &StartRequest,
    mut call: impl FnMut(&str, &Value) -> std::result::Result<Value, String>,
) -> Result<StartReceipt> {
    start_on(body, request, |method, params| {
        call(method, &params).map_err(|error| {
            if method == "GetUserStatus" {
                Failure::Unavailable(error)
            } else {
                // A missing response does not prove a mutation failed. Do not
                // retry the create or its first message on another service.
                Failure::AcceptanceUncertain
            }
        })
    })
}

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

#[cfg(test)]
mod tests {
    use super::*;

    const CASCADE: &str = "81ecaf0a-a36d-4f86-99c0-7b03633f3c54";

    fn request() -> StartRequest {
        StartRequest {
            provider: Provider::Antigravity,
            request_id: "c16ba816-9ab7-4173-92ae-fe78a3a574b5".into(),
            text: "Hello".into(),
            folder: "~/project".into(),
            title: None,
            model: Some("MODEL_PLACEHOLDER_M318".into()),
        }
    }

    #[test]
    fn desktop_start_sends_project_then_prompt_on_one_endpoint() {
        let body = json!({
            "projectEnvConfig": {"projectId":"example","defaultProjectEnvironment":{}}
        });
        let mut calls = Vec::new();
        let receipt = start_with_endpoint(body.clone(), &request(), |method, params| {
            calls.push(method.to_owned());
            match method {
                "StartCascade" => {
                    assert_eq!(params, &body);
                    Ok(json!({"cascadeId": CASCADE}))
                }
                "SendUserCascadeMessage" => {
                    assert_eq!(params["cascadeId"], CASCADE);
                    assert_eq!(params["items"][0]["text"], "Hello");
                    Ok(json!({}))
                }
                _ => panic!("unexpected method {method}"),
            }
        })
        .unwrap();
        assert_eq!(calls, ["StartCascade", "SendUserCascadeMessage"]);
        assert_eq!(receipt.session_id.as_deref(), Some(CASCADE));
        assert_eq!(receipt.state, "started");
    }

    #[test]
    fn desktop_start_timeout_never_attempts_the_first_message() {
        let mut calls = 0;
        let error = start_with_endpoint(json!({}), &request(), |method, _| {
            calls += 1;
            assert_eq!(method, "StartCascade");
            Err("connection closed".into())
        })
        .unwrap_err();
        assert_eq!(error, Failure::AcceptanceUncertain);
        assert_eq!(calls, 1);
    }

    #[test]
    fn desktop_start_keeps_the_id_when_the_first_message_is_unconfirmed() {
        let mut calls = Vec::new();
        let receipt = start_with_endpoint(json!({}), &request(), |method, _| {
            calls.push(method.to_owned());
            match method {
                "StartCascade" => Ok(json!({"cascadeId": CASCADE})),
                "SendUserCascadeMessage" => Err("connection closed".into()),
                _ => panic!("unexpected method {method}"),
            }
        })
        .unwrap();
        assert_eq!(calls, ["StartCascade", "SendUserCascadeMessage"]);
        assert_eq!(receipt.session_id.as_deref(), Some(CASCADE));
        assert_eq!(receipt.state, "uncertain");
    }

    #[test]
    fn desktop_project_start_never_falls_back_to_the_cli() {
        let project = json!({"projectEnvConfig":{"projectId":"example"}});
        assert!(matches!(without_desktop(&project), Failure::Rejected(_)));
        assert!(matches!(
            without_desktop(&json!({"workspaceUris":["file:///D:/example"]})),
            Failure::Unavailable(_)
        ));
    }
}
