//! LAN link: encrypted listener, local identity, pairing and grants, and a cursor-based
//! snapshot/changes/events feed. Plaintext requests never bind a production listener.
//!
//! The crate never interprets what it carries. The host supplies already-projected
//! records through [`Record`], and may mount extra authenticated routes through
//! [`server::Extension`]; nothing here knows about sessions or conversations.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
pub type Result<T> = std::result::Result<T, String>;
pub mod cache;
pub mod canonical;
pub mod credentials;
pub mod fs;
pub mod http;
pub mod identity;
mod os;
pub mod server;

/// One replicated item. `id` is its stable key.
pub trait Record: Clone + PartialEq + Serialize {
    fn id(&self) -> String;
    fn device_id(&self) -> &str;
    fn modified_at_ms(&self) -> i64;
    /// False once the record has expired; expired records are not published.
    fn is_visible(&self, now_ms: i64) -> bool;
    /// Whether `other` publishes the same content, its modification time
    /// aside: false whenever a client would need to see a new version.
    fn same_content(&self, other: &Self) -> bool;
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cursor {
    pub base_version: String,
    pub change_sequence: i64,
}
impl Cursor {
    pub fn new(ms: i64, sequence: i64) -> Option<Self> {
        if ms <= 0 || sequence < 0 {
            return None;
        }
        let date = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)?;
        Some(Self {
            base_version: date.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            change_sequence: sequence,
        })
    }
    pub fn base_ms(&self) -> Option<i64> {
        let date = chrono::DateTime::parse_from_rfc3339(&self.base_version).ok()?;
        let ms = date.timestamp_millis();
        (Self::new(ms, self.change_sequence)?.base_version == self.base_version).then_some(ms)
    }
    pub fn valid(&self) -> bool {
        self.base_ms().is_some()
    }
    pub fn query_value(&self) -> String {
        format!("{},{}", self.base_version, self.change_sequence)
    }
    pub fn parse(value: &str) -> Option<Self> {
        if value.len() > 80 {
            return None;
        }
        let (base, sequence) = value.split_once(',')?;
        let sequence = sequence
            .parse::<i64>()
            .ok()
            .filter(|number| number.to_string() == sequence)?;
        let cursor = Self {
            base_version: base.into(),
            change_sequence: sequence,
        };
        cursor.valid().then_some(cursor)
    }
}
