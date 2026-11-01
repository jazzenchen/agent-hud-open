use agent_hud_agents::account::{HttpFailure, MAX_BODY};
use reqwest::blocking::Response;
use serde_json::Value;
use std::io::Read;

/// Authentication and response bodies never become diagnostics. Limit even a
/// successful endpoint response before parsing it, without retaining raw
/// bodies.
pub(crate) fn read_json(response: Response) -> Result<Value, HttpFailure> {
    let status = response.status();
    if !status.is_success() {
        // Cloudflare names the browser check it answers with in place of the data.
        let challenged = response
            .headers()
            .get("cf-mitigated")
            .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"challenge"));
        return Err(if challenged {
            HttpFailure::Challenged
        } else {
            HttpFailure::Status(status.as_u16())
        });
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_BODY + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| HttpFailure::Unreadable)?;
    if bytes.len() as u64 > MAX_BODY {
        return Err(HttpFailure::TooLarge);
    }
    serde_json::from_slice(&bytes).map_err(|_| HttpFailure::NotJson)
}

/// `read_json`, with a failure worded as Open's ProviderHTTP words it; the
/// provider's section already names whose reading it is.
pub(crate) fn response_json(response: Response, _provider: &str) -> Result<Value, String> {
    read_json(response).map_err(HttpFailure::message)
}
