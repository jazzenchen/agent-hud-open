use super::*;
use std::{
    cell::OnceCell,
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, PoisonError},
    time::Duration,
};
mod serve;
pub use serve::{Changes, Head, Snapshot};
/// Counts the cache's commits so the event streams wait for the next one
/// instead of polling it.
#[derive(Default)]
pub struct Signal {
    count: Mutex<u64>,
    moved: Condvar,
}
impl Signal {
    pub fn bump(&self) {
        *self.count.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        self.moved.notify_all();
    }
    pub fn current(&self) -> u64 {
        *self.count.lock().unwrap_or_else(PoisonError::into_inner)
    }
    /// Waits until the count passes `seen` or `timeout` runs out, and returns
    /// the count then.
    pub fn wait(&self, seen: u64, timeout: Duration) -> u64 {
        let count = self.count.lock().unwrap_or_else(PoisonError::into_inner);
        let (count, _) = self
            .moved
            .wait_timeout_while(count, timeout, |count| *count == seen)
            .unwrap_or_else(PoisonError::into_inner);
        *count
    }
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
        let cursor = self.next_cursor(now, &named);
        self.previous = named;
        if let Err(error) = self.commit(&changed, &deleted, cursor, now) {
            // Until the version is committed the published records stay as they
            // were, so a failed commit is found changed again on the next update.
            let mut named = std::mem::take(&mut self.previous);
            for id in &changed {
                match replaced.remove(id) {
                    Some(old) => named.insert(id.clone(), old),
                    None => named.remove(id),
                };
            }
            named.append(&mut previous);
            self.previous = named;
            return Err(error);
        }
        Ok(true)
    }
    /// The cursor of the next version over `named`: the next change on the
    /// published base, or a new base.
    fn next_cursor(&self, now: i64, named: &BTreeMap<String, R>) -> Cursor {
        match &self.head {
            Some(head) => Cursor {
                change_sequence: head.cursor.change_sequence + 1,
                ..head.cursor.clone()
            },
            None => self.next_baseline(now, named),
        }
    }
    /// A new major version over the published records, for the next business
    /// upload; returned with copies of them.
    pub fn baseline(&mut self, now: i64) -> Result<Option<Snapshot<R>>> {
        if self.head.is_none() {
            return Ok(None);
        }
        let cursor = self.next_baseline(now, &self.previous);
        self.commit(&[], &[], cursor, now)?;
        Ok(self.snapshot())
    }
    fn next_baseline(&self, now: i64, records: &BTreeMap<String, R>) -> Cursor {
        let prior = self
            .persisted
            .as_ref()
            .and_then(|c| chrono::DateTime::parse_from_rfc3339(&c.base_version).ok())
            .map(|t| t.timestamp_millis())
            .unwrap_or(0);
        Cursor::new(
            now.max(prior + 1).max(
                records
                    .values()
                    .map(|r| r.modified_at_ms())
                    .max()
                    .unwrap_or(0),
            ),
            0,
        )
        .expect("a cursor at a clock time")
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
        let cursor_body = crate::canonical::to_vec(&json!(cursor));
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
mod tests;
