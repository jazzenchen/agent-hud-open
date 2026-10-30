//! Generic transport port of the public CodexDesktopIPC.swift. Only framing,
//! request correlation and discovery refusal live here; callers own operations.
use serde_json::{json, Value};
use std::{
    io::{self, Read, Write},
    path::Path,
    time::Duration,
};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{endpoint, LocalSocket, ShutdownHandle};
#[cfg(unix)]
use unix::{random as fill_random, Frame};
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{endpoint, LocalSocket, ShutdownHandle};
#[cfg(windows)]
use windows::{random as fill_random, Frame};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IpcError {
    Unavailable,
    TimedOut,
    Protocol,
    Rejected(String),
}
/// What a failed answer tells the person, in their language; the variant
/// itself is for the logs.
impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use agent_hud_core::l10n::text;
        match self {
            Self::Unavailable => f.write_str(&text("无法连接 Codex", "Codex could not be reached")),
            Self::TimedOut => {
                f.write_str(&text("Codex 没有及时回应", "Codex did not answer in time"))
            }
            Self::Protocol => f.write_str(&text(
                "Codex 的回应无法识别",
                "Codex answered in a way Agent HUD cannot read",
            )),
            Self::Rejected(reason) => {
                if agent_hud_core::l10n::chinese() {
                    write!(f, "Codex 拒绝了这个回应：{reason}")
                } else {
                    write!(f, "Codex declined the answer: {reason}")
                }
            }
        }
    }
}
impl std::error::Error for IpcError {}
impl From<io::Error> for IpcError {
    fn from(e: io::Error) -> Self {
        if matches!(
            e.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ) {
            Self::TimedOut
        } else {
            Self::Unavailable
        }
    }
}

pub struct DesktopIpc {
    socket: LocalSocket,
}
impl DesktopIpc {
    pub fn connect(path: &Path, timeout: Duration) -> Result<Self, IpcError> {
        Ok(Self {
            socket: LocalSocket::connect(path, timeout)?,
        })
    }
    pub fn renew(&mut self, timeout: Duration) {
        self.socket.renew(timeout);
    }
    pub fn wait_for_message(&self, timeout: Duration) -> Result<bool, IpcError> {
        self.socket.wait_for_message(timeout)
    }
    pub fn request(
        &mut self,
        method: &str,
        version: i64,
        source: &str,
        target: Option<&str>,
        params: Value,
        on_write: Option<&mut dyn FnMut()>,
    ) -> Result<Value, IpcError> {
        let request_id = request_id()?;
        let mut message = json!({"type":"request","requestId":request_id,"sourceClientId":source,"version":version,"method":method,"params":params,
            "timeoutMs":self.socket.remaining().as_millis().clamp(1,i32::MAX as u128) as u64});
        if let Some(target) = target {
            message["targetClientId"] = json!(target);
        }
        self.post_with_write(message, on_write)?;
        loop {
            let response = self.receive()?;
            if self.decline_discovery(&response)? {
                continue;
            }
            if response["type"] != "response" || response["requestId"] != request_id {
                continue;
            }
            return match response["resultType"].as_str() {
                Some("success") => Ok(response),
                Some("error") => Err(IpcError::Rejected(
                    response["error"]
                        .as_str()
                        .unwrap_or("unknown-ipc-error")
                        .into(),
                )),
                _ => Err(IpcError::Protocol),
            };
        }
    }
    pub fn post(&mut self, message: Value) -> Result<(), IpcError> {
        self.post_with_write(message, None)
    }
    pub fn post_with_write(
        &mut self,
        message: Value,
        mut on_write: Option<&mut dyn FnMut()>,
    ) -> Result<(), IpcError> {
        let bytes = serde_json::to_vec(&message).map_err(|_| IpcError::Protocol)?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(IpcError::Protocol);
        }
        let frame = [
            (bytes.len() as u32).to_le_bytes().as_slice(),
            bytes.as_slice(),
        ]
        .concat();
        let mut offset = 0;
        while offset < frame.len() {
            let count = self.socket.write(&frame[offset..])?;
            if count == 0 {
                return Err(IpcError::Unavailable);
            }
            if offset == 0 {
                if let Some(callback) = on_write.as_mut() {
                    callback();
                }
            }
            offset += count;
        }
        Ok(())
    }
    pub fn receive(&mut self) -> Result<Value, IpcError> {
        self.receive_with_limit(64 * 1024 * 1024)
    }
    pub fn receive_with_limit(&mut self, limit: usize) -> Result<Value, IpcError> {
        let bytes = self.receive_frame(limit)?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| IpcError::Protocol)?;
        if !value.is_object() {
            return Err(IpcError::Protocol);
        }
        Ok(value)
    }
    /// Like the source stream reader, omit oversized leaf strings while decoding,
    /// rather than first retaining copies of screenshots/tool dumps in a Value tree.
    pub fn receive_trimmed(
        &mut self,
        limit: usize,
        string_limit: usize,
    ) -> Result<Value, IpcError> {
        use serde::de::DeserializeSeed;
        let bytes = self.receive_frame(limit)?;
        let mut decoder = serde_json::Deserializer::from_slice(&bytes);
        let value = TrimStrings(string_limit)
            .deserialize(&mut decoder)
            .map_err(|_| IpcError::Protocol)?;
        decoder.end().map_err(|_| IpcError::Protocol)?;
        if !value.is_object() {
            return Err(IpcError::Protocol);
        }
        Ok(value)
    }
    fn receive_frame(&mut self, limit: usize) -> Result<Frame, IpcError> {
        let mut header = [0; 4];
        self.socket.read_exact(&mut header)?;
        let length = u32::from_le_bytes(header) as usize;
        if length == 0 || length > limit {
            return Err(IpcError::Protocol);
        }
        let mut bytes = Frame::new(length)?;
        for chunk in bytes.bytes_mut().chunks_mut(1024 * 1024) {
            self.socket.read_exact(chunk)?;
        }
        Ok(bytes)
    }
    pub fn decline_discovery(&mut self, message: &Value) -> Result<bool, IpcError> {
        if message["type"] != "client-discovery-request" {
            return Ok(false);
        }
        // A discovery request without its id is not one to answer; it is
        // ignored as Open ignores it, and the connection stays.
        let Some(id) = message["requestId"].as_str() else {
            return Ok(false);
        };
        self.post(json!({"type":"client-discovery-response","requestId":id,"response":{"canHandle":false}}))?;
        Ok(true)
    }
}
#[derive(Clone, Copy)]
struct TrimStrings(usize);
impl<'de> serde::de::DeserializeSeed<'de> for TrimStrings {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<Value, D::Error> {
        decoder.deserialize_any(self)
    }
}
impl<'de> serde::de::Visitor<'de> for TrimStrings {
    type Value = Value;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JSON value")
    }
    fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Value, E> {
        Ok(serde_json::Number::from_f64(v)
            .map(Value::Number)
            .unwrap_or(Value::Null))
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E: serde::de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(if v.len() > self.0 {
            Value::Null
        } else {
            Value::String(v.into())
        })
    }
    fn visit_borrowed_str<E: serde::de::Error>(self, v: &'de str) -> Result<Value, E> {
        self.visit_str(v)
    }
    fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Value, E> {
        Ok(if v.len() > self.0 {
            Value::Null
        } else {
            Value::String(v)
        })
    }
    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(self)? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut members = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            members.insert(key, map.next_value_seed(self)?);
        }
        Ok(Value::Object(members))
    }
}
pub fn request_id() -> Result<String, IpcError> {
    let mut random = [0u8; 16];
    fill_random(&mut random)?;
    random[6] = (random[6] & 0x0f) | 0x40;
    random[8] = (random[8] & 0x3f) | 0x80;
    let hex = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}
