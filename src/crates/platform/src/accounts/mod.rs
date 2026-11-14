//! Provider account readings on one worker, one at a time, with one result
//! cache outside the Runtime mutex. A reading holds the collection gate, so it
//! never overlaps a local pass either.
//! Cadence follows UsageRefresh: 300s quiet, 150s running, >=60s per account.
//!
//! Each provider states its own facts (`agents::account::Provider`): how it
//! is read, whose work spends its windows, what a failure means. Everything
//! here is the same for every provider: when to read, how long a reading
//! that missed an answer keeps the last one, how far apart retries are, and
//! when the vendor's status page is asked.
use agent_hud_core::l10n;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    process::Child,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, PoisonError, Weak,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod credentials;
mod engine;
mod homes;
mod host;
pub(crate) mod http;
mod price_feed;
mod projection;
mod restart_copy;
mod schedule;
mod settle;
mod sign_in;
mod sources;
mod status;
mod web;
mod worker;

pub use sign_in::WebSignInDesk;
use worker::run;

#[derive(Clone)]
pub(super) struct AccountContext {
    pub home: PathBuf,
    pub data_dir: PathBuf,
    pub env: HashMap<String, String>,
    pub account_homes: HashMap<String, Vec<PathBuf>>,
    pub http: Client,
    pub children: Arc<Mutex<Vec<Weak<Mutex<Child>>>>>,
    pub cancelled: Arc<AtomicBool>,
    pub copilot_consent: Arc<AtomicBool>,
    /// The relay addresses the owner agreed to read (`relays::CONSENT`).
    pub relay_consent: Arc<Mutex<HashSet<String>>>,
    /// A web sign-in being tried before it is kept (`WebSignInDesk::accepts`):
    /// the provider and the session, the only one a reading then sees.
    pub trial_web: Option<(String, agents::account::WebSession)>,
}
use agent_hud_agents::account::Reading as AccountReading;

type Waker = Arc<dyn Fn() + Send + Sync>;
use agent_hud_agents as agents;
use agents::account::Provider;
/// The providers a service reads, one at a time, in their order
/// (`agents::ACCOUNTS` outside tests).
type Providers = &'static [&'static Provider];

/// Held for every account reading and every local collection pass, so the two
/// never read at the same time. A host takes it before any lock that UI
/// requests also wait on.
pub type CollectionGate = Arc<Mutex<()>>;

struct Entry {
    reading: AccountReading,
    checked_at: Option<String>,
    attempted: Instant,
    attempted_ms: i64,
    notice: Option<String>,
    /// The last reading failed and is shown so: its rows stale, why under it.
    failed: bool,
    spread: u64,
    /// From the restart copy and not yet read in this run: due at once.
    restored: bool,
    /// Readings that failed in a row, shown or not (within the grace).
    failures: u32,
    /// What the last failed reading was (`Unread::kind`), while it is shown.
    failure: Option<&'static str>,
    /// Since when the readings fail, in ms.
    failing_since: Option<i64>,
    /// Not before this, as the service asked.
    retry_at: Option<Instant>,
}
/// A vendor's status page, as last read.
struct PageReading {
    attempted: Instant,
    checked_at: Option<String>,
    summary: Option<Value>,
}
struct Control {
    stopped: bool,
    force: HashSet<String>,
    enabled: HashSet<String>,
    /// Sources with a turn running that was heard from in the last 300 s.
    running: HashSet<String>,
    /// Sources with a session still in flight, however long it has been
    /// quiet (agent-hud-native-open `LiveSession.isLive(at:)`).
    live: HashSet<String>,
    finished: HashMap<String, i64>,
    entries: BTreeMap<&'static str, Entry>,
    /// Status pages by address.
    pages: HashMap<&'static str, PageReading>,
    external_prices: agent_hud_core::ExternalPrices,
    account_homes: HashMap<String, Vec<PathBuf>>,
    waker: Option<Waker>,
}
struct Shared {
    state: Mutex<Control>,
    wake: Condvar,
}
pub struct AccountService {
    shared: Arc<Shared>,
    context: AccountContext,
    gate: CollectionGate,
}

impl AccountService {
    pub fn new(
        home: PathBuf,
        data_dir: PathBuf,
        enabled: HashSet<String>,
        gate: CollectionGate,
    ) -> Result<Self, String> {
        Self::with_readers(home, data_dir, enabled, gate, agents::ACCOUNTS)
    }
    fn with_readers(
        home: PathBuf,
        data_dir: PathBuf,
        enabled: HashSet<String>,
        gate: CollectionGate,
        readers: Providers,
    ) -> Result<Self, String> {
        // Never follow a redirect carrying credentials to a different origin.
        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .user_agent("AgentHUD/0.1")
            .build()
            .map_err(|_| {
                l10n::text(
                    "账户网络请求无法启动",
                    "Account HTTP client could not start",
                )
            })?;
        let restored = restart_copy::load(&data_dir, readers);
        let entries = restored
            .readings
            .into_iter()
            .map(|(reading, checked_at)| {
                let attempted_ms = date_ms(&checked_at).unwrap_or_default();
                let entry = Entry {
                    notice: reading.notice.clone(),
                    reading,
                    checked_at: Some(checked_at),
                    attempted: Instant::now(),
                    attempted_ms,
                    failed: false,
                    spread: 0,
                    restored: true,
                    failures: 0,
                    failure: None,
                    failing_since: None,
                    retry_at: None,
                };
                (entry.reading.provider, entry)
            })
            .collect();
        let context = AccountContext {
            home,
            data_dir,
            env: std::env::vars().collect(),
            account_homes: HashMap::new(),
            http,
            children: Arc::new(Mutex::new(Vec::new())),
            cancelled: Arc::new(AtomicBool::new(false)),
            copilot_consent: Arc::new(AtomicBool::new(false)),
            relay_consent: Arc::new(Mutex::new(HashSet::new())),
            trial_web: None,
        };
        let (external_prices, price_wait) = price_feed::restore(&context.data_dir);
        let price_at = (!cfg!(test)).then(|| Instant::now() + price_wait);
        let shared = Arc::new(Shared {
            state: Mutex::new(Control {
                stopped: false,
                force: HashSet::new(),
                enabled,
                running: HashSet::new(),
                live: HashSet::new(),
                finished: HashMap::new(),
                entries,
                pages: HashMap::new(),
                external_prices,
                account_homes: restored.account_homes,
                waker: None,
            }),
            wake: Condvar::new(),
        });
        let worker = Arc::clone(&shared);
        let worker_context = context.clone();
        let worker_gate = Arc::clone(&gate);
        std::thread::Builder::new()
            .name("agenthud-accounts".into())
            .spawn(move || run(worker_context, worker, worker_gate, readers, price_at))
            .map_err(|_| l10n::text("账户读取线程无法启动", "Account worker could not start"))?;
        Ok(Self {
            shared,
            context,
            gate,
        })
    }
    pub fn set_waker(&self, waker: Waker) {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .waker = Some(waker);
    }
    pub fn stop(&self) {
        self.context.cancelled.store(true, Ordering::Release);
        let mut copy = None;
        {
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            // The next launch starts from what the app shows now.
            if !state.stopped {
                copy = Some(restart_copy::copy(&state.entries, &state.account_homes));
            }
            state.stopped = true;
            self.shared.wake.notify_all();
        }
        if let Some(copy) = copy {
            restart_copy::save(&self.context.data_dir, &copy);
        }
        let children = self
            .context
            .children
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter_map(Weak::upgrade)
            .collect::<Vec<_>>();
        for child in children {
            let _ = child.lock().unwrap_or_else(PoisonError::into_inner).kill();
        }
    }
    pub fn refresh(&self) {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let enabled = state.enabled.clone();
        state.force.extend(enabled);
        self.shared.wake.notify_all();
    }
}
impl Drop for AccountService {
    fn drop(&mut self) {
        self.stop();
    }
}

/// How long a window the provider stopped reporting keeps its last reading
/// (agent-hud-native-open QuotaHistoryStore.retention).
const WINDOW_RETENTION_MS: i64 = 30 * 86_400_000;

fn date_ms(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.timestamp_millis())
}

#[cfg(test)]
mod tests;
