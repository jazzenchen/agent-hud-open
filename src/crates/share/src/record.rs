//! One record as the phone reads it: the envelope, its identifier and the
//! hash its content is compared by. The phone reads the same envelope from
//! the LAN link as from any other copy of a Mac's records.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const VERSION: u32 = 4;
pub const ACCOUNT_VERSION: u32 = 3;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncRecord {
    pub schema_version: u32,
    pub kind: String,
    #[serde(rename = "deviceID")]
    pub device_id: String,
    pub key: String,
    pub modified_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<i64>,
    pub payload: Value,
}
impl SyncRecord {
    pub fn new(
        kind: impl Into<String>,
        device_id: impl Into<String>,
        key: impl Into<String>,
        payload: Value,
        modified_at_ms: i64,
        expires_at_ms: Option<i64>,
    ) -> Self {
        let kind = kind.into();
        Self {
            schema_version: version(&kind),
            kind,
            device_id: device_id.into(),
            key: key.into(),
            payload,
            modified_at_ms,
            expires_at_ms,
        }
    }
    pub fn id(&self) -> String {
        record_id(&self.kind, &self.device_id, &self.key, None)
    }
    pub fn is_visible(&self, now_ms: i64) -> bool {
        self.expires_at_ms.is_none_or(|end| end > now_ms)
    }
}
pub fn version(kind: &str) -> u32 {
    if matches!(kind, "subscription" | "declaration") {
        ACCOUNT_VERSION
    } else {
        VERSION
    }
}
pub fn hash(components: &[&str]) -> String {
    let mut digest = Sha256::new();
    for value in components {
        digest.update(value.len().to_string().as_bytes());
        digest.update(b":");
        digest.update(value.as_bytes());
    }
    format!("{:x}", digest.finalize())
}
pub fn record_id(kind: &str, device_id: &str, key: &str, explicit_version: Option<u32>) -> String {
    format!(
        "v{}.{}",
        explicit_version.unwrap_or_else(|| version(kind)),
        hash(&[kind, device_id, key])
    )
}
/// What a record says, its expiry aside.
pub fn content(record: &SyncRecord) -> String {
    hash(&[
        &record.kind,
        &record.device_id,
        &record.key,
        &serde_json::to_string(&record.payload).unwrap_or_default(),
    ])
}
/// What a record says, its expiry included: a new value means a new version.
pub fn current_content(record: &SyncRecord) -> String {
    hash(&[
        &content(record),
        &record
            .expires_at_ms
            .map(|v| v.to_string())
            .unwrap_or_default(),
    ])
}
impl agent_hud_lan::Record for SyncRecord {
    fn id(&self) -> String {
        SyncRecord::id(self)
    }
    fn device_id(&self) -> &str {
        &self.device_id
    }
    fn modified_at_ms(&self) -> i64 {
        self.modified_at_ms
    }
    fn is_visible(&self, now_ms: i64) -> bool {
        SyncRecord::is_visible(self, now_ms)
    }
    fn content(&self) -> String {
        current_content(self)
    }
    /// The fields `current_content` hashes, compared without hashing them.
    fn same_content(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.device_id == other.device_id
            && self.key == other.key
            && self.expires_at_ms == other.expires_at_ms
            && self.payload == other.payload
    }
}
