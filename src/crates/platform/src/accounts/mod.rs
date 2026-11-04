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
use agent_hud_core::AccountProjection;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    process::Child,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, Weak,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod credentials;
mod engine;
mod homes;
mod host;
pub(crate) mod http;
mod restart_copy;
mod status;
mod web;

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
use agents::account::{Provider, Unread};
/// The providers a service reads, one at a time, in their order
/// (`agents::ACCOUNTS` outside tests).
type Providers = &'static [&'static Provider];

/// A provider's own facts, by id. A provider that states none (a test's
/// fixture) is read like one whose windows this computer's work moves.
fn facts(provider: &str) -> Option<&'static Provider> {
    agents::account_provider(provider)
}
/// Whether this computer's work moves the provider's windows.
fn sees_local_work(provider: &str) -> bool {
    facts(provider).is_none_or(|facts| facts.local_work)
}
/// The session sources whose work is the provider's.
fn work_sources(provider: &str) -> &'static [&'static str] {
    facts(provider).map_or(&[], Provider::sources)
}

/// How long a reading that missed an answer (`Unread::Transient`) keeps the
/// last good one on screen as it was, without greying it or saying why:
/// a network hiccup or a busy service passes by itself.
const GRACE_MS: i64 = 10 * 60_000;
/// Retries after failed readings wait 60 s, then twice as long each time,
/// up to this.
const MOST_BACKOFF: Duration = Duration::from_secs(15 * 60);
/// The longest wait a service's `Retry-After` is followed for.
const MOST_RETRY_AFTER: Duration = Duration::from_secs(3600);

/// How long after a failed reading the next one waits, by how many failed in
/// a row.
fn backoff(failures: u32) -> Duration {
    Duration::from_secs(60)
        .saturating_mul(1u32 << failures.saturating_sub(1).min(10))
        .min(MOST_BACKOFF)
}
/// UsageRefresh.accountStepBudget: after this long of back-to-back readings the
/// worker waits as long again, so a local pass is not held behind the queue.
const STEP_BUDGET: Duration = Duration::from_secs(1);

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
/// What the worker does next: read a provider's account, or a status page.
enum Job {
    Account(
        &'static str,
        &'static Provider,
        HashMap<String, Vec<PathBuf>>,
    ),
    Page(&'static agents::account::StatusPage),
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
    account_homes: HashMap<String, Vec<PathBuf>>,
    waker: Option<Waker>,
}
struct Shared {
    state: Mutex<Control>,
    wake: Condvar,
    children: Arc<Mutex<Vec<Weak<Mutex<Child>>>>>,
    cancelled: Arc<AtomicBool>,
}
pub struct AccountService {
    shared: Arc<Shared>,
    context: AccountContext,
    credential_lock: Mutex<()>,
    gate: CollectionGate,
}

/// The sign-in window's side of a web sign-in (`agents::account::WebSignIn`),
/// which the host holds while the window is open. A session the window
/// collects is tried with the provider's own reader, in the collection
/// queue, and kept only once that reader reads the quota with it.
#[derive(Clone)]
pub struct WebSignInDesk {
    context: AccountContext,
    shared: Arc<Shared>,
    gate: CollectionGate,
}

impl WebSignInDesk {
    /// The page to open and the name to show for `provider` on `host`.
    pub fn site(&self, provider: &str, host: &str) -> Option<(&'static str, &'static str)> {
        web::site(provider, host).map(|site| (site.sign_in, site.name))
    }
    /// The browser identity the window presents and the session is kept with.
    pub fn user_agent(&self) -> &'static str {
        agents::account::WEB_USER_AGENT
    }
    /// Tries the cookies the window holds for `host` as `provider`'s session
    /// and keeps them once the provider's reader reads the quota with them:
    /// true when the sign-in is done.
    pub fn sign_in_with(&self, provider: &str, host: &str, cookie: &str) -> Result<bool, String> {
        let session = agents::account::WebSession {
            host: host.to_owned(),
            cookie: cookie.to_owned(),
            user_agent: agents::account::WEB_USER_AGENT.to_owned(),
        };
        if !self.accepts(provider, &session) {
            return Ok(false);
        }
        self.keep(provider, &session)?;
        Ok(true)
    }
    /// Whether the provider's reader reads its quota with `session` and no
    /// other: the sign-in is done once it does.
    fn accepts(&self, provider: &str, session: &agents::account::WebSession) -> bool {
        let Some(facts) = agents::account_provider(provider) else {
            return false;
        };
        if web::site(provider, &session.host).is_none() {
            return false;
        }
        let mut context = self.context.clone();
        context.trial_web = Some((provider.to_owned(), session.clone()));
        let _turn = self
            .gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (facts.read)(&context).is_ok_and(|reading| reading.account.is_some())
    }
    /// Keeps `session` for `provider` and reads the provider again now.
    fn keep(&self, provider: &str, session: &agents::account::WebSession) -> Result<(), String> {
        web::save(&self.context, provider, session)?;
        wake_provider(&self.shared, provider);
        Ok(())
    }
}

/// Reads `provider` again at once and tells the snapshot.
fn wake_provider(shared: &Shared, provider: &str) {
    if let Ok(mut state) = shared.state.lock() {
        state.entries.remove(provider);
        state.force.insert(provider.into());
    }
    shared.wake.notify_all();
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
        let children = Arc::new(Mutex::new(Vec::new()));
        let cancelled = Arc::new(AtomicBool::new(false));
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
            children: Arc::clone(&children),
            cancelled: Arc::clone(&cancelled),
            copilot_consent: Arc::new(AtomicBool::new(false)),
            relay_consent: Arc::new(Mutex::new(HashSet::new())),
            trial_web: None,
        };
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
                account_homes: restored.account_homes,
                waker: None,
            }),
            wake: Condvar::new(),
            children,
            cancelled,
        });
        let worker = Arc::clone(&shared);
        let worker_context = context.clone();
        let worker_gate = Arc::clone(&gate);
        std::thread::Builder::new()
            .name("agenthud-accounts".into())
            .spawn(move || run(worker_context, worker, worker_gate, readers))
            .map_err(|_| l10n::text("账户读取线程无法启动", "Account worker could not start"))?;
        Ok(Self {
            shared,
            context,
            credential_lock: Mutex::new(()),
            gate,
        })
    }
    /// What the host's sign-in window works with (`WebSignInDesk`).
    pub fn web_sign_in_desk(&self) -> WebSignInDesk {
        WebSignInDesk {
            context: self.context.clone(),
            shared: Arc::clone(&self.shared),
            gate: Arc::clone(&self.gate),
        }
    }
    /// Forgets the web sign-in of `provider` on `host` and reads it again.
    pub fn web_sign_out(&self, params: &Value) -> Result<Value, String> {
        let provider = params["provider"].as_str().ok_or("provider is required")?;
        let host = params["host"].as_str().ok_or("host is required")?;
        web::remove(&self.context, provider, host)?;
        wake_provider(&self.shared, provider);
        Ok(json!({"kind":"ack","provider":provider,"host":host,"signedIn":false}))
    }
    pub fn credentials_status(&self) -> Value {
        credentials::status(&self.context)
    }
    pub fn validate_account_homes(&self, value: &Value) -> Result<(), String> {
        homes::paths(&self.context.home, value).map(|_| ())
    }
    pub fn account_changed(&self, provider: &str) {
        if let Ok(mut state) = self.shared.state.lock() {
            if state.enabled.contains(provider) {
                state.force.insert(provider.into());
                self.shared.wake.notify_all();
            }
        }
    }
    pub fn set_credentials(&self, params: &Value) -> Result<Value, String> {
        let _guard = self.credential_lock.lock().map_err(|_| {
            l10n::text(
                "凭据存储暂时不可用",
                "Credential storage lock is unavailable",
            )
        })?;
        let result = credentials::save(&self.context, params)?;
        self.credentials_changed(params);
        Ok(result)
    }
    /// Adds a relay by hand (`credentials::add_relay_source`) and reads it.
    pub fn add_relay_source(&self, params: &Value) -> Result<Value, String> {
        let _guard = self.credential_lock.lock().map_err(|_| {
            l10n::text(
                "凭据存储暂时不可用",
                "Credential storage lock is unavailable",
            )
        })?;
        let result = credentials::add_relay_source(&self.context, params)?;
        self.account_changed("relay");
        Ok(result)
    }
    /// Keeps the template chosen for a relay's address
    /// (`credentials::set_relay_kind`) and reads the relays again.
    pub fn set_relay_kind(&self, params: &Value) -> Result<Value, String> {
        let _guard = self.credential_lock.lock().map_err(|_| {
            l10n::text(
                "凭据存储暂时不可用",
                "Credential storage lock is unavailable",
            )
        })?;
        let result = credentials::set_relay_kind(&self.context, params)?;
        self.account_changed("relay");
        Ok(result)
    }
    /// Removes a relay added by hand.
    pub fn remove_relay_source(&self, params: &Value) -> Result<Value, String> {
        let _guard = self.credential_lock.lock().map_err(|_| {
            l10n::text(
                "凭据存储暂时不可用",
                "Credential storage lock is unavailable",
            )
        })?;
        let result = credentials::remove_relay_source(&self.context, params)?;
        self.account_changed("relay");
        Ok(result)
    }
    pub fn remove_credentials(&self, params: &Value) -> Result<Value, String> {
        let _guard = self.credential_lock.lock().map_err(|_| {
            l10n::text(
                "凭据存储暂时不可用",
                "Credential storage lock is unavailable",
            )
        })?;
        let result = credentials::remove(&self.context, params)?;
        self.credentials_changed(params);
        Ok(result)
    }
    fn credentials_changed(&self, params: &Value) {
        let wake = if let Ok(mut state) = self.shared.state.lock() {
            if let Some(provider) = params["provider"].as_str() {
                state.entries.remove(provider);
                state.force.insert(provider.into());
                state.enabled.insert(provider.into());
            }
            self.shared.wake.notify_all();
            state.waker.clone()
        } else {
            None
        };
        if let Some(wake) = wake {
            wake();
        }
    }
    pub fn set_waker(&self, waker: Waker) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.waker = Some(waker);
        }
    }
    pub fn stop(&self) {
        self.shared.cancelled.store(true, Ordering::Release);
        let mut copy = None;
        if let Ok(mut state) = self.shared.state.lock() {
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
            .shared
            .children
            .lock()
            .map(|children| {
                children
                    .iter()
                    .filter_map(Weak::upgrade)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for child in children {
            if let Ok(mut child) = child.lock() {
                let _ = child.kill();
            }
        }
    }
    pub fn refresh(&self) {
        if let Ok(mut state) = self.shared.state.lock() {
            let enabled = state.enabled.clone();
            state.force.extend(enabled);
            self.shared.wake.notify_all();
        }
    }
    pub fn update_sources(&self, snapshot: &Value) {
        let account_homes = homes::configured(&self.context.home, &snapshot["settings"]);
        let consent = snapshot["settings"][agent_hud_agents::copilot::account::CONSENT] == true;
        if self.context.copilot_consent.swap(consent, Ordering::AcqRel) != consent {
            if let Ok(mut state) = self.shared.state.lock() {
                state.entries.remove("copilot");
                state.force.insert("copilot".into());
            }
        }
        let relays: HashSet<String> = snapshot["settings"][agent_hud_agents::relays::CONSENT]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|host| host.as_str().map(str::to_owned))
            .collect();
        let relays_changed = self
            .context
            .relay_consent
            .lock()
            .map(|mut held| {
                let changed = *held != relays;
                *held = relays;
                changed
            })
            .unwrap_or(false);
        if relays_changed {
            if let Ok(mut state) = self.shared.state.lock() {
                state.force.insert("relay".into());
            }
        }
        let mut enabled: HashSet<String> = snapshot["providers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|value| value["detected"] == true && value["enabled"] == true)
            .filter_map(|value| value["id"].as_str().map(str::to_owned))
            .collect();
        for row in credentials::status(&self.context)["providers"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if row["configured"] == true {
                if let Some(provider) = row["provider"].as_str() {
                    if snapshot["providers"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|p| p["id"] == provider && p["enabled"] == true)
                    {
                        enabled.insert(provider.into());
                    }
                }
            }
        }
        // Pi's ChatGPT OAuth home is another source of the Codex account, as in
        // CodexUsageProvider. An explicit Codex visibility setting still wins.
        if enabled.contains("pi")
            && snapshot["providers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|value| value["id"] == "codex" && value["enabled"] == true)
        {
            enabled.insert("codex".into());
        }
        // Relays are found in Claude Code's and Codex's configuration, or
        // added by hand for any agent; with none, the reader reads two
        // configuration files and asks nothing.
        enabled.insert("relay".into());
        // A quota read with a web sign-in is offered for every client that
        // spends it; until the user signs in its reader asks nothing.
        for &facts in agents::ACCOUNTS {
            if facts.web.is_some() && facts.sources().iter().any(|s| enabled.contains(*s)) {
                enabled.insert(facts.id.into());
            }
        }
        for provider in account_homes.keys() {
            if snapshot["providers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|value| value["id"] == *provider && value["enabled"] == true)
            {
                enabled.insert(provider.clone());
            }
        }
        let cutoff = chrono::Utc::now().timestamp_millis() - 300_000;
        let mut running = HashSet::new();
        let mut live = HashSet::new();
        let mut finished = HashMap::<String, i64>::new();
        for session in snapshot["sessions"].as_array().into_iter().flatten() {
            let Some(provider) = session["source"].as_str() else {
                continue;
            };
            // A session's state is in flight only while its newest evidence is
            // under 30 minutes old (Open's `SessionPhase.Limits.vouched`).
            if matches!(session["state"].as_str(), Some("running" | "waiting")) {
                live.insert(provider.to_owned());
                if session["liveFacts"]["observedAt"]
                    .as_str()
                    .and_then(date_ms)
                    .is_some_and(|at| at > cutoff)
                {
                    running.insert(provider.to_owned());
                }
            }
            if let Some(at) = session["liveFacts"]["endedAt"].as_str().and_then(date_ms) {
                finished
                    .entry(provider.into())
                    .and_modify(|old| *old = (*old).max(at))
                    .or_insert(at);
            }
        }
        if let Ok(mut state) = self.shared.state.lock() {
            if state.account_homes != account_homes {
                let changed = state
                    .account_homes
                    .keys()
                    .chain(account_homes.keys())
                    .filter(|key| state.account_homes.get(*key) != account_homes.get(*key))
                    .cloned()
                    .collect::<HashSet<_>>();
                for provider in changed {
                    state.entries.remove(provider.as_str());
                    state.force.insert(provider);
                }
                state.account_homes = account_homes;
                self.shared.wake.notify_all();
            }
            if state.enabled != enabled
                || state.running != running
                || state.live != live
                || state.finished != finished
            {
                state.enabled = enabled;
                state.running = running;
                state.live = live;
                state.finished = finished;
                self.shared.wake.notify_all();
            }
        }
    }
    pub fn projection(&self) -> AccountProjection {
        let Ok(state) = self.shared.state.lock() else {
            return AccountProjection::default();
        };
        let mut result = AccountProjection::default();
        let now = chrono::Utc::now().timestamp_millis();
        // Every provider the app knows, whatever this service reads.
        for &facts in agents::ACCOUNTS {
            let provider = facts.id;
            if !state.enabled.contains(provider) {
                continue;
            }
            let Some(entry) = state.entries.get(provider) else {
                result.providers.insert(
                    provider.into(),
                    json!({"status":"loading","checkedAt":null,"notice":l10n::text("正在读取额度", "Reading account usage")}),
                );
                continue;
            };
            result.quotas.extend(
                entry
                    .reading
                    .quotas
                    .iter()
                    .filter(|row| !expired_report(facts, row, "observedAt", now))
                    .cloned()
                    .map(|mut row| {
                        if row["observedAt"].is_null() {
                            row["observedAt"] = json!(entry.checked_at);
                        }
                        if row["accountId"].is_null() {
                            if let Some(account) = entry.reading.account.as_ref() {
                                row["accountId"] = account["id"].clone();
                            }
                        }
                        if entry.failed
                            || stale_report(facts, &row, "observedAt", now)
                            || row["resetAt"]
                                .as_str()
                                .and_then(date_ms)
                                .is_some_and(|at| at <= chrono::Utc::now().timestamp_millis())
                        {
                            row["status"] = json!("stale");
                        }
                        row
                    }),
            );
            result
                .billing
                .extend(entry.reading.billing.iter().cloned().map(|mut row| {
                    if row["accountId"].is_null() {
                        if let Some(account) = entry.reading.account.as_ref() {
                            row["accountId"] = account["id"].clone();
                        }
                    }
                    if entry.failed {
                        row["notice"] = json!(entry.notice);
                    }
                    row
                }));
            for account in entry
                .reading
                .account
                .iter()
                .chain(&entry.reading.additional_accounts)
            {
                if expired_report(facts, account, "checkedAt", now) {
                    continue;
                }
                // A failed read keeps the last reading; why stands under the
                // account it failed for, as a signed-out Claude Code asks to
                // be signed in again.
                let failure = entry.notice.as_ref().filter(|_| entry.failed).filter(|_| {
                    entry
                        .reading
                        .account
                        .as_ref()
                        .is_some_and(|read| std::ptr::eq(read, account))
                });
                let mut account = account.clone();
                if account["checkedAt"].is_null() {
                    account["checkedAt"] = json!(entry.checked_at);
                }
                account["stale"] = json!(
                    entry.failed
                        || account["stale"] == true
                        || stale_report(facts, &account, "checkedAt", now)
                );
                if let Some(notice) = failure {
                    account["notice"] = json!(notice);
                }
                result.accounts.push(account);
            }
            result
                .session_records
                .extend(entry.reading.session_records.iter().cloned());
            result
                .services
                .extend(entry.reading.services.iter().cloned());
            let ready = !entry.reading.quotas.is_empty() || !entry.reading.billing.is_empty();
            // A failure shows with its kind, so what the agent's own state
            // is (signed out) stays apart from what only missed an answer.
            let failure = entry.failure.filter(|_| entry.failed);
            let failing_since = entry
                .failing_since
                .filter(|_| entry.failed)
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|at| at.to_rfc3339());
            // What the vendor's status page says, while a component this
            // provider depends on is not operational.
            let service = facts.status.and_then(|page| {
                let read = state.pages.get(page.url)?;
                status::service(page, read.summary.as_ref()?, read.checked_at.as_deref()?)
            });
            // Where the user may sign in for this quota, and where they have;
            // the other clients that spend it show the same.
            let web_sign_in = web::status(&self.context, facts);
            if let Some(status) = &web_sign_in {
                for &source in facts.sources().iter().filter(|s| **s != facts.id) {
                    result.providers.entry(source.into()).or_insert_with(|| {
                        json!({"status":"unavailable","checkedAt":null,"notice":null,"webSignIn":status})
                    });
                }
            }
            result.providers.insert(entry.reading.provider.into(),json!({"status":if entry.failed {"error"}else if ready {"ready"}else{"unavailable"},"checkedAt":entry.checked_at,"notice":entry.notice,"failure":failure,"failingSince":failing_since,"service":service,"webSignIn":web_sign_in}));
        }
        for metadata in credentials::status(&self.context)["providers"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(provider) = metadata["provider"].as_str() {
                let entry=result.providers.entry(provider.into()).or_insert_with(||json!({"status":"unavailable","checkedAt":null,"notice":l10n::text("可以在 Agent HUD 中配置 API 凭据", "API credentials can be configured in Agent HUD")}));
                entry["configurableFields"] = metadata["configurableFields"].clone();
                entry["configured"] = metadata["configured"].clone();
            }
        }
        result
    }
}
impl Drop for AccountService {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(context: AccountContext, shared: Arc<Shared>, gate: CollectionGate, readers: Providers) {
    let mut burst: Option<Instant> = None;
    let mut resume_at: Option<Instant> = None;
    let mut saved: Option<Instant> = None;
    loop {
        let selected = {
            let Ok(mut state) = shared.state.lock() else {
                return;
            };
            loop {
                if state.stopped {
                    return;
                }
                // Force remains pending until each enabled account satisfies the
                // same 60s request spacing as the original collector.
                let mut pause = Duration::from_secs(300);
                let mut due = None;
                for &facts in readers {
                    let provider = facts.id;
                    if !state.enabled.contains(provider) {
                        continue;
                    }
                    let wait = state
                        .entries
                        .get(provider)
                        .map_or(Duration::ZERO, |entry| due_after(provider, entry, &state));
                    if wait.is_zero() {
                        due = Some((provider, facts));
                        break;
                    }
                    pause = pause.min(wait);
                }
                let now = Instant::now();
                // A status page waits behind every account reading that is due.
                let mut page = None;
                if due.is_none() {
                    for &facts in readers {
                        let Some(status) =
                            facts.status.filter(|_| state.enabled.contains(facts.id))
                        else {
                            continue;
                        };
                        let wait = state.pages.get(status.url).map_or(Duration::ZERO, |read| {
                            status::INTERVAL.saturating_sub(read.attempted.elapsed())
                        });
                        if wait.is_zero() {
                            page = Some(status);
                            break;
                        }
                        pause = pause.min(wait);
                    }
                }
                if let Some(at) = resume_at.filter(|at| *at > now) {
                    pause = at - now;
                } else if due.is_some() || page.is_some() {
                    if burst.is_none_or(|started| started.elapsed() < STEP_BUDGET) {
                        resume_at = None;
                        let Some(selected) = due else {
                            break Job::Page(page.expect("a page is due"));
                        };
                        // Consume the request this job serves. A callback arriving
                        // during I/O remains pending for the next permitted pass.
                        state.force.remove(selected.0);
                        break Job::Account(selected.0, selected.1, state.account_homes.clone());
                    }
                    burst = None;
                    resume_at = Some(now + STEP_BUDGET);
                    pause = STEP_BUDGET;
                } else {
                    burst = None;
                }
                let Ok((next, _)) = shared.wake.wait_timeout(state, pause) else {
                    return;
                };
                state = next;
            }
        };
        burst.get_or_insert_with(Instant::now);
        let selected = match selected {
            Job::Account(provider, facts, homes) => (provider, facts, homes),
            Job::Page(page) => {
                let attempted = Instant::now();
                let summary = {
                    let _turn = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    if shared.cancelled.load(Ordering::Acquire) {
                        return;
                    }
                    status::read(&context.http, page.url)
                };
                let waker = {
                    let Ok(mut state) = shared.state.lock() else {
                        return;
                    };
                    // A page that could not be read keeps what it said last.
                    let old = state.pages.remove(page.url);
                    let changed = summary.is_some()
                        && old.as_ref().and_then(|old| old.summary.as_ref()) != summary.as_ref();
                    let (checked_at, summary) = match summary {
                        Some(summary) => (Some(chrono::Utc::now().to_rfc3339()), Some(summary)),
                        None => old
                            .map(|old| (old.checked_at, old.summary))
                            .unwrap_or_default(),
                    };
                    state.pages.insert(
                        page.url,
                        PageReading {
                            attempted,
                            checked_at,
                            summary,
                        },
                    );
                    changed.then(|| state.waker.clone()).flatten()
                };
                if let Some(waker) = waker {
                    waker();
                }
                continue;
            }
        };
        let attempted = Instant::now();
        let attempted_ms = chrono::Utc::now().timestamp_millis();
        // The gate, not the shared state or the Runtime, is held across the I/O.
        let result = {
            let _turn = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if shared.cancelled.load(Ordering::Acquire) {
                return;
            }
            let mut reading_context = context.clone();
            reading_context.account_homes = selected.2.clone();
            (selected.1.read)(&reading_context)
        };
        let (waker, copy) = {
            let Ok(mut state) = shared.state.lock() else {
                return;
            };
            if state.stopped {
                return;
            }
            // A removed home must not reappear when its in-flight read completes.
            if state.account_homes.get(selected.0) != selected.2.get(selected.0) {
                continue;
            }
            let spread = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as u64
                % 61;
            settle(
                &mut state.entries,
                selected.1,
                result,
                (attempted, attempted_ms),
                spread,
            );
            let copy = saved
                .is_none_or(|at| at.elapsed() >= restart_copy::SAVE_INTERVAL)
                .then(|| restart_copy::copy(&state.entries, &state.account_homes));
            (state.waker.clone(), copy)
        };
        if let Some(copy) = copy {
            saved = Some(Instant::now());
            restart_copy::save(&context.data_dir, &copy);
        }
        if let Some(waker) = waker {
            waker();
        }
    }
}

/// A provider's reading, as its entry keeps it. A good one replaces the
/// entry. A failed one goes through `failed_reading`, with the windows the
/// agent itself last recorded where it brings them (`Reading::failure`).
fn settle(
    entries: &mut BTreeMap<&'static str, Entry>,
    facts: &'static Provider,
    result: Result<AccountReading, Unread>,
    (attempted, attempted_ms): (Instant, i64),
    spread: u64,
) {
    let (record, result) = match result {
        Ok(mut reading) => match reading.failure.take() {
            Some(unread) => (Some(reading), Err(unread)),
            None => (None, Ok(reading)),
        },
        Err(unread) => (None, Err(unread)),
    };
    match result {
        Ok(mut reading) => {
            if facts.keeps_unreported_windows {
                retain_windows(entries.get(facts.id), &mut reading, attempted_ms);
            }
            let notice = reading.notice.clone();
            entries.insert(
                facts.id,
                Entry {
                    reading,
                    checked_at: Some(chrono::Utc::now().to_rfc3339()),
                    attempted,
                    attempted_ms,
                    notice,
                    failed: false,
                    spread,
                    restored: false,
                    failures: 0,
                    failure: None,
                    failing_since: None,
                    retry_at: None,
                },
            );
        }
        Err(unread) => {
            let entry = entries.entry(facts.id).or_insert_with(|| Entry {
                reading: AccountReading::empty(facts.id),
                checked_at: None,
                attempted,
                attempted_ms,
                notice: None,
                failed: true,
                spread,
                restored: false,
                failures: 0,
                failure: None,
                failing_since: None,
                retry_at: None,
            });
            if let Some(record) = record {
                take_record(entry, record);
            }
            failed_reading(entry, unread, attempted, attempted_ms);
            entry.spread = spread;
        }
    }
}

/// What the agent itself last recorded, brought by a failed reading: each
/// window and account newer than the one the entry shows, or that it does
/// not show, takes its place, keeping the time it was read. A row without
/// its own time was read when the entry was.
fn take_record(entry: &mut Entry, record: AccountReading) {
    let checked = entry.checked_at.clone();
    let read_at =
        |row: &Value, key: &str| row[key].as_str().or(checked.as_deref()).and_then(date_ms);
    for row in record.quotas {
        match entry
            .reading
            .quotas
            .iter_mut()
            .find(|old| old["id"] == row["id"])
        {
            Some(old) if read_at(old, "observedAt") >= read_at(&row, "observedAt") => {}
            Some(old) => *old = row,
            None => entry.reading.quotas.push(row),
        }
    }
    for account in record.account.into_iter().chain(record.additional_accounts) {
        let reading = &mut entry.reading;
        let shown = reading
            .account
            .iter_mut()
            .chain(&mut reading.additional_accounts)
            .find(|old| old["id"] == account["id"]);
        if let Some(old) = shown {
            if read_at(old, "checkedAt") < read_at(&account, "checkedAt") {
                *old = account;
            }
        } else if reading.account.is_none() {
            reading.account = Some(account);
        } else {
            reading.additional_accounts.push(account);
        }
    }
}

/// How long a window the provider stopped reporting keeps its last reading
/// (agent-hud-native-open QuotaHistoryStore.retention).
const WINDOW_RETENTION_MS: i64 = 30 * 86_400_000;

/// How old a row a client reported by itself is (`Provider::reported`).
fn report_age(facts: &Provider, row: &Value, time_key: &str, now: i64) -> Option<i64> {
    let reported = facts.reported?;
    (row["source"] == reported.source)
        .then(|| {
            row[time_key]
                .as_str()
                .and_then(date_ms)
                .map(|at| now.saturating_sub(at))
        })
        .flatten()
}

fn stale_report(facts: &Provider, row: &Value, time_key: &str, now: i64) -> bool {
    let Some(reported) = facts.reported else {
        return false;
    };
    report_age(facts, row, time_key, now)
        .is_some_and(|age| age > reported.stale_after_seconds * 1000)
}

fn expired_report(facts: &Provider, row: &Value, time_key: &str, now: i64) -> bool {
    report_age(facts, row, time_key, now).is_some_and(|age| age >= WINDOW_RETENTION_MS)
}

/// Records a failed reading. A reading that only missed an answer, while the
/// last good one is under `GRACE_MS` old, changes nothing on screen; any
/// other failure is shown at once with its reason, and the last reading
/// stays, marked stale. Either way the next try backs off, and waits as long
/// as the service asked.
fn failed_reading(entry: &mut Entry, unread: Unread, attempted: Instant, attempted_ms: i64) {
    let recent = entry
        .checked_at
        .as_deref()
        .and_then(date_ms)
        .is_some_and(|at| attempted_ms.saturating_sub(at) < GRACE_MS);
    let unnoticed = !entry.failed && recent && matches!(unread, Unread::Transient { .. });
    entry.attempted = attempted;
    entry.attempted_ms = attempted_ms;
    entry.restored = false;
    entry.failures = entry.failures.saturating_add(1);
    entry.failing_since.get_or_insert(attempted_ms);
    entry.retry_at = match &unread {
        Unread::Transient {
            retry_after: Some(wait),
            ..
        } => Some(attempted + (*wait).min(MOST_RETRY_AFTER)),
        _ => None,
    };
    if unnoticed {
        return;
    }
    entry.failure = Some(unread.kind());
    entry.notice = Some(unread.reason().to_owned());
    entry.failed = true;
}

/// An absent reading is not a reset (agent-hud-native-open
/// RetainedUsageProvider.retainingReadings): a Claude window this reading
/// left out keeps its last reading, and the time it was read, while the same
/// account is signed in (any of the reading's accounts, one per folder) and
/// for at most 30 days. Codex's app-server reports its complete window
/// inventory, so a window it leaves out retires at once; the other
/// providers' rules belong to their own readers.
fn retain_windows(previous: Option<&Entry>, reading: &mut AccountReading, now: i64) {
    let Some(previous) = previous else {
        return;
    };
    let accounts = reading
        .account
        .iter()
        .chain(&reading.additional_accounts)
        .map(|account| account["id"].clone())
        .collect::<Vec<_>>();
    let reported = reading
        .quotas
        .iter()
        .filter_map(|row| row["id"].as_str().map(str::to_owned))
        .collect::<HashSet<_>>();
    let kept = previous
        .reading
        .quotas
        .iter()
        .filter(|row| row["id"].as_str().is_some_and(|id| !reported.contains(id)))
        .filter(|row| accounts.contains(&row["accountId"]))
        .filter_map(|row| {
            let mut row = row.clone();
            if row["observedAt"].is_null() {
                row["observedAt"] = json!(previous.checked_at);
            }
            let seen = row["observedAt"].as_str().and_then(date_ms)?;
            (now - seen < WINDOW_RETENTION_MS).then_some(row)
        })
        .collect::<Vec<_>>();
    reading.quotas.extend(kept);
}

fn date_ms(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.timestamp_millis())
}
/// When a provider's next reading is due, as Open's `accountCheck`: a
/// running turn is read every 150 s, a session in flight but quiet every
/// 300 s, a provider whose windows this Mac's work does not move every 300 s,
/// work that finished since the last reading at once, and an idle provider
/// whose windows all reset later only at the first reset. Every interval
/// waits the reading's spread past it, and no reading follows another within
/// 60 s.
fn due_after(provider: &str, entry: &Entry, state: &Control) -> Duration {
    if entry.restored {
        return Duration::ZERO;
    }
    let elapsed = entry.attempted.elapsed();
    // What the service asked holds even for a refresh the user asked for.
    let asked = entry.retry_at.map_or(Duration::ZERO, |at| {
        at.saturating_duration_since(Instant::now())
    });
    let spacing = Duration::from_secs(60).saturating_sub(elapsed);
    let now = chrono::Utc::now().timestamp_millis();
    let resets = entry
        .reading
        .quotas
        .iter()
        .filter_map(|row| row["resetAt"].as_str().and_then(date_ms))
        .collect::<Vec<_>>();
    let sources = work_sources(provider);
    let running = sources.iter().any(|source| state.running.contains(*source));
    let live = sources.iter().any(|source| state.live.contains(*source));
    let finished = sources
        .iter()
        .filter_map(|source| state.finished.get(*source))
        .any(|at| *at > entry.attempted_ms);
    let local = sees_local_work(provider) && !state.account_homes.contains_key(provider);
    // After failed readings the next try backs off, whatever the interval;
    // a refresh the user asked for, or this computer's work finishing, tries
    // at once (signing in again and working is how many failures end).
    if entry.failures > 0 {
        let now_due = state.force.contains(provider) || (local && finished);
        let retry = if now_due {
            Duration::ZERO
        } else {
            backoff(entry.failures).saturating_sub(elapsed)
        };
        return retry.max(spacing).max(asked);
    }
    let quiet_known_reset = local
        && !entry.failed
        && !resets.is_empty()
        && resets.iter().all(|reset| *reset > entry.attempted_ms);
    let interval = if state.force.contains(provider) {
        60
    } else if running {
        150 + entry.spread
    } else if live || !local || !quiet_known_reset {
        300 + entry.spread
    } else {
        86400
    };
    let mut pause = Duration::from_secs(interval).saturating_sub(elapsed);
    if local && !running && !live && finished {
        pause = Duration::ZERO;
    }
    for reset in resets
        .into_iter()
        .filter(|reset| *reset > entry.attempted_ms)
    {
        pause = pause.min(Duration::from_millis(
            (reset + entry.spread as i64 * 1000 - now).max(0) as u64,
        ));
    }
    // Account requests are never <60s apart.
    pause.max(spacing).max(asked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    static ACTIVE: AtomicUsize = AtomicUsize::new(0);
    static MOST: AtomicUsize = AtomicUsize::new(0);
    static DONE: AtomicUsize = AtomicUsize::new(0);
    static GATE: Mutex<Option<CollectionGate>> = Mutex::new(None);

    fn reading(provider: &'static str) -> Result<AccountReading, Unread> {
        let active = ACTIVE.fetch_add(1, Ordering::SeqCst) + 1;
        MOST.fetch_max(active, Ordering::SeqCst);
        let gate = GATE.lock().unwrap().clone().unwrap();
        assert!(
            gate.try_lock().is_err(),
            "a reading holds the collection gate"
        );
        std::thread::sleep(Duration::from_millis(30));
        ACTIVE.fetch_sub(1, Ordering::SeqCst);
        DONE.fetch_add(1, Ordering::SeqCst);
        Ok(AccountReading::empty(provider))
    }
    fn first(_: &dyn agents::Host) -> Result<AccountReading, Unread> {
        reading("first")
    }
    fn second(_: &dyn agents::Host) -> Result<AccountReading, Unread> {
        reading("second")
    }
    fn third(_: &dyn agents::Host) -> Result<AccountReading, Unread> {
        reading("third")
    }
    static FIRST: Provider = Provider::new("first", first);
    static SECOND: Provider = Provider::new("second", second);
    static THIRD: Provider = Provider::new("third", third);
    static FAKE: [&Provider; 3] = [&FIRST, &SECOND, &THIRD];

    fn control() -> Control {
        Control {
            stopped: false,
            force: HashSet::new(),
            enabled: HashSet::new(),
            running: HashSet::new(),
            live: HashSet::new(),
            finished: HashMap::new(),
            entries: BTreeMap::new(),
            pages: HashMap::new(),
            account_homes: HashMap::new(),
            waker: None,
        }
    }

    #[test]
    fn callback_samples_age_and_expire_even_when_the_provider_cannot_be_read() {
        let directory = tempfile::tempdir().unwrap();
        let service = AccountService::with_readers(
            directory.path().into(),
            directory.path().into(),
            HashSet::new(),
            CollectionGate::default(),
            &[],
        )
        .unwrap();
        let now = chrono::Utc::now();
        let stale = (now - chrono::Duration::seconds(301)).to_rfc3339();
        let expired = (now - chrono::Duration::days(31)).to_rfc3339();
        let mut reading = AccountReading::empty("antigravity");
        reading.account =
            Some(json!({"id":"recent","source":"antigravity_cli","checkedAt":stale,"stale":false}));
        reading.additional_accounts = vec![
            json!({"id":"expired","source":"antigravity_cli","checkedAt":expired}),
            json!({"id":"native","checkedAt":expired}),
        ];
        reading.quotas = vec![
            json!({"id":"recent","source":"antigravity_cli","observedAt":stale,"usedPercent":20,"status":"normal"}),
            json!({"id":"expired","source":"antigravity_cli","observedAt":expired}),
            json!({"id":"native","observedAt":expired,"status":"normal"}),
        ];
        {
            let mut state = service.shared.state.lock().unwrap();
            state.enabled.insert("antigravity".into());
            state.entries.insert(
                "antigravity",
                entry(reading, Some(&now.to_rfc3339()), false),
            );
        }
        for failed in [false, true] {
            service
                .shared
                .state
                .lock()
                .unwrap()
                .entries
                .get_mut("antigravity")
                .unwrap()
                .failed = failed;
            let projection = service.projection();
            assert_eq!(projection.accounts.len(), 2);
            assert_eq!(projection.quotas.len(), 2);
            assert_eq!(projection.accounts[0]["stale"], true);
            assert_eq!(projection.quotas[0]["status"], "stale");
            assert_eq!(projection.quotas[0]["observedAt"], stale);
            assert_eq!(projection.quotas[1]["id"], "native");
        }
    }

    #[test]
    fn a_failed_read_keeps_its_windows_and_says_why_under_its_account() {
        let directory = tempfile::tempdir().unwrap();
        let service = AccountService::with_readers(
            directory.path().join("home"),
            directory.path().join("data"),
            HashSet::new(),
            CollectionGate::default(),
            &[],
        )
        .unwrap();
        let mut reading = AccountReading::empty("claude");
        reading.account = Some(json!({"id":"me","provider":"claude"}));
        reading.additional_accounts = vec![json!({"id":"other","provider":"claude"})];
        reading.quotas = vec![json!({"id":"me:claude-session","usedPercent":18,"status":"normal"})];
        let checked = chrono::Utc::now().to_rfc3339();
        {
            let mut state = service.shared.state.lock().unwrap();
            state.enabled.insert("claude".into());
            state
                .entries
                .insert("claude", entry(reading, Some(&checked), false));
        }
        let projection = service.projection();
        assert!(projection.accounts[0]["notice"].is_null());

        {
            let mut state = service.shared.state.lock().unwrap();
            let entry = state.entries.get_mut("claude").unwrap();
            entry.failed = true;
            entry.notice = Some("Claude Code is signed out".into());
        }
        let projection = service.projection();
        assert_eq!(projection.quotas[0]["usedPercent"], 18);
        assert_eq!(projection.quotas[0]["status"], "stale");
        assert_eq!(projection.accounts[0]["stale"], true);
        assert_eq!(projection.accounts[0]["checkedAt"], checked);
        assert_eq!(
            projection.accounts[0]["notice"],
            "Claude Code is signed out"
        );
        // The read failed for the signed-in account, not for another one.
        assert!(projection.accounts[1]["notice"].is_null());
    }

    #[test]
    fn connected_cli_reports_use_account_cadence_and_callback_requests_respect_spacing() {
        let mut state = control();
        let mut reading = AccountReading::empty("antigravity");
        reading
            .quotas
            .push(json!({"resetAt": (chrono::Utc::now()+chrono::Duration::days(7)).to_rfc3339()}));
        let fresh = entry(reading, Some("2026-10-23T08:00:00Z"), false);
        assert!(due_after("antigravity", &fresh, &state) > Duration::from_secs(300));
        state
            .account_homes
            .insert("antigravity".into(), vec!["/tmp/config.json".into()]);
        assert!(due_after("antigravity", &fresh, &state) <= Duration::from_secs(300));
        state.force.insert("antigravity".into());
        let pause = due_after("antigravity", &fresh, &state);
        assert!(pause > Duration::from_secs(59) && pause <= Duration::from_secs(60));
    }

    #[test]
    fn a_callback_during_an_account_read_remains_pending_after_that_read_commits() {
        fn blocked(host: &dyn agents::Host) -> Result<AccountReading, Unread> {
            assert_eq!(
                host.account_homes("antigravity"),
                [host.home().join("cli/settings.json")],
            );
            std::fs::write(host.home().join("read-started"), "")
                .map_err(|error| error.to_string())?;
            let deadline = Instant::now() + Duration::from_secs(5);
            while !host.home().join("release-read").exists() {
                if host.cancelled() || Instant::now() >= deadline {
                    return Err("fixture read was cancelled or timed out".into());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(AccountReading::empty("antigravity"))
        }
        static BLOCKED: Provider = Provider::new("antigravity", blocked);
        static READERS: [&Provider; 1] = [&BLOCKED];
        let directory = tempfile::tempdir().unwrap();
        let service = AccountService::with_readers(
            directory.path().into(),
            directory.path().join("data"),
            HashSet::new(),
            CollectionGate::default(),
            &READERS,
        )
        .unwrap();
        service.update_sources(&json!({
            "settings":{"antigravityCliSettings":["~/cli/settings.json"]},
            "providers":[{"id":"antigravity","enabled":true,"detected":false}],
            "sessions":[],
        }));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !directory.path().join("read-started").exists() {
            assert!(
                Instant::now() < deadline,
                "the fixture reader did not start"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        service.account_changed("antigravity");
        std::fs::write(directory.path().join("release-read"), "").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let (pending, pause, elapsed, failed) = loop {
            let state = service.shared.state.lock().unwrap();
            if let Some(entry) = state.entries.get("antigravity") {
                break (
                    state.force.contains("antigravity"),
                    due_after("antigravity", entry, &state),
                    entry.attempted.elapsed(),
                    entry.failed,
                );
            }
            drop(state);
            assert!(
                Instant::now() < deadline,
                "the fixture reading did not commit"
            );
            std::thread::sleep(Duration::from_millis(2));
        };
        service.stop();
        assert!(
            !failed,
            "the controlled account reading completed successfully"
        );
        assert!(
            pending,
            "a callback received during I/O still needs a new reading"
        );
        assert!(pause > Duration::ZERO && pause <= Duration::from_secs(60));
        assert!(
            pause >= Duration::from_secs(60).saturating_sub(elapsed),
            "the pending callback respects the account's 60-second request spacing",
        );
    }

    #[test]
    fn callback_inbox_reaches_only_the_selected_account_context() {
        let directory = tempfile::tempdir().unwrap();
        let configuration = directory.path().join("settings.json");
        std::fs::write(&configuration, "{}").unwrap();
        let configuration = configuration.canonicalize().unwrap();
        let data = directory.path().join("data");
        let args = vec![
            "fixture".into(),
            "--antigravity-statusline-hook".into(),
            data.to_str().unwrap().into(),
            configuration.to_str().unwrap().into(),
        ];
        let mut input = std::io::Cursor::new(br#"{"email":"account@example.com","quota":{"gemini-weekly":{"remaining_fraction":0.4}},"transcript_path":"private"}"#);
        let mut output = Vec::new();
        assert_eq!(
            crate::hooks::handle_with_io(&args, &data, &mut input, &mut output),
            Some(0)
        );
        assert!(output.is_empty());
        let service = AccountService::with_readers(
            directory.path().into(),
            data,
            HashSet::new(),
            CollectionGate::default(),
            &[],
        )
        .unwrap();
        let mut context = service.context.clone();
        assert!(agents::Host::account_reports(&context, "antigravity").is_empty());
        context
            .account_homes
            .insert("antigravity".into(), vec![configuration]);
        let reports = agents::Host::account_reports(&context, "antigravity");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0]["email"], "account@example.com");
        assert!(!reports[0].to_string().contains("private"));
        context.account_homes.clear();
        assert!(agents::Host::account_reports(&context, "antigravity").is_empty());
    }

    fn entry(reading: AccountReading, checked_at: Option<&str>, restored: bool) -> Entry {
        Entry {
            reading,
            checked_at: checked_at.map(str::to_owned),
            attempted: Instant::now(),
            attempted_ms: chrono::Utc::now().timestamp_millis(),
            notice: None,
            failed: checked_at.is_none(),
            spread: 0,
            restored,
            failures: 0,
            failure: None,
            failing_since: None,
            retry_at: None,
        }
    }

    #[test]
    fn configured_homes_enable_undetected_accounts_and_changes_retire_old_readings() {
        for (provider, setting) in [("codex", "codexHomes"), ("claude", "claudeHomes")] {
            let directory = tempfile::tempdir().unwrap();
            let service = AccountService::with_readers(
                directory.path().into(),
                directory.path().into(),
                HashSet::new(),
                CollectionGate::default(),
                &[],
            )
            .unwrap();
            let mut snapshot = json!({
                "settings": {},
                "providers": [{"id":provider, "enabled":true, "detected":false}],
                "sessions": [],
            });
            snapshot["settings"][setting] = json!(["~/work"]);
            service.update_sources(&snapshot);
            {
                let mut state = service.shared.state.lock().unwrap();
                assert!(state.enabled.contains(provider));
                assert_eq!(
                    state.account_homes[provider],
                    vec![directory.path().join("work")]
                );
                state.entries.insert(
                    provider,
                    entry(AccountReading::empty(provider), None, false),
                );
            }
            // Routine snapshot updates preserve the cache; edits retire its old homes.
            service.update_sources(&snapshot);
            assert!(service
                .shared
                .state
                .lock()
                .unwrap()
                .entries
                .contains_key(provider));
            snapshot["settings"][setting] = json!(["~/other"]);
            service.update_sources(&snapshot);
            {
                let mut state = service.shared.state.lock().unwrap();
                assert!(!state.entries.contains_key(provider), "{provider}");
                assert!(state.force.contains(provider), "{provider}");
                state.entries.insert(
                    provider,
                    entry(AccountReading::empty(provider), None, false),
                );
            }
            snapshot["settings"][setting] = json!([]);
            service.update_sources(&snapshot);
            {
                let state = service.shared.state.lock().unwrap();
                assert!(!state.entries.contains_key(provider), "{provider}");
                assert!(!state.enabled.contains(provider), "{provider}");
                assert!(state.account_homes.is_empty());
            }
            snapshot["settings"][setting] = json!(["~/work"]);
            snapshot["providers"][0]["enabled"] = json!(false);
            service.update_sources(&snapshot);
            assert!(!service
                .shared
                .state
                .lock()
                .unwrap()
                .enabled
                .contains(provider));
            assert!(service
                .validate_account_homes(&json!(["relative"]))
                .is_err());
        }
    }

    #[test]
    fn the_restart_copy_keeps_each_last_good_reading_with_its_time() {
        let directory = tempfile::tempdir().unwrap();
        let mut good = AccountReading::empty("second");
        good.quotas =
            vec![json!({"id":"second:5h","usedPercent":42,"resetAt":"2026-10-20T15:00:00Z"})];
        good.billing = vec![json!({"id":"second:balance","amount":"12.50"})];
        good.account = Some(json!({"id":"second:account","email":"a@example.com"}));
        good.notice = Some("Plan from the last sign-in".into());
        let mut entries = BTreeMap::new();
        entries.insert(
            "second",
            entry(good, Some("2026-10-20T10:00:00+00:00"), false),
        );
        // A provider that has only failed has nothing to keep.
        entries.insert("third", entry(AccountReading::empty("third"), None, false));
        restart_copy::save(
            directory.path(),
            &restart_copy::copy(&entries, &HashMap::new()),
        );

        let restored = restart_copy::load(directory.path(), &FAKE).readings;
        assert_eq!(restored.len(), 1);
        let (reading, checked_at) = &restored[0];
        assert_eq!(reading.provider, "second");
        assert_eq!(
            checked_at, "2026-10-20T10:00:00+00:00",
            "a kept reading keeps the time it was read"
        );
        assert_eq!(reading.quotas[0]["usedPercent"], 42);
        assert_eq!(reading.billing[0]["amount"], "12.50");
        assert_eq!(reading.account.as_ref().unwrap()["id"], "second:account");
        assert_eq!(
            reading.notice.as_deref(),
            Some("Plan from the last sign-in")
        );
        assert!(
            restart_copy::load(directory.path(), &FAKE[..1])
                .readings
                .is_empty(),
            "providers this build does not read are left out"
        );

        std::fs::write(
            directory.path().join("last-account-readings.json"),
            "{\"version\":99}",
        )
        .unwrap();
        assert!(
            restart_copy::load(directory.path(), &FAKE)
                .readings
                .is_empty(),
            "a copy in another format restores nothing"
        );
    }

    #[test]
    fn restored_accounts_keep_their_home_scope_and_removed_homes_do_not_return() {
        fn unread(_: &dyn agents::Host) -> Result<AccountReading, Unread> {
            panic!("the collection gate keeps this fixture read-only")
        }
        static CODEX: Provider = Provider::new("codex", unread);
        static CLAUDE: Provider = Provider::new("claude", unread);
        static ANTIGRAVITY: Provider = Provider::new("antigravity", unread);
        static READERS: [&Provider; 3] = [&CODEX, &CLAUDE, &ANTIGRAVITY];
        for (provider, setting) in [
            ("codex", "codexHomes"),
            ("claude", "claudeHomes"),
            ("antigravity", "antigravityCliSettings"),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let homes = HashMap::from([(provider.into(), vec![directory.path().join("work")])]);
            let entries = BTreeMap::from([(
                provider,
                entry(
                    AccountReading::empty(provider),
                    Some("2026-10-22T12:00:00Z"),
                    false,
                ),
            )]);
            for (configured, retained) in [(json!(["~/work"]), true), (json!([]), false)] {
                restart_copy::save(directory.path(), &restart_copy::copy(&entries, &homes));
                let gate = CollectionGate::default();
                let pass = gate.lock().unwrap();
                let service = AccountService::with_readers(
                    directory.path().into(),
                    directory.path().into(),
                    HashSet::new(),
                    Arc::clone(&gate),
                    &READERS,
                )
                .unwrap();
                let mut snapshot = json!({
                    "settings":{},
                    "providers":[{"id":provider,"enabled":true,"detected":true}],
                });
                snapshot["settings"][setting] = configured;
                service.update_sources(&snapshot);
                assert_eq!(
                    service
                        .shared
                        .state
                        .lock()
                        .unwrap()
                        .entries
                        .contains_key(provider),
                    retained,
                    "{provider} must use the restored account's selected source scope",
                );
                service.stop();
                drop(pass);
            }
        }
    }

    #[test]
    fn a_restored_reading_is_due_at_once_and_a_fresh_one_waits() {
        let state = control();
        let restored = entry(
            AccountReading::empty("first"),
            Some("2026-10-20T10:00:00+00:00"),
            true,
        );
        assert_eq!(due_after("first", &restored, &state), Duration::ZERO);
        let fresh = entry(
            AccountReading::empty("first"),
            Some("2026-10-20T10:00:00+00:00"),
            false,
        );
        assert!(
            due_after("first", &fresh, &state) >= Duration::from_secs(60),
            "never sooner than 60 s after a reading"
        );
    }

    #[test]
    fn readings_run_one_at_a_time_and_never_beside_a_local_pass() {
        let gate = CollectionGate::default();
        *GATE.lock().unwrap() = Some(Arc::clone(&gate));
        let enabled = ["first", "second", "third"].map(String::from).into();
        let directory = tempfile::tempdir().unwrap();
        // A local pass is in progress: no reading may start until it ends.
        let pass = gate.lock().unwrap();
        let service = AccountService::with_readers(
            directory.path().into(),
            directory.path().into(),
            enabled,
            Arc::clone(&gate),
            &FAKE,
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            DONE.load(Ordering::SeqCst),
            0,
            "readings wait for the local pass"
        );
        drop(pass);
        let deadline = Instant::now() + Duration::from_secs(5);
        while DONE.load(Ordering::SeqCst) < 3 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        service.stop();
        assert_eq!(DONE.load(Ordering::SeqCst), 3);
        assert_eq!(MOST.load(Ordering::SeqCst), 1, "never two readings at once");
    }

    fn reset_later(provider: &'static str) -> Entry {
        let mut reading = AccountReading::empty(provider);
        let reset = (chrono::Utc::now() + chrono::Duration::hours(3)).to_rfc3339();
        reading.quotas = vec![json!({"id":"w","usedPercent":10,"resetAt":reset})];
        let mut entry = entry(reading, Some("2026-10-21T08:00:00+00:00"), false);
        // The last reading ran ten minutes ago.
        entry.attempted -= Duration::from_secs(600);
        entry.attempted_ms -= 600_000;
        entry
    }

    #[test]
    fn a_quiet_session_in_flight_keeps_its_provider_on_the_live_interval() {
        let mut state = control();
        let claude = reset_later("claude");
        assert!(
            due_after("claude", &claude, &state) > Duration::from_secs(3600),
            "idle: wait for the reset"
        );
        state.live.insert("claude".into());
        assert_eq!(
            due_after("claude", &claude, &state),
            Duration::ZERO,
            "live: read every 300 s"
        );
        state.running.insert("claude".into());
        assert_eq!(due_after("claude", &claude, &state), Duration::ZERO);
    }

    #[test]
    fn only_providers_this_macs_work_moves_wait_for_their_reset() {
        let mut state = control();
        for provider in ["copilot", "antigravity", "devin", "kimi"] {
            assert!(
                due_after(provider, &reset_later(provider), &state) > Duration::from_secs(3600),
                "{provider}"
            );
        }
        for provider in ["codex", "cursor", "grok-bot"] {
            assert_eq!(
                due_after(provider, &reset_later(provider), &state),
                Duration::ZERO,
                "{provider}"
            );
        }
        // Work that finished since the last reading is read at once, but only
        // where this Mac's work moves the windows.
        let later = chrono::Utc::now().timestamp_millis();
        state.finished.insert("opencode".into(), later);
        assert_eq!(
            due_after("glm", &reset_later("glm"), &state),
            Duration::ZERO,
            "an open agent's work is the plans'"
        );
    }

    #[test]
    fn a_missed_answer_keeps_a_fresh_reading_as_it_was_and_backs_off() {
        let mut reading = AccountReading::empty("claude");
        reading.quotas = vec![json!({"id":"w","usedPercent":10})];
        let checked = chrono::Utc::now().to_rfc3339();
        let mut entry = entry(reading, Some(&checked), false);
        let now = Instant::now();
        let now_ms = chrono::Utc::now().timestamp_millis();
        failed_reading(&mut entry, Unread::transient("offline"), now, now_ms);
        assert!(!entry.failed, "within the grace nothing changes on screen");
        assert_eq!(entry.notice, None);
        assert_eq!(entry.failures, 1);
        let state = control();
        let first = due_after("claude", &entry, &state);
        assert!(first > Duration::from_secs(55) && first <= Duration::from_secs(60));
        failed_reading(&mut entry, Unread::transient("offline"), now, now_ms);
        failed_reading(&mut entry, Unread::transient("offline"), now, now_ms);
        let third = due_after("claude", &entry, &state);
        assert!(third > Duration::from_secs(235) && third <= Duration::from_secs(240));
        for _ in 0..10 {
            failed_reading(&mut entry, Unread::transient("offline"), now, now_ms);
        }
        assert!(due_after("claude", &entry, &state) <= MOST_BACKOFF);
        // A refresh the user asks for tries again at once, past the spacing.
        let mut asked = control();
        asked.force.insert("claude".into());
        assert!(due_after("claude", &entry, &asked) <= Duration::from_secs(60));
    }

    #[test]
    fn an_old_reading_or_a_sign_out_shows_the_failure_at_once() {
        let now = Instant::now();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let old = (chrono::Utc::now() - chrono::Duration::minutes(11)).to_rfc3339();
        let mut stale = entry(AccountReading::empty("claude"), Some(&old), false);
        failed_reading(&mut stale, Unread::transient("offline"), now, now_ms);
        assert!(stale.failed);
        assert_eq!(stale.failure, Some("transient"));
        assert_eq!(stale.notice.as_deref(), Some("offline"));

        let fresh = chrono::Utc::now().to_rfc3339();
        let mut signed_out = entry(AccountReading::empty("claude"), Some(&fresh), false);
        failed_reading(
            &mut signed_out,
            Unread::SignedOut("sign in again".into()),
            now,
            now_ms,
        );
        assert!(signed_out.failed, "the agent's own state is not hidden");
        assert_eq!(signed_out.failure, Some("signedOut"));
        assert_eq!(signed_out.failing_since, Some(now_ms));
    }

    #[test]
    fn a_failed_reading_brings_the_agents_own_record_where_it_is_newer() {
        let facts = &agents::claude::account::PROVIDER;
        let now = chrono::Utc::now();
        let ago = |minutes: i64| (now - chrono::Duration::minutes(minutes)).to_rfc3339();
        // The windows the agent recorded `minutes` ago, brought by a failure.
        let record = |minutes: i64, used: u64, failure: Unread| {
            let mut record = AccountReading::empty("claude");
            record.account = Some(json!({"id":"acct","label":"me","checkedAt":ago(minutes)}));
            record.quotas = vec![
                json!({"id":"acct:claude-session","usedPercent":used,"accountId":"acct","observedAt":ago(minutes)}),
                json!({"id":"acct:claude-weekly-fable","usedPercent":3,"accountId":"acct","observedAt":ago(minutes)}),
            ];
            record.failure = Some(failure);
            Ok(record)
        };
        let attempted = (Instant::now(), now.timestamp_millis());

        // Nothing read yet: the record shows, with the failure as it is.
        let mut entries = BTreeMap::new();
        let signed_out = || Unread::SignedOut("sign in again".into());
        settle(
            &mut entries,
            facts,
            record(120, 30, signed_out()),
            attempted,
            0,
        );
        let kept = &entries["claude"];
        assert!(kept.failed);
        assert_eq!(kept.failure, Some("signedOut"));
        assert_eq!(kept.notice.as_deref(), Some("sign in again"));
        assert_eq!(kept.checked_at, None, "no good reading yet");
        assert_eq!(kept.reading.quotas.len(), 2);
        assert_eq!(
            kept.reading.account.as_ref().unwrap()["checkedAt"],
            ago(120)
        );

        // An older record leaves the last good reading as it was.
        let mut good = AccountReading::empty("claude");
        good.account = Some(json!({"id":"acct","label":"me"}));
        good.quotas = vec![json!({"id":"acct:claude-session","usedPercent":40,"accountId":"acct"})];
        let mut entries = BTreeMap::from([("claude", entry(good, Some(&ago(2)), false))]);
        settle(
            &mut entries,
            facts,
            record(3, 35, signed_out()),
            attempted,
            0,
        );
        let kept = &entries["claude"];
        assert_eq!(kept.reading.quotas[0]["usedPercent"], 40);
        assert!(kept.reading.quotas[0]["observedAt"].is_null());
        assert!(kept.reading.account.as_ref().unwrap()["checkedAt"].is_null());
        assert_eq!(
            kept.reading.quotas[1]["id"], "acct:claude-weekly-fable",
            "a window it does not show is added"
        );
        assert!(kept.failed, "a sign-out shows at once");

        // A newer one takes its place; a failure that passes still changes
        // nothing else within the grace.
        let mut good = AccountReading::empty("claude");
        good.account = Some(json!({"id":"acct","label":"me"}));
        good.quotas = vec![json!({"id":"acct:claude-session","usedPercent":40,"accountId":"acct"})];
        let mut entries = BTreeMap::from([("claude", entry(good, Some(&ago(2)), false))]);
        settle(
            &mut entries,
            facts,
            record(1, 45, Unread::transient("busy")),
            attempted,
            0,
        );
        let kept = &entries["claude"];
        assert_eq!(kept.reading.quotas[0]["usedPercent"], 45);
        assert_eq!(kept.reading.quotas[0]["observedAt"], ago(1));
        assert_eq!(kept.reading.account.as_ref().unwrap()["checkedAt"], ago(1));
        assert!(!kept.failed);
        assert_eq!(kept.failures, 1, "and the next try backs off");
    }

    #[test]
    fn a_service_that_asks_to_wait_is_not_asked_sooner() {
        let fresh = chrono::Utc::now().to_rfc3339();
        let mut limited = entry(AccountReading::empty("codex"), Some(&fresh), false);
        failed_reading(
            &mut limited,
            Unread::Transient {
                reason: "busy".into(),
                retry_after: Some(Duration::from_secs(1800)),
            },
            Instant::now(),
            chrono::Utc::now().timestamp_millis(),
        );
        let mut state = control();
        state.force.insert("codex".into());
        assert!(due_after("codex", &limited, &state) > Duration::from_secs(1790));
    }

    #[test]
    fn readings_run_in_opens_source_order() {
        let order = agents::ACCOUNTS
            .iter()
            .map(|facts| facts.id)
            .collect::<Vec<_>>();
        assert_eq!(&order[..3], ["claude", "codex", "deepseek"]);
    }

    #[test]
    fn a_claude_window_left_out_keeps_its_last_reading_while_the_account_stays() {
        let row = |id: &str, used: u64, account: &str| json!({"id":id,"usedPercent":used,"accountId":account});
        let mut before = AccountReading::empty("claude");
        before.account = Some(json!({"id":"acct-a"}));
        before.quotas = vec![
            row("a:claude-session", 10, "acct-a"),
            row("a:claude-weekly-opus", 40, "acct-a"),
        ];
        let previous = entry(before, Some("2026-10-21T08:00:00+00:00"), false);
        let now = date_ms("2026-10-21T09:00:00+00:00").unwrap();

        let mut next = AccountReading::empty("claude");
        next.account = Some(json!({"id":"acct-a"}));
        next.quotas = vec![row("a:claude-session", 12, "acct-a")];
        retain_windows(Some(&previous), &mut next, now);
        assert_eq!(next.quotas.len(), 2);
        assert_eq!(
            next.quotas[0]["usedPercent"], 12,
            "a reported window takes its new reading"
        );
        assert_eq!(
            next.quotas[1]["usedPercent"], 40,
            "a window left out keeps its last one"
        );
        assert_eq!(
            next.quotas[1]["observedAt"], "2026-10-21T08:00:00+00:00",
            "and the time it was read"
        );

        let mut other = AccountReading::empty("claude");
        other.account = Some(json!({"id":"acct-b"}));
        other.quotas = vec![row("b:claude-session", 1, "acct-b")];
        retain_windows(Some(&previous), &mut other, now);
        assert_eq!(
            other.quotas.len(),
            1,
            "another account's windows are not kept"
        );

        // An account read from another Claude folder keeps its windows too.
        let mut both = AccountReading::empty("claude");
        both.account = Some(json!({"id":"acct-b"}));
        both.additional_accounts = vec![json!({"id":"acct-a"})];
        both.quotas = vec![
            row("b:claude-session", 1, "acct-b"),
            row("a:claude-session", 12, "acct-a"),
        ];
        retain_windows(Some(&previous), &mut both, now);
        assert_eq!(both.quotas.len(), 3);
        assert_eq!(both.quotas[2]["id"], "a:claude-weekly-opus");

        let mut late = AccountReading::empty("claude");
        late.account = Some(json!({"id":"acct-a"}));
        retain_windows(Some(&previous), &mut late, now + 31 * 86_400_000);
        assert!(
            late.quotas.is_empty(),
            "a window unseen for 30 days retires"
        );

        // Only a provider that reports the windows in use keeps the others;
        // Codex reports its whole inventory.
        assert!(agents::claude::account::PROVIDER.keeps_unreported_windows);
        assert!(!agents::codex::account::PROVIDER.keeps_unreported_windows);
    }
}
