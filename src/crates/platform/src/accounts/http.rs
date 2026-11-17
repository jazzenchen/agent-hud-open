use agent_hud_agents::account::{HttpFailure, MAX_BODY};
use reqwest::blocking::Response;
use serde_json::Value;
use std::io::Read;

/// Authentication and response bodies never become diagnostics. Limit even a
/// successful endpoint response before parsing it, without retaining raw
/// bodies.
pub(crate) fn read_json(mut response: Response) -> Result<Value, HttpFailure> {
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
    (&mut response)
        .take(MAX_BODY + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| HttpFailure::Unreadable)?;
    if bytes.len() as u64 > MAX_BODY {
        return Err(HttpFailure::TooLarge {
            size: whole_size(bytes.len() as u64, response),
        });
    }
    serde_json::from_slice(&bytes).map_err(|_| HttpFailure::NotJson)
}

/// How far a body over the limit is counted, not kept, to tell its size:
/// far enough for a body somewhat over, without waiting on a runaway one.
const COUNTED: u64 = 4 * MAX_BODY;

/// The size of a body whose first `read` bytes were read: the rest is counted
/// and dropped. None when it runs past `COUNTED` or cannot be read to its end.
fn whole_size(read: u64, rest: impl Read) -> Option<u64> {
    let limit = COUNTED.checked_sub(read)? + 1;
    let rest = std::io::copy(&mut rest.take(limit), &mut std::io::sink()).ok()?;
    Some(read + rest).filter(|size| *size <= COUNTED)
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

#[cfg(test)]
mod tests {
    use super::{retry_after, whole_size, COUNTED, MAX_BODY};
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
    use std::io::Read;

    #[test]
    fn a_body_over_the_limit_is_counted_to_its_end_up_to_a_bound() {
        let read = MAX_BODY + 1;
        let rest = std::io::repeat(b'x').take(2_000_000);
        assert_eq!(whole_size(read, rest), Some(read + 2_000_000));
        assert_eq!(whole_size(read, std::io::empty()), Some(read));
        // Past the bound, or cut off by an error, it has no size.
        assert_eq!(whole_size(read, std::io::repeat(b'x')), None);
        struct Broken;
        impl std::io::Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::ConnectionReset.into())
            }
        }
        assert_eq!(whole_size(read, Broken), None);
        assert_eq!(whole_size(COUNTED, std::io::empty()), Some(COUNTED));
        assert_eq!(whole_size(COUNTED + 1, std::io::empty()), None);
    }

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
