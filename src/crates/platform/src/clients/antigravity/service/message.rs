//! A message into an existing Antigravity conversation, sent once to the
//! running service that opened it.
use super::{service, Endpoint};
use serde_json::{json, Value};

/// Failure before a message is sent, or an unconfirmed result after the one
/// permitted send. Never retry an unconfirmed write against another service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageError {
    Invalid(String),
    Unavailable(String),
    Uncertain,
}

/// An existing conversation and the running service that opened it. Its
/// endpoint and credential remain private to the platform adapter.
pub struct MessageSession {
    cascade_id: String,
    endpoint: Endpoint,
}

/// The service a message goes to: one that already has the conversation
/// open, else the first that can open it. `GetAllCascadeTrajectories` lists
/// only the conversations a service has loaded (none: the map is left out),
/// so it can prefer a service but never refuse one; any service opens a
/// conversation from disk when asked for its steps (`open`), and answers
/// not_found for one it cannot find.
fn pick_service<T: Clone>(
    endpoints: &[T],
    cascade_id: &str,
    mut loaded: impl FnMut(&T) -> Result<Value, String>,
    mut open: impl FnMut(&T) -> Result<Value, String>,
) -> Result<T, MessageError> {
    let has_it = |body: Result<Value, String>| {
        body.ok().is_some_and(|body| {
            body["trajectorySummaries"]
                .as_object()
                .is_some_and(|loaded| loaded.contains_key(cascade_id))
        })
    };
    let (first, rest): (Vec<&T>, Vec<&T>) = endpoints
        .iter()
        .partition(|endpoint| has_it(loaded(endpoint)));
    let mut last = None;
    for endpoint in first.into_iter().chain(rest) {
        match open(endpoint) {
            Ok(_) => return Ok(endpoint.clone()),
            Err(error) => last = Some(error),
        }
    }
    Err(MessageError::Unavailable(last.unwrap_or_else(|| {
        "No running Antigravity service is available".into()
    })))
}

/// Resolve a known conversation without enabling permission observation or
/// the generic control channel on Windows.
pub fn message_session(cascade_id: &str) -> Result<MessageSession, MessageError> {
    if !crate::hooks::origins::is_uuid(cascade_id) {
        return Err(MessageError::Invalid("Antigravity session identity".into()));
    }
    if !crate::clients::runs(&agent_hud_agents::antigravity::ACCOUNT_SERVICE) {
        return Err(MessageError::Unavailable(
            "Antigravity message discovery is unsupported on this system".into(),
        ));
    }
    let service = service().map_err(MessageError::Unavailable)?;
    let endpoints = service
        .discover(true, false)
        .map_err(MessageError::Unavailable)?;
    let endpoint = pick_service(
        &endpoints,
        cascade_id,
        |endpoint| service.send(endpoint, "GetAllCascadeTrajectories", &json!({})),
        // The steps after the last there can be: none, so the answer is
        // `{}` however long the conversation is.
        |endpoint| {
            service.send(
                endpoint,
                "GetCascadeTrajectorySteps",
                &json!({"cascadeId": cascade_id, "stepOffset": u32::MAX}),
            )
        },
    )?;
    Ok(MessageSession {
        cascade_id: cascade_id.to_owned(),
        endpoint,
    })
}

impl MessageSession {
    /// A read of the conversation's latest turn from the service that opened
    /// it: how many steps it has, or the turn holding a step.
    pub fn read(&self, method: &str, params: &Value) -> Result<Value, String> {
        if !matches!(method, "GetConversationMetadata" | "GetTrajectoryTurns") {
            return Err("Unsupported public Antigravity service operation".into());
        }
        service()?.send(&self.endpoint, method, params)
    }

    pub fn user_status(&self) -> Result<Value, MessageError> {
        service()
            .map_err(MessageError::Unavailable)?
            .send(
                &self.endpoint,
                "GetUserStatus",
                &json!({"metadata":{"ideName":"antigravity","extensionName":"antigravity","locale":"en","ideVersion":"unknown"}}),
            )
            .map_err(MessageError::Unavailable)
    }

    /// The one write, to the service that opened the conversation. An error
    /// after the request starts is uncertain, including a lost response or an
    /// unexpected body, and is never retried on another service.
    pub fn send(&self, params: &Value) -> Result<Value, MessageError> {
        if params["cascadeId"].as_str() != Some(self.cascade_id.as_str()) {
            return Err(MessageError::Invalid("Antigravity session identity".into()));
        }
        service()
            .map_err(MessageError::Unavailable)?
            .send(&self.endpoint, "SendUserCascadeMessage", params)
            .map_err(|_| MessageError::Uncertain)
    }
}

#[cfg(test)]
mod tests;
