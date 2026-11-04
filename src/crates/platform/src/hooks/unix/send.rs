//! A callback's side of the channel: its message sent to the app, and the
//! answer read back.
use super::super::{socket_path, PAYLOAD_LIMIT};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};

pub(crate) fn send(
    data: &Path,
    source: &str,
    flag: &str,
    payload: &Value,
) -> Result<Option<Value>, String> {
    let message = json!({"source":source,"flag":flag,"payload":payload});
    let mut bytes = serde_json::to_vec(&message).unwrap();
    bytes.push(b'\n');
    if bytes.len() > PAYLOAD_LIMIT {
        return Err("Hook callback is too large".into());
    }
    let mut stream = UnixStream::connect(socket_path(data))
        .map_err(|_| "Agent HUD hooks are disabled or not running")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|_| "Hook timeout unavailable")?;
    stream
        .write_all(&bytes)
        .map_err(|_| "Hook callback failed")?;
    // Keep the write side open: EOF means the waiting CLI was cancelled. The
    // JSONL request already frames its payload without requiring a half-close.
    let mut response = Vec::new();
    if flag == "--permission-hook" {
        let mut transcript = if super::super::settles_in_transcript(source) {
            super::super::transcript::Transcript::new(payload)
        } else {
            None
        };
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .map_err(|_| "Hook timeout unavailable")?;
        let deadline = std::time::Instant::now() + Duration::from_secs(86_405);
        loop {
            if transcript.as_mut().is_some_and(|watch| watch.settled()) {
                return Ok(None);
            }
            if std::time::Instant::now() >= deadline {
                return Ok(None);
            }
            let mut chunk = [0u8; 8192];
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    response.extend_from_slice(&chunk[..n]);
                    if response.len() > PAYLOAD_LIMIT {
                        return Err("Hook response is too large".into());
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    continue
                }
                Err(_) => return Err("Hook response did not complete".into()),
            }
        }
    } else {
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .map_err(|_| "Hook timeout unavailable")?;
        stream
            .take((PAYLOAD_LIMIT + 1) as u64)
            .read_to_end(&mut response)
            .map_err(|_| "Hook response did not complete")?;
    }
    if response.len() > PAYLOAD_LIMIT {
        return Err("Hook response is too large".into());
    }
    if response.is_empty() {
        Ok(None)
    } else {
        serde_json::from_slice(&response)
            .map(Some)
            .map_err(|_| "Invalid hook response".into())
    }
}
