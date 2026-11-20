use super::{Handler, NO_ANSWER, REQUEST_LIMIT, RESPONSE_LIMIT};
use serde_json::Value;
use std::{path::Path, sync::Arc, time::Duration};
pub struct Server(crate::os::pipe::Server);
impl Server {
    pub fn start(data: &Path, handler: Handler) -> Result<Self, String> {
        crate::os::pipe::Server::start(
            data,
            "tools",
            REQUEST_LIMIT as usize,
            RESPONSE_LIMIT as usize,
            Arc::new(move |request, active| Some(handler(request, active))),
        )
        .map(Self)
    }
    pub fn is_listening(&self) -> bool {
        self.0.is_listening()
    }
}
pub fn request(data: &Path, request: &Value) -> Result<Value, String> {
    crate::os::pipe::request(
        data,
        "tools",
        request,
        REQUEST_LIMIT as usize,
        RESPONSE_LIMIT as usize,
        Duration::from_secs(40),
        || false,
    )?
    .ok_or_else(|| NO_ANSWER.into())
}
