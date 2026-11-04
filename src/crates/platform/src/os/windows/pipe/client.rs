//! A request to a local named-pipe server, and its answer read back.
use super::name;
use serde_json::Value;
use std::{
    path::Path,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::ClientOptions,
};

pub fn request(
    data: &Path,
    channel: &str,
    message: &Value,
    request_limit: usize,
    response_limit: usize,
    timeout: Duration,
    mut settled: impl FnMut() -> bool,
) -> Result<Option<Value>, String> {
    let mut bytes = serde_json::to_vec(message).unwrap();
    bytes.push(b'\n');
    if bytes.len() > request_limit {
        return Err("The local IPC request is too large".into());
    }
    let name = name(data, channel)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "The local IPC runtime is unavailable")?;
    runtime.block_on(async {
        let mut stream = connect(&name, timeout.min(Duration::from_secs(2))).await?;
        tokio::time::timeout(Duration::from_secs(2), stream.write_all(&bytes))
            .await
            .map_err(|_| "The local IPC request timed out")?
            .map_err(|_| "The local IPC request failed")?;
        let deadline = Instant::now() + timeout;
        let mut response = Vec::new();
        loop {
            if settled() || Instant::now() >= deadline {
                return Ok(None);
            }
            let mut chunk = [0; 8192];
            match tokio::time::timeout(Duration::from_millis(100), stream.read(&mut chunk)).await {
                Err(_) => continue,
                Ok(Err(_)) => return Err("The local IPC response failed".into()),
                Ok(Ok(0)) => {
                    return if response.is_empty() {
                        Ok(None)
                    } else {
                        serde_json::from_slice(&response)
                            .map(Some)
                            .map_err(|_| "The local IPC response is invalid".into())
                    }
                }
                Ok(Ok(n)) => {
                    response.extend_from_slice(&chunk[..n]);
                    if response.len() > response_limit {
                        return Err("The local IPC response is too large".into());
                    }
                    if response.last() == Some(&b'\n') {
                        return serde_json::from_slice(&response)
                            .map(Some)
                            .map_err(|_| "The local IPC response is invalid".into());
                    }
                }
            }
        }
    })
}

/// A connected instance may be busy for the brief interval before the listener
/// creates its next instance. Retry only that Windows error, within the caller's
/// deadline; a missing endpoint still fails immediately.
pub(super) async fn connect(
    name: &str,
    timeout: Duration,
) -> Result<tokio::net::windows::named_pipe::NamedPipeClient, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match ClientOptions::new().open(name) {
            Ok(stream) => return Ok(stream),
            Err(error)
                if error.raw_os_error()
                    == Some(windows_sys::Win32::Foundation::ERROR_PIPE_BUSY as i32)
                    && Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(_) => {
                return Err(
                    "Agent HUD is not listening; enable the requested local service in Settings"
                        .into(),
                )
            }
        }
    }
}
