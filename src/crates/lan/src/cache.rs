use super::*;
use sha2::Digest;
use std::{
    cell::OnceCell,
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};
/// Counts the cache's commits so the event streams wait for the next one
/// instead of polling it.
#[derive(Default)]
pub struct Signal {
    count: Mutex<u64>,
    moved: Condvar,
}
impl Signal {
    pub fn bump(&self) {
        *self.count.lock().unwrap_or_else(|p| p.into_inner()) += 1;
        self.moved.notify_all();
    }
    pub fn current(&self) -> u64 {
        *self.count.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// Waits until the count passes `seen` or `timeout` runs out, and returns
    /// the count then.
    pub fn wait(&self, seen: u64, timeout: Duration) -> u64 {
        let count = self.count.lock().unwrap_or_else(|p| p.into_inner());
        let (count, _) = self
            .moved
            .wait_timeout_while(count, timeout, |count| *count == seen)
            .unwrap_or_else(|p| p.into_inner());
        *count
    }
}
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
/// The published version as the cache keeps it, with its cursor in the
/// phone's JSON form, which the content revision hashes.
struct Published {
    generated_at_ms: i64,
    cursor: Cursor,
    cursor_body: Vec<u8>,
}
struct Base {
    base: String,
    first: u64,
    last: u64,
}
pub struct SnapshotCache<R> {
    device: String,
    name: String,
    file: PathBuf,
    persisted: Option<Cursor>,
    head: Option<Published>,
    /// The published version's content revision, hashed the first time it is
    /// asked for after each commit. It covers every record's body, so its cost
    /// grows with the records, and a version no phone looks at is not hashed.
    revision: OnceCell<String>,
    /// The published records by id, held once.
    previous: BTreeMap<String, R>,
    /// Each published record in the phone's JSON form, encoded when it changed;
    /// the content revision hashes these (native `LocalNetworkSnapshotProjection`
    /// kept the same per-record bodies).
    bodies: BTreeMap<String, Vec<u8>>,
    written: BTreeMap<String, u64>,
    deleted: BTreeMap<String, u64>,
    bases: Vec<Base>,
    checkpoints: VecDeque<(u64, i64)>,
    version: u64,
    oldest: u64,
    body: Option<Vec<u8>>,
    signal: Arc<Signal>,
}
impl<R: Record> SnapshotCache<R> {
    pub fn new(device: String, name: String, file: PathBuf) -> Result<Self> {
        if !file.is_absolute() {
            return Err("Explicit LAN cursor path required".into());
        }
        let persisted = match std::fs::read(&file) {
            Ok(bytes) => {
                let value: Cursor =
                    serde_json::from_slice(&bytes).map_err(|_| "Invalid saved LAN cursor")?;
                if !value.valid() {
                    return Err("Invalid saved LAN cursor".into());
                }
                Some(value)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err("Cannot read LAN cursor".into()),
        };
        Ok(Self {
            device,
            name,
            file,
            persisted,
            head: None,
            revision: OnceCell::new(),
            previous: BTreeMap::new(),
            bodies: BTreeMap::new(),
            written: BTreeMap::new(),
            deleted: BTreeMap::new(),
            bases: Vec::new(),
            checkpoints: VecDeque::new(),
            version: 0,
            oldest: 0,
            body: None,
            signal: Arc::default(),
        })
    }
    /// Moves on every commit and clear.
    pub fn signal(&self) -> Arc<Signal> {
        self.signal.clone()
    }
    /// Publishes `records` as the next version when any of them differs from
    /// what is published. An unchanged record keeps the copy, JSON and
    /// modification time already published; only new and changed ones are
    /// copied and encoded.
    pub fn update<'a>(&mut self, records: impl IntoIterator<Item = &'a R>, now: i64) -> Result<bool>
    where
        R: 'a,
    {
        let mut previous = std::mem::take(&mut self.previous);
        let mut named = BTreeMap::new();
        let mut changed = Vec::new();
        // What a changed record replaced, to put back should the commit fail.
        let mut replaced = BTreeMap::new();
        for record in records
            .into_iter()
            .filter(|r| r.device_id() == self.device && r.is_visible(now))
        {
            let id = record.id();
            match previous.remove(&id) {
                Some(old) if old.same_content(record) => {
                    named.insert(id, old);
                }
                old => {
                    if let Some(old) = old {
                        replaced.insert(id.clone(), old);
                    }
                    changed.push(id.clone());
                    named.insert(id, record.clone());
                }
            }
        }
        // Records not named again are deleted.
        let deleted: Vec<String> = previous.keys().cloned().collect();
        if self.head.is_some() && changed.is_empty() && deleted.is_empty() {
            self.previous = named;
            return Ok(false);
        }
        // Until the version is committed the published records stay as they
        // were, so a failed commit is found changed again on the next update.
        let mut restore = |cache: &mut Self, mut named: BTreeMap<String, R>| {
            for id in &changed {
                match replaced.remove(id) {
                    Some(old) => named.insert(id.clone(), old),
                    None => named.remove(id),
                };
            }
            named.append(&mut previous);
            cache.previous = named;
        };
        let cursor = match self.next_cursor(now, &named) {
            Ok(cursor) => cursor,
            Err(error) => {
                restore(self, named);
                return Err(error);
            }
        };
        self.previous = named;
        if let Err(error) = self.commit(&changed, &deleted, cursor, now) {
            let named = std::mem::take(&mut self.previous);
            restore(self, named);
            return Err(error);
        }
        Ok(true)
    }
    /// The cursor of the next version over `named`: the next change on the
    /// published base, or a new base.
    fn next_cursor(&self, now: i64, named: &BTreeMap<String, R>) -> Result<Cursor> {
        Ok(if let Some(head) = &self.head {
            let mut cursor = head.cursor.clone();
            cursor.change_sequence = cursor
                .change_sequence
                .checked_add(1)
                .ok_or("LAN cursor exhausted")?;
            cursor
        } else {
            self.next_baseline(now, named)?
        })
    }
    /// A new major version over the published records, for the next business
    /// upload; returned with copies of them.
    pub fn baseline(&mut self, now: i64) -> Result<Option<Snapshot<R>>> {
        if self.head.is_none() {
            return Ok(None);
        }
        let cursor = self.next_baseline(now, &self.previous)?;
        self.commit(&[], &[], cursor, now)?;
        Ok(self.snapshot())
    }
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
    fn next_baseline(&self, now: i64, records: &BTreeMap<String, R>) -> Result<Cursor> {
        let prior = self
            .persisted
            .as_ref()
            .and_then(|c| chrono::DateTime::parse_from_rfc3339(&c.base_version).ok())
            .map(|t| t.timestamp_millis())
            .unwrap_or(0);
        let next = prior.checked_add(1).ok_or("LAN cursor exhausted")?;
        Cursor::new(
            now.max(next).max(
                records
                    .values()
                    .map(|r| r.modified_at_ms())
                    .max()
                    .unwrap_or(0),
            ),
            0,
        )
        .ok_or_else(|| "LAN cursor timestamp invalid".into())
    }
    /// Commits the records now in `previous` as a version at `cursor`, where
    /// `changed` and `deleted` name what differs from the last one.
    fn commit(
        &mut self,
        changed: &[String],
        deleted: &[String],
        cursor: Cursor,
        now: i64,
    ) -> Result<()> {
        // Everything that can fail runs before any state moves.
        let mut bodies = Vec::with_capacity(changed.len());
        for id in changed {
            if let Some(record) = self.previous.get(id) {
                let value = serde_json::to_value(record).map_err(|_| "LAN record encoding")?;
                bodies.push((id.clone(), crate::canonical::to_vec(&value)));
            }
        }
        let cursor_value = serde_json::to_value(&cursor).map_err(|_| "LAN cursor encoding")?;
        let cursor_body = crate::canonical::to_vec(&cursor_value);
        crate::fs::atomic_json(&self.file, &cursor).map_err(|e| e.to_string())?;
        self.bodies.extend(bodies);
        for id in deleted {
            self.bodies.remove(id);
        }
        self.persisted = Some(cursor.clone());
        self.version += 1;
        for id in changed {
            self.written.insert(id.clone(), self.version);
            self.deleted.remove(id);
        }
        for id in deleted {
            self.written.remove(id);
            self.deleted.insert(id.clone(), self.version);
        }
        if let Some(base) = self
            .bases
            .last_mut()
            .filter(|base| base.base == cursor.base_version)
        {
            base.last = self.version;
        } else {
            self.bases.push(Base {
                base: cursor.base_version.clone(),
                first: self.version - cursor.change_sequence as u64,
                last: self.version,
            });
        }
        if self
            .checkpoints
            .back()
            .is_none_or(|(_, at)| now - *at >= 3_600_000)
        {
            self.checkpoints.push_back((self.version, now));
        }
        while self
            .checkpoints
            .front()
            .is_some_and(|(_, at)| *at <= now - 86_400_000)
        {
            let (version, _) = self.checkpoints.pop_front().unwrap();
            self.oldest = self.oldest.max(version + 1);
        }
        self.deleted.retain(|_, v| *v > self.oldest);
        self.bases.retain(|base| base.last >= self.oldest);
        self.head = Some(Published {
            generated_at_ms: now,
            cursor,
            cursor_body,
        });
        self.revision = OnceCell::new();
        self.body = None;
        self.signal.bump();
        Ok(())
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
        if !since.valid() {
            return None;
        }
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
    pub fn clear(&mut self) {
        self.head = None;
        self.revision = OnceCell::new();
        self.previous.clear();
        self.bodies.clear();
        self.written.clear();
        self.deleted.clear();
        self.bases.clear();
        self.checkpoints.clear();
        self.body = None;
        self.signal.bump();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Serialize)]
    struct Item {
        key: String,
        value: f64,
        at: i64,
    }
    impl Record for Item {
        fn id(&self) -> String {
            self.key.clone()
        }
        fn device_id(&self) -> &str {
            "mac"
        }
        fn modified_at_ms(&self) -> i64 {
            self.at
        }
        fn is_visible(&self, _: i64) -> bool {
            true
        }
        fn content(&self) -> String {
            format!("{}={}", self.key, self.value)
        }
    }
    fn item(key: &str, value: f64, at: i64) -> Item {
        Item {
            key: key.into(),
            value,
            at,
        }
    }
    /// The revision as the phone checks it: its JSON form of the whole snapshot.
    fn whole(cache: &SnapshotCache<Item>) -> String {
        let snapshot = cache.snapshot().unwrap();
        let value = json!({"cursor": snapshot.cursor, "records": snapshot.records});
        format!(
            "{:x}",
            sha2::Sha256::digest(crate::canonical::to_vec(&value))
        )
    }

    #[test]
    fn a_version_carries_only_its_changes_and_the_revision_of_the_whole() {
        let folder = std::env::temp_dir().join(format!("lan-cache-{}", std::process::id()));
        crate::fs::private_dir(&folder).unwrap();
        let mut cache =
            SnapshotCache::<Item>::new("mac".into(), "Mac".into(), folder.join("cursor.json"))
                .unwrap();
        let first = [
            item("a", 1.0, 10),
            item("b", 0.00001, 10),
            item("c", 3.5, 10),
        ];
        assert!(cache.update(&first, 1_000).unwrap());
        let start = cache.head().unwrap().cursor.clone();
        assert_eq!(cache.head().unwrap().revision, whole(&cache));

        // The same content with a later time publishes nothing.
        let later = [
            item("a", 1.0, 20),
            item("b", 0.00001, 20),
            item("c", 3.5, 20),
        ];
        assert!(!cache.update(&later, 2_000).unwrap());
        assert_eq!(cache.snapshot().unwrap().records[0].at, 10);

        // One change and one deletion make the next minor version.
        let next = [item("a", 2.0, 30), item("b", 0.00001, 30)];
        assert!(cache.update(&next, 3_000).unwrap());
        let head = cache.head().unwrap().clone();
        assert_eq!(head.cursor.base_version, start.base_version);
        assert_eq!(head.cursor.change_sequence, 1);
        assert_eq!(head.revision, whole(&cache));
        let changes = cache.changes(&start).unwrap();
        assert_eq!(changes.records, vec![item("a", 2.0, 30)]);
        assert_eq!(changes.deleted_ids, vec!["c".to_string()]);

        // A baseline is a new major version over the same records.
        let baseline = cache.baseline(4_000).unwrap().unwrap();
        assert_eq!(baseline.cursor.change_sequence, 0);
        assert_ne!(baseline.cursor.base_version, start.base_version);
        assert_eq!(cache.head().unwrap().revision, whole(&cache));
        let served: Value = serde_json::from_slice(&cache.body().unwrap().unwrap()).unwrap();
        assert_eq!(
            served,
            serde_json::to_value(cache.snapshot().unwrap()).unwrap()
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_version_that_could_not_be_saved_goes_out_on_the_next_update() {
        let folder = std::env::temp_dir().join(format!("lan-unsaved-{}", std::process::id()));
        crate::fs::private_dir(&folder).unwrap();
        let file = folder.join("cursor.json");
        let mut cache =
            SnapshotCache::<Item>::new("mac".into(), "Mac".into(), file.clone()).unwrap();
        assert!(cache
            .update(&[item("a", 1.0, 10), item("c", 3.0, 10)], 1_000)
            .unwrap());
        let start = cache.head().unwrap();
        // A folder where the cursor goes makes saving it fail.
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir_all(file.join("blocked")).unwrap(); // plain directory
        let next = [item("a", 2.0, 20)];
        assert!(cache.update(&next, 2_000).is_err());
        // What is served is still the saved version, whole.
        let head = cache.head().unwrap();
        assert_eq!(head.cursor, start.cursor);
        assert_eq!(head.revision, start.revision);
        assert_eq!(head.revision, whole(&cache));
        assert_eq!(
            cache.snapshot().unwrap().records,
            vec![item("a", 1.0, 10), item("c", 3.0, 10)]
        );
        // Once saving works, the same update publishes the change.
        std::fs::remove_dir_all(&file).unwrap();
        assert!(cache.update(&next, 3_000).unwrap());
        let changes = cache.changes(&start.cursor).unwrap();
        assert_eq!(changes.records, vec![item("a", 2.0, 20)]);
        assert_eq!(changes.deleted_ids, vec!["c".to_string()]);
        assert_eq!(cache.head().unwrap().revision, whole(&cache));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_revision_hashed_only_when_asked_is_still_the_whole_ones() {
        // Commits that nobody reads in between are hashed once, when asked,
        // and the answer is the latest version's, never an earlier one's.
        let folder = std::env::temp_dir().join(format!("lan-lazy-{}", std::process::id()));
        crate::fs::private_dir(&folder).unwrap();
        let mut cache =
            SnapshotCache::<Item>::new("mac".into(), "Mac".into(), folder.join("cursor.json"))
                .unwrap();
        assert!(cache.head().is_none());
        assert!(cache.update(&[item("a", 1.0, 10)], 1_000).unwrap());
        let first = cache.head().unwrap().revision;
        assert!(cache.update(&[item("a", 2.0, 20)], 2_000).unwrap());
        assert!(cache
            .update(&[item("a", 2.0, 20), item("b", 1.0, 20)], 3_000)
            .unwrap());
        let latest = cache.head().unwrap();
        assert_ne!(latest.revision, first);
        assert_eq!(latest.revision, whole(&cache));
        assert_eq!(latest.cursor.change_sequence, 2);
        let served: Value = serde_json::from_slice(&cache.body().unwrap().unwrap()).unwrap();
        assert_eq!(served["revision"], latest.revision.as_str());
        // A change set names the same revision as the head.
        let changes = cache.changes(&Cursor {
            change_sequence: 0,
            ..latest.cursor.clone()
        });
        assert_eq!(changes.unwrap().revision, latest.revision);
        cache.clear();
        assert!(cache.head().is_none());
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
