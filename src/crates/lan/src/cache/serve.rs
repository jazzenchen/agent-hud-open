//! What the cache serves a phone: the published head and its content
//! revision, the snapshot copied or encoded, and the changes since a cursor.
use super::SnapshotCache;
use crate::{Cursor, Record, Result};
use serde::Serialize;
use sha2::Digest;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot<R> {
    pub schema_version: u32,
    #[serde(rename = "deviceID")]
    pub device_id: String,
    pub device_name: String,
    pub revision: String,
    pub generated_at_ms: i64,
    pub records: Vec<R>,
    pub cursor: Cursor,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes<R> {
    pub schema_version: u32,
    #[serde(rename = "deviceID")]
    pub device_id: String,
    pub device_name: String,
    pub from_cursor: Cursor,
    pub to_cursor: Cursor,
    pub revision: String,
    pub generated_at_ms: i64,
    pub records: Vec<R>,
    #[serde(rename = "deletedIDs")]
    pub deleted_ids: Vec<String>,
}
/// The snapshot's body as served: the same fields as [`Snapshot`], over the
/// records the cache holds.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotBody<'a, R> {
    schema_version: u32,
    #[serde(rename = "deviceID")]
    device_id: &'a str,
    device_name: &'a str,
    revision: &'a str,
    generated_at_ms: i64,
    records: Vec<&'a R>,
    cursor: &'a Cursor,
}
/// What a phone is told of the published version: its content revision, when
/// it was made and its cursor. Its records are the cache's own.
#[derive(Clone)]
pub struct Head {
    pub revision: String,
    pub generated_at_ms: i64,
    pub cursor: Cursor,
}
impl<R: Record> SnapshotCache<R> {
    /// The published version, if any.
    pub fn head(&self) -> Option<Head> {
        let head = self.head.as_ref()?;
        Some(Head {
            revision: self.revision()?.to_owned(),
            generated_at_ms: head.generated_at_ms,
            cursor: head.cursor.clone(),
        })
    }
    /// The published version's content revision, hashed as the phone writes
    /// the snapshot back, which is what it hashes to check:
    /// {"cursor":…,"records":[…]}, the records in id order.
    fn revision(&self) -> Option<&str> {
        let head = self.head.as_ref()?;
        let revision = self.revision.get_or_init(|| {
            let mut hash = sha2::Sha256::new();
            hash.update(b"{\"cursor\":");
            hash.update(&head.cursor_body);
            hash.update(b",\"records\":[");
            for (index, body) in self.bodies.values().enumerate() {
                if index > 0 {
                    hash.update(b",");
                }
                hash.update(body);
            }
            hash.update(b"]}");
            format!("{:x}", hash.finalize())
        });
        Some(revision)
    }
    /// The published version with copies of its records.
    pub fn snapshot(&self) -> Option<Snapshot<R>> {
        let head = self.head.as_ref()?;
        Some(Snapshot {
            schema_version: 1,
            device_id: self.device.clone(),
            device_name: self.name.clone(),
            revision: self.revision()?.to_owned(),
            generated_at_ms: head.generated_at_ms,
            records: self.previous.values().cloned().collect(),
            cursor: head.cursor.clone(),
        })
    }
    /// The snapshot as served, encoded once per version on first request.
    pub fn body(&mut self) -> Result<Option<Vec<u8>>> {
        if self.body.is_none() {
            let encoded = match (self.head.as_ref(), self.revision()) {
                (Some(head), Some(revision)) => Some(
                    serde_json::to_vec(&SnapshotBody {
                        schema_version: 1,
                        device_id: &self.device,
                        device_name: &self.name,
                        revision,
                        generated_at_ms: head.generated_at_ms,
                        records: self.previous.values().collect(),
                        cursor: &head.cursor,
                    })
                    .map_err(|_| "LAN snapshot encoding")?,
                ),
                _ => None,
            };
            self.body = encoded;
        }
        Ok(self.body.clone())
    }
    pub fn changes(&self, since: &Cursor) -> Option<Changes<R>> {
        let snapshot = self.head.as_ref()?;
        let version = if *since == snapshot.cursor {
            self.version
        } else {
            let base = self
                .bases
                .iter()
                .find(|base| base.base == since.base_version)?;
            let version = base.first.checked_add(since.change_sequence as u64)?;
            if version < self.oldest || version > base.last || version >= self.version {
                return None;
            }
            version
        };
        Some(Changes {
            schema_version: 1,
            device_id: self.device.clone(),
            device_name: self.name.clone(),
            from_cursor: since.clone(),
            to_cursor: snapshot.cursor.clone(),
            revision: self.revision()?.to_owned(),
            generated_at_ms: snapshot.generated_at_ms,
            records: self
                .written
                .iter()
                .filter(|(_, v)| **v > version)
                .filter_map(|(id, _)| self.previous.get(id).cloned())
                .collect(),
            deleted_ids: self
                .deleted
                .iter()
                .filter(|(_, v)| **v > version)
                .map(|(id, _)| id.clone())
                .collect(),
        })
    }
}
