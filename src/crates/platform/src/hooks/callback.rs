//! What the callback listener does with a callback, on either transport:
//! waits for the HUD's answer to a permission request, takes over an event
//! the callback could not write itself, and tells whether a request settles
//! in the client's own transcript.
use super::{now_ms, reminders, reviewer};
use agent_hud_agents::HookEvent;
use agent_hud_core::requests::Requests;

/// Waits for the HUD's answer to a permission request a client's hook sent,
/// while `keep_waiting` holds and until the request's deadline. None when the
/// request is not one, its reviewer is not the user, or no answer came.
pub(super) fn wait_for_answer(
    requests: &Requests,
    reminders: &reminders::Reminders,
    source: &str,
    flag: &str,
    payload: &serde_json::Value,
    keep_waiting: impl Fn() -> bool,
) -> Option<serde_json::Value> {
    let request = agent_hud_agents::parse_request(source, payload).ok()??;
    if request.requires_user_reviewer && !reviewer::is_user(requests, &request.source, payload) {
        return None;
    }
    reminders.record(source, flag, payload);
    let ticket = requests.register(request, now_ms());
    let response = loop {
        if !keep_waiting() {
            break None;
        }
        match ticket
            .response
            .recv_timeout(std::time::Duration::from_millis(100))
        {
            Ok(response) => break response,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break None,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if requests
                    .deadline(&ticket.id)
                    .is_none_or(|at| now_ms() >= at)
                {
                    break None;
                }
            }
        }
    };
    requests.cancel(&ticket.id);
    response
}

/// The event a callback hands over the channel because it could not write
/// it to the inbox (`entry`): only one the named client's hooks report.
pub(crate) fn handed_event(source: &str, payload: &serde_json::Value) -> Option<HookEvent> {
    let event: HookEvent = serde_json::from_value(payload.get("event")?.clone()).ok()?;
    let (id, _) = agent_hud_agents::hooks::hooks_of(source).ok()?;
    let reported = matches!(
        event.kind.as_str(),
        "origin" | "attention" | "compaction" | "completion" | "session-observation"
    );
    (event.source == id && reported).then_some(event)
}

/// Whether the client's transcript shows a permission request answered in
/// its own window, so a waiting callback can stop waiting for the HUD.
pub(crate) fn settles_in_transcript(source: &str) -> bool {
    agent_hud_agents::hooks::hooks_of(source)
        .ok()
        .and_then(|(_, hooks)| hooks.permission.as_ref())
        .is_some_and(|permission| permission.settles_in_transcript)
}
