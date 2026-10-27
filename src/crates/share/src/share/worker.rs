//! The share's thread. It starts and stops the LAN link with the page's
//! switch, projects each snapshot into records, reads the ledger's usage at
//! most every 30 seconds once it has moved, and publishes what changed.
use super::{settings::Settings, Command, Query};
use crate::{keys, projection, SyncRecord};
use agent_hud_lan::{
    cache::SnapshotCache,
    credentials::{Credentials, SecretStore},
    identity::Identity,
    server::LanServer,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

const USAGE_EVERY_MS: i64 = 30_000;
const ADDRESS_EVERY_MS: i64 = 30_000;
/// Usage goes out for the last 30 days, as every other copy of it does.
const RETENTION_MS: i64 = 30 * keys::DAY_MS;

pub(super) struct Worker {
    directory: PathBuf,
    query: Query,
    secrets: Option<Arc<dyn SecretStore>>,
    settings: Settings,
    name: String,
    cache: Arc<Mutex<SnapshotCache<SyncRecord>>>,
    lan: Option<LanServer<SyncRecord>>,
    pub(super) records: BTreeMap<String, SyncRecord>,
    snapshot: Option<Arc<Value>>,
    /// What the readings last published said was read (`reading_facts`).
    readings: Option<Value>,
    usage_revision: Option<u64>,
    usage_seeded: bool,
    usage_tried_at: i64,
    address_at: i64,
    error: Option<String>,
}
impl Worker {
    pub fn new(directory: PathBuf, query: Query) -> Result<Self, String> {
        Self::with_secrets(directory, query, super::os::secrets())
    }
    pub(super) fn with_secrets(
        directory: PathBuf,
        query: Query,
        secrets: Option<Arc<dyn SecretStore>>,
    ) -> Result<Self, String> {
        let settings = Settings::load(&directory)?;
        let name = super::computer_name();
        let cache = SnapshotCache::new(
            settings.device_id.clone(),
            name.clone(),
            directory.join("cursor.json"),
        )?;
        Ok(Self {
            directory,
            query,
            secrets,
            settings,
            name,
            cache: Arc::new(Mutex::new(cache)),
            lan: None,
            records: BTreeMap::new(),
            snapshot: None,
            readings: None,
            usage_revision: None,
            usage_seeded: false,
            usage_tried_at: 0,
            address_at: 0,
            error: None,
        })
    }
    pub fn run(&mut self, inbox: mpsc::Receiver<Command>) {
        if self.settings.enabled {
            if let Err(error) = self.start_lan() {
                self.error = Some(error);
            }
        }
        loop {
            match inbox.recv_timeout(Duration::from_secs(5)) {
                Ok(Command::Snapshot(snapshot)) => self.snapshot = Some(snapshot),
                Ok(Command::Request {
                    method,
                    params,
                    reply,
                }) => {
                    let _ = reply.send(self.request(&method, params, now()));
                }
                Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if let Err(error) = self.pass(now()) {
                self.error = Some(error);
            }
        }
        if let Some(mut lan) = self.lan.take() {
            let _ = lan.stop(false);
        }
    }
    #[cfg(test)]
    pub(super) fn observe_for_test(&mut self, snapshot: Arc<Value>) {
        self.snapshot = Some(snapshot);
    }
    fn start_lan(&mut self) -> Result<(), String> {
        if self.lan.is_some() {
            return Ok(());
        }
        let secrets = self
            .secrets
            .clone()
            .ok_or("Sharing with phones is not available on this system yet.")?;
        let device = self.settings.device_id.clone();
        let identity = Identity::load(&device, &self.directory)?;
        let credentials = Credentials::new(&device, self.directory.clone(), secrets)?;
        let mut lan = LanServer::new(
            device,
            self.name.clone(),
            credentials,
            self.cache.clone(),
            identity,
            // Phones only read here: no route beyond the link's own.
            Arc::new(|_, _| Ok(None)),
        );
        let port = lan.start(self.settings.port)?;
        if self.settings.port != port {
            self.settings.port = port;
            self.settings.save(&self.directory)?;
        }
        self.lan = Some(lan);
        self.address_at = 0;
        Ok(())
    }
    /// Turning sharing off disconnects the phones and revokes their grants.
    fn stop_lan(&mut self) -> Result<(), String> {
        match self.lan.take() {
            Some(mut lan) => lan.stop(true),
            None => Credentials::revoke_saved(&self.directory),
        }
    }
    pub(super) fn request(
        &mut self,
        method: &str,
        params: Value,
        now: i64,
    ) -> Result<Value, String> {
        match method {
            "status" => Ok(self.status()),
            "set_enabled" => {
                let enabled = params["enabled"]
                    .as_bool()
                    .ok_or("enabled must be a boolean")?;
                if enabled {
                    self.start_lan()?;
                } else {
                    self.stop_lan()?;
                }
                self.settings.enabled = enabled;
                self.settings.save(&self.directory)?;
                self.error = None;
                Ok(self.status())
            }
            "pair_invitation" => self
                .lan
                .as_ref()
                .ok_or("Turn on sharing with phones before pairing")?
                .invitation(now),
            "revoke_clients" => {
                if let Some(lan) = &self.lan {
                    lan.revoke()?;
                }
                Ok(self.status())
            }
            _ => Err(format!("Unknown phone sharing request: {method}")),
        }
    }
    fn status(&self) -> Value {
        let mut status = self
            .lan
            .as_ref()
            .map(LanServer::status)
            .unwrap_or_else(|| json!({"enabled": false, "pairedClients": 0, "peers": []}));
        status["supported"] = json!(self.secrets.is_some());
        status["on"] = json!(self.settings.enabled);
        status["name"] = json!(self.name);
        status["error"] = json!(self.error);
        status
    }
    fn pass(&mut self, now: i64) -> Result<(), String> {
        let Some(lan) = &self.lan else {
            return Ok(());
        };
        if now - self.address_at >= ADDRESS_EVERY_MS {
            self.address_at = now;
            lan.refresh_address();
        }
        self.project(now)?;
        self.cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .update(self.records.values(), now)?;
        self.error = None;
        Ok(())
    }
    /// The records as this pass has them: this computer, the readings when
    /// what was read changed, and usage when the ledger moved.
    pub(super) fn project(&mut self, now: i64) -> Result<(), String> {
        let device = projection::device(&self.settings.device_id, &self.name, now);
        self.records.entry(device.id()).or_insert(device);
        if let Some(snapshot) = self.snapshot.clone() {
            self.project_readings(&snapshot, now);
            self.project_usage(&snapshot, now)?;
        }
        let cutoff = keys::day_start(now - RETENTION_MS);
        self.records
            .retain(|_, r| r.is_visible(now) && (r.kind != "usage" || usage_day(r) >= cutoff));
        Ok(())
    }
    fn project_readings(&mut self, snapshot: &Value, now: i64) {
        let record = projection::readings(&self.settings.device_id, snapshot, now);
        let empty = record.payload["providers"]
            .as_array()
            .is_none_or(Vec::is_empty);
        // Accounts still being read would otherwise go out as none at all.
        if empty && projection::accounts_loading(snapshot) {
            return;
        }
        let facts = projection::reading_facts(&record.payload, now);
        if self.readings.as_ref() != Some(&facts) {
            self.readings = Some(facts);
            self.records.insert(record.id(), record);
        }
    }
    fn project_usage(&mut self, snapshot: &Value, now: i64) -> Result<(), String> {
        let revision = snapshot["statisticsRevision"].as_u64();
        if revision.is_some() && revision == self.usage_revision {
            return Ok(());
        }
        if self.usage_tried_at != 0 && now - self.usage_tried_at < USAGE_EVERY_MS {
            return Ok(());
        }
        self.usage_tried_at = now;
        // The first read covers every day kept; later ones yesterday and today.
        let full = !self.usage_seeded;
        let retention_start = now - RETENTION_MS;
        let window_start = if full {
            retention_start
        } else {
            (keys::day_start(now) - keys::DAY_MS).max(retention_start)
        };
        let facts = (self.query)(
            "usage_buckets",
            json!({"from":rfc3339(window_start)?,"to":rfc3339(now)?,"bucketMinutes":15}),
        )?;
        // Until indexing can replace whole days, the days already sent stay.
        if facts["coverage"]["complete"] != true {
            return Ok(());
        }
        let records = projection::usage(&self.settings.device_id, &facts, now)?;
        let window_day = keys::day_start(window_start);
        self.records.retain(|_, r| match r.kind.as_str() {
            "usage" => !full && usage_day(r) < window_day,
            // Names stay for the days not read again.
            "consumer" => !full,
            _ => true,
        });
        for record in records {
            self.records.insert(record.id(), record);
        }
        self.usage_seeded = true;
        self.usage_revision = revision;
        Ok(())
    }
}
/// The day a usage record covers: it expires 31 days after that day begins.
fn usage_day(record: &SyncRecord) -> i64 {
    record
        .expires_at_ms
        .map(|end| end - 31 * keys::DAY_MS)
        .unwrap_or(0)
}
fn rfc3339(ms: i64) -> Result<String, String> {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .map(|at| at.to_rfc3339())
        .ok_or_else(|| "Usage range".to_owned())
}
fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
