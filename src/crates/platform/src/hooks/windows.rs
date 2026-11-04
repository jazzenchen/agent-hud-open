use super::{now_ms, Wake, PAYLOAD_LIMIT};
use agent_hud_core::requests::Requests;
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{atomic::Ordering, mpsc::RecvTimeoutError, Arc},
    time::Duration,
};
pub(super) struct Server(crate::os::pipe::Server);
impl Server {
    pub fn start(
        data: &Path,
        requests: Requests,
        wake: Wake,
        reminders: super::reminders::Reminders,
    ) -> Result<Self, String> {
        let folder = data.to_path_buf();
        let handler = Arc::new(
            move |message: Value, active: &std::sync::atomic::AtomicBool| {
                let (Some(source), Some(flag), Some(payload)) = (
                    message["source"].as_str(),
                    message["flag"].as_str(),
                    message.get("payload"),
                ) else {
                    return None;
                };
                if !active.load(Ordering::Acquire) {
                    return None;
                }
                if flag != "--permission-hook" {
                    // The event of a callback that could not write it (`entry`).
                    if let Some(event) = super::handed_event(source, payload) {
                        let _ = super::record_event(&folder, &event);
                    }
                    reminders.record(source, flag, payload);
                    wake();
                    return Some(json!({}));
                }
                let request = agent_hud_agents::parse_request(source, payload).ok()??;
                if request.requires_user_reviewer
                    && !super::reviewer::is_user(&requests, &request.source, payload)
                {
                    return None;
                }
                reminders.record(source, flag, payload);
                let ticket = requests.register(request, now_ms());
                let response = loop {
                    if !active.load(Ordering::Acquire) {
                        break None;
                    }
                    match ticket.response.recv_timeout(Duration::from_millis(100)) {
                        Ok(response) => break response,
                        Err(RecvTimeoutError::Disconnected) => break None,
                        Err(RecvTimeoutError::Timeout) => {
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
            },
        );
        crate::os::pipe::Server::start(data, "hooks", PAYLOAD_LIMIT, PAYLOAD_LIMIT, handler)
            .map(Self)
    }
    pub fn is_listening(&self) -> bool {
        self.0.is_listening()
    }
}
pub(super) fn send(
    data: &Path,
    source: &str,
    flag: &str,
    payload: &Value,
    _origin: Option<Value>,
) -> Result<Option<Value>, String> {
    let mut transcript = if super::settles_in_transcript(source) && flag == "--permission-hook" {
        super::transcript::Transcript::new(payload)
    } else {
        None
    };
    crate::os::pipe::request(
        data,
        "hooks",
        &json!({"source":source,"flag":flag,"payload":payload}),
        PAYLOAD_LIMIT,
        PAYLOAD_LIMIT,
        Duration::from_secs(if flag == "--permission-hook" {
            86405
        } else {
            3
        }),
        || transcript.as_mut().is_some_and(|watch| watch.settled()),
    )
}
