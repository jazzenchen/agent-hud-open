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
        &record.payload.to_string(),
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
    fn kind(&self) -> &str {
        &self.kind
    }
    fn in_session(&self, provider: &str, session: &str) -> bool {
        self.payload["provider"] == provider && self.payload["sessionID"] == session
    }
    /// A turn without its words: what a phone keeps of every turn it is not
    /// reading (its `SyncRecord.resident`).
    fn light(&self) -> Self {
        let mut light = self.clone();
        if let Some(payload) = light.payload.as_object_mut() {
            payload.remove("text");
            payload.remove("items");
        }
        light
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

#[cfg(test)]
mod link_tests {
    use super::*;
    use agent_hud_lan::{cache::SnapshotCache, link};
    use serde_json::json;
    use std::sync::Mutex;

    fn record(kind: &str, key: &str, payload: Value) -> SyncRecord {
        serde_json::from_value(json!({"schemaVersion":VERSION,"kind":kind,"deviceID":"mac","key":key,"modifiedAtMs":1_000,"payload":payload}))
            .unwrap()
    }

    /// A reader that asks for "light" keeps every record, usage too, and every
    /// turn's facts; only the session it watches comes with its words.
    #[test]
    fn a_light_reader_gets_every_turn_and_the_words_of_the_one_it_watches() {
        let directory = tempfile::tempdir().unwrap();
        let cache = Mutex::new(
            SnapshotCache::new(
                "mac".into(),
                "Mac".into(),
                directory.path().join("cursor.json"),
            )
            .unwrap(),
        );
        let turn = |session: &str| {
            record(
                "turn",
                &format!("claude/{session}/t1"),
                json!({"provider":"claude","sessionID":session,"turnID":"t1","startedAtMs":5,"prompt":"hi","text":"hello","items":[{"id":"i","role":"assistant","text":"hello","atMs":6}]}),
            )
        };
        let records = [
            record(
                "session",
                "claude/a",
                json!({"provider":"claude","sessionID":"a"}),
            ),
            turn("a"),
            turn("b"),
            record(
                "usage",
                "2026-11-26",
                json!({"day":"2026-11-26","tokens":[]}),
            ),
        ];
        cache.lock().unwrap().update(records.iter(), 2_000).unwrap();
        let ask = |request: Value| -> Value {
            let (reply, _) =
                link::answer(&cache, request.to_string().as_bytes(), &mut |_, _, _| None);
            serde_json::from_slice(&reply).unwrap()
        };
        let turns = |reply: &Value| -> Vec<(String, bool, bool)> {
            let mut turns: Vec<_> = reply["body"]["records"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|record| record["kind"] == "turn")
                .map(|record| {
                    let payload = &record["payload"];
                    (
                        payload["sessionID"].as_str().unwrap().to_owned(),
                        payload.get("text").is_some() && payload.get("items").is_some(),
                        payload["prompt"] == "hi" && record["modifiedAtMs"] == 1_000,
                    )
                })
                .collect();
            turns.sort();
            turns
        };
        let whole = ask(json!({"id":1,"method":"GET","path":"/v1/snapshot"}));
        let light = ask(json!({"id":2,"method":"GET","path":"/v1/snapshot","scope":"light"}));
        // Nothing is left out, usage included; no turn has its words, each keeps the rest.
        assert_eq!(light["body"]["records"].as_array().unwrap().len(), 4);
        assert_eq!(
            turns(&light),
            [("a".into(), false, true), ("b".into(), false, true)]
        );
        // The position and the check value are the whole content's.
        assert_eq!(light["etag"], whole["etag"]);
        assert_eq!(light["body"]["cursor"], whole["body"]["cursor"]);
        let watching = json!({"provider":"claude","sessionID":"a"});
        let watched = ask(
            json!({"id":3,"method":"GET","path":"/v1/snapshot","scope":"light","session":watching}),
        );
        assert_eq!(
            turns(&watched),
            [("a".into(), true, true), ("b".into(), false, true)]
        );
        // One conversation asked for by itself comes whole, as before.
        let talk = ask(json!({"id":4,"method":"GET","path":"/v1/conversation","session":watching}));
        assert_eq!(turns(&talk), [("a".into(), true, true)]);

        // Changes and a feed's events are shaped the same way.
        let cursor = whole["body"]["cursor"].clone();
        let (_, subscription) = link::answer(
            &cache,
            json!({"id":"feed","method":"GET","path":"/v1/events","cursor":cursor,"scope":"light","session":watching})
                .to_string()
                .as_bytes(),
            &mut |_, _, _| None,
        );
        let link::Subscription::Began(mut feed) = subscription else {
            panic!("no feed")
        };
        link::next_event(&cache, &mut feed).unwrap();
        let mut later = records.to_vec();
        for session in ["a", "b"] {
            let mut next = turn(session);
            next.key = format!("claude/{session}/t2");
            later.push(next);
        }
        cache.lock().unwrap().update(later.iter(), 3_000).unwrap();
        let event: Value =
            serde_json::from_slice(&link::next_event(&cache, &mut feed).unwrap()).unwrap();
        let shaped = json!({"body":event["data"]});
        assert_eq!(
            turns(&shaped),
            [("a".into(), true, true), ("b".into(), false, true)]
        );
        let since = ask(
            json!({"id":5,"method":"GET","path":"/v1/changes","cursor":cursor,"scope":"light"}),
        );
        assert_eq!(
            turns(&since),
            [("a".into(), false, true), ("b".into(), false, true)]
        );
    }
}
