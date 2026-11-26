//! LAN link: encrypted listener, local identity, pairing and grants, and a cursor-based
//! snapshot/changes/events feed. Plaintext requests never bind a production listener.
//!
//! The crate never interprets what it carries. The host supplies already-projected
//! records through [`Record`], and may mount extra authenticated routes through
//! [`server::Extension`]. All it asks of a record is its kind and whether it is
//! one session's, so a reader can take the list without every conversation
//! ([`link::Scope`]).
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
pub type Result<T> = std::result::Result<T, String>;
pub mod cache;
pub mod canonical;
pub mod credentials;
pub mod fs;
pub mod http;
pub mod identity;
pub mod link;
mod os;
pub mod server;

/// Whether a caller at `peer`, taken on this device's address `here`, is on
/// the device's own network and not a tunnel or the internet (the listener's
/// own rule, `Request::nearby`), for a host that listens beside it.
pub fn nearby(peer: std::net::IpAddr, here: std::net::IpAddr) -> bool {
    os::nearby(peer, here, &os::addresses())
}

/// This device's addresses a caller can reach it at: those on its own network
/// (Ethernet and Wi-Fi), then those of an overlay network such as Tailscale.
pub fn addresses() -> (Vec<String>, Vec<String>) {
    (os::addresses(), os::overlay_addresses())
}

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
    /// What the record is of ("session", "turn", "usage"), for a reader that
    /// asks for less than everything (`link::Scope`). A record type with no
    /// kinds says none, and is always sent.
    fn kind(&self) -> &str {
        ""
    }
    /// Whether the record belongs to one session's conversation.
    fn in_session(&self, _provider: &str, _session: &str) -> bool {
        false
    }
    /// The record without its long text, for a reader that keeps every record
    /// but opens one conversation at a time (`link::Scope`, "light"). A record
    /// type with nothing to leave out gives itself.
    fn light(&self) -> Self {
        self.clone()
    }
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
