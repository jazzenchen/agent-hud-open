//! A connection to the shared Codex CLI daemon: JSON-RPC over a WebSocket on
//! its Unix control socket. A mutation that wrote any bytes before failing
//! reads as uncertain, never as refused.
use super::*;
use crate::clients::codex::desktop_io::LocalSocket;
use std::io::{Read, Write};
use tungstenite::{Message, WebSocket};

pub(super) struct TrackedSocket {
    pub(super) inner: LocalSocket,
    mutating: bool,
    wrote: bool,
}
impl Read for TrackedSocket {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(bytes)
    }
}
impl Write for TrackedSocket {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(bytes)?;
        if n > 0 && self.mutating {
            self.wrote = true;
        }
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
pub(super) struct Cli {
    pub(super) socket: WebSocket<TrackedSocket>,
    next_id: u64,
}
impl Cli {
    pub(super) fn connect(path: &Path, timeout: Duration) -> Result<Self> {
        let stream = TrackedSocket {
            inner: LocalSocket::connect(path, timeout)?,
            mutating: false,
            wrote: false,
        };
        let config = tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(8 * 1024 * 1024))
            .max_frame_size(Some(8 * 1024 * 1024));
        let (socket, _) =
            tungstenite::client::client_with_config("ws://localhost/", stream, Some(config))
                .map_err(|e| Failure::Protocol(format!("CLI websocket handshake: {e}")))?;
        let mut connection = Self { socket, next_id: 1 };
        connection.request("initialize",json!({"clientInfo":{"name":"agent_hud","title":"Agent HUD","version":"0.1.0"},"capabilities":{"experimentalApi":true}}),false)?;
        connection
            .socket
            .send(Message::Text(
                json!({"method":"initialized"}).to_string().into(),
            ))
            .map_err(ws_error)?;
        Ok(connection)
    }
    pub(super) fn request(&mut self, method: &str, params: Value, mutation: bool) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        {
            let stream = self.socket.get_mut();
            stream.mutating = mutation;
            stream.wrote = false;
        }
        let result = (|| {
            self.socket
                .send(Message::Text(
                    json!({"id":id,"method":method,"params":params})
                        .to_string()
                        .into(),
                ))
                .map_err(ws_error)?;
            loop {
                let message = self.socket.read().map_err(ws_error)?;
                let value: Value = match message {
                    Message::Text(text) => serde_json::from_str(text.as_str())
                        .map_err(|_| Failure::Protocol("CLI JSON".into()))?,
                    Message::Ping(_) | Message::Pong(_) => continue,
                    _ => return Err(Failure::Unavailable("CLI websocket closed".into())),
                };
                if value.get("method").is_some() || value["id"].as_u64() != Some(id) {
                    continue;
                }
                if let Some(error) = value.get("error") {
                    return Err(Failure::Rejected(format!(
                        "{}: {}",
                        error["code"],
                        error["message"].as_str().unwrap_or("Unknown CLI rejection")
                    )));
                }
                if !value["result"].is_object() {
                    return Err(Failure::Protocol("CLI result".into()));
                }
                return Ok(value["result"].clone());
            }
        })();
        let wrote = self.socket.get_ref().wrote;
        self.socket.get_mut().mutating = false;
        match result {
            Err(error) if mutation && wrote && !matches!(error, Failure::Rejected(_)) => {
                Err(Failure::AcceptanceUncertain)
            }
            result => result,
        }
    }
    pub(super) fn mutation(
        &mut self,
        method: &str,
        params: Value,
        parse: impl FnOnce(Value) -> Result<(DeliveryState, Option<String>)>,
    ) -> Result<(DeliveryState, Option<String>)> {
        let value = self.request(method, params, true)?;
        parse(value).map_err(|_| Failure::AcceptanceUncertain)
    }
}
fn ws_error(error: tungstenite::Error) -> Failure {
    match error {
        tungstenite::Error::Io(error) => error.into(),
        _ => Failure::Protocol(format!("CLI websocket: {error}")),
    }
}
