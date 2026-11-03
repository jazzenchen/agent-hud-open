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
        } else if status.as_u16() == 429 {
            HttpFailure::Limited {
                retry_after: retry_after(response.headers()),
            }
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

/// The seconds a `Retry-After` header asks to wait: a number of seconds, or
/// an HTTP date from now. None when it says neither.
fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<u32> {
    let value = headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim();
    if let Ok(seconds) = value.parse::<u32>() {
        return Some(seconds);
    }
    let at = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    u32::try_from((at.timestamp() - chrono::Utc::now().timestamp()).max(0)).ok()
}

/// `read_json`, with a failure worded as Open's ProviderHTTP words it; the
/// provider's section already names whose reading it is.
pub(crate) fn response_json(response: Response, _provider: &str) -> Result<Value, String> {
    read_json(response).map_err(HttpFailure::message)
}

#[cfg(test)]
mod tests {
    use super::retry_after;
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

    #[test]
    fn retry_after_reads_seconds_or_a_date() {
        let mut headers = HeaderMap::new();
        assert_eq!(retry_after(&headers), None);
        headers.insert(RETRY_AFTER, HeaderValue::from_static("120"));
        assert_eq!(retry_after(&headers), Some(120));
        let later = (chrono::Utc::now() + chrono::Duration::seconds(90)).to_rfc2822();
        headers.insert(RETRY_AFTER, HeaderValue::from_str(&later).unwrap());
        assert!(retry_after(&headers).is_some_and(|seconds| (85..=90).contains(&seconds)));
        headers.insert(RETRY_AFTER, HeaderValue::from_static("soon"));
        assert_eq!(retry_after(&headers), None);
    }
}
