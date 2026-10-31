//! Provider account readings on one worker, one at a time, with one result
//! cache outside the Runtime mutex. A reading holds the collection gate, so it
//! never overlaps a local pass either.
//! Cadence follows UsageRefresh: 300s quiet, 150s running, >=60s per account.
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
}
use agent_hud_agents::account::Reading as AccountReading;

type Waker = Arc<dyn Fn() + Send + Sync>;
use agent_hud_agents as agents;
type Reader = fn(&dyn agents::Host) -> Result<AccountReading, String>;
/// One reading at a time, in this order, as UsageCollector runs the sources'
/// account steps: Claude, Codex, DeepSeek, then the additional sources in
/// AdditionalSource order, then the open agents (CombinedUsageProvider).
const READERS: [(&str, Reader); 12] = [
    ("claude", agents::claude::account::read),
    ("codex", agents::codex::account::read),
    ("deepseek", agents::deepseek::account::read),
    ("antigravity", agents::antigravity::account::read),
    ("cursor", agents::cursor::account::read),
    ("grok-bot", agents::grok::account::read),
    ("copilot", agents::copilot::account::read),
    ("devin", agents::devin::account::read),
    ("kimi", agents::coding_plans::kimi),
    ("glm", agents::coding_plans::glm),
    ("opencode", agents::coding_plans::opencode_go),
    ("relay", agents::relays::read),
];

/// Whether this Mac's work moves the provider's windows. Codex's are spent
/// by Pi and other clients without Codex rollouts, Cursor's on every device
/// the account signs in on, and Grok Bot's by bots running on Cursor's
/// servers, so theirs are read on the account interval whatever this Mac
/// does (agent-hud-native-open `seesLocalWork`).
fn sees_local_work(provider: &str) -> bool {
    !matches!(provider, "codex" | "cursor" | "grok-bot")
}

/// The session sources whose work is the provider's: Grok's quota covers
/// its CLI and its Bot, and the open agents' plans are read as one source
/// whose sessions are OpenCode's, Kimi's, GLM's and Pi's (agent-hud-native-open
/// OpenAgentSessions).
fn work_sources(provider: &str) -> &[&str] {
    match provider {
        "grok-bot" => &["grok-bot", "grok-cli"],
        "kimi" | "glm" | "opencode" => &["opencode", "kimi", "glm", "pi"],
        // A relay is read sooner while the clients it is found in work.
        "relay" => &["claude", "codex"],
        _ => READERS
            .iter()
            .find(|(id, _)| *id == provider)
            .map_or(&[], |(id, _)| std::slice::from_ref(id)),
    }
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
    failed: bool,
    spread: u64,
    /// From the restart copy and not yet read in this run: due at once.
    restored: bool,
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
}

impl AccountService {
    pub fn new(
        home: PathBuf,
        data_dir: PathBuf,
        enabled: HashSet<String>,
        gate: CollectionGate,
    ) -> Result<Self, String> {
        Self::with_readers(home, data_dir, enabled, gate, &READERS)
    }
    fn with_readers(
        home: PathBuf,
        data_dir: PathBuf,
        enabled: HashSet<String>,
        gate: CollectionGate,
        readers: &'static [(&'static str, Reader)],
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
                account_homes: restored.account_homes,
                waker: None,
            }),
            wake: Condvar::new(),
            children,
            cancelled,
        });
        let worker = Arc::clone(&shared);
        let worker_context = context.clone();
        std::thread::Builder::new()
            .name("agenthud-accounts".into())
            .spawn(move || run(worker_context, worker, gate, readers))
            .map_err(|_| l10n::text("账户读取线程无法启动", "Account worker could not start"))?;
        Ok(Self {
            shared,
            context,
            credential_lock: Mutex::new(()),
        })
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
        for (provider, _) in READERS {
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
                    .filter(|row| !expired_cli_sample(row, "observedAt", now))
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
                            || stale_cli_sample(&row, "observedAt", now)
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
                if expired_cli_sample(account, "checkedAt", now) {
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
                        || stale_cli_sample(&account, "checkedAt", now)
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
            result.providers.insert(entry.reading.provider.into(),json!({"status":if entry.failed {"error"}else if ready {"ready"}else{"unavailable"},"checkedAt":entry.checked_at,"notice":entry.notice}));
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

fn run(
    context: AccountContext,
    shared: Arc<Shared>,
    gate: CollectionGate,
    readers: &'static [(&'static str, Reader)],
) {
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
                for &(provider, reader) in readers {
                    if !state.enabled.contains(provider) {
                        continue;
                    }
                    let wait = state
                        .entries
                        .get(provider)
                        .map_or(Duration::ZERO, |entry| due_after(provider, entry, &state));
                    if wait.is_zero() {
                        due = Some((provider, reader));
                        break;
                    }
                    pause = pause.min(wait);
                }
                let now = Instant::now();
                if let Some(at) = resume_at.filter(|at| *at > now) {
                    pause = at - now;
                } else if let Some(selected) = due {
                    if burst.is_none_or(|started| started.elapsed() < STEP_BUDGET) {
                        resume_at = None;
                        // Consume the request this job serves. A callback arriving
                        // during I/O remains pending for the next permitted pass.
                        state.force.remove(selected.0);
                        break (selected.0, selected.1, state.account_homes.clone());
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
            (selected.1)(&reading_context)
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
            match result {
                Ok(mut reading) => {
                    retain_windows(state.entries.get(selected.0), &mut reading, attempted_ms);
                    let notice = reading.notice.clone();
                    state.entries.insert(
                        selected.0,
                        Entry {
                            reading,
                            checked_at: Some(chrono::Utc::now().to_rfc3339()),
                            attempted,
                            attempted_ms,
                            notice,
                            failed: false,
                            spread,
                            restored: false,
                        },
                    );
                }
                Err(error) => {
                    let entry = state.entries.entry(selected.0).or_insert_with(|| Entry {
                        reading: AccountReading::empty(selected.0),
                        checked_at: None,
                        attempted,
                        attempted_ms,
                        notice: None,
                        failed: true,
                        spread,
                        restored: false,
                    });
                    entry.attempted = attempted;
                    entry.attempted_ms = attempted_ms;
                    entry.notice = Some(error);
                    entry.failed = true;
                    entry.spread = spread;
                    entry.restored = false;
                }
            }
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

/// How long a window the provider stopped reporting keeps its last reading
/// (agent-hud-native-open QuotaHistoryStore.retention).
const WINDOW_RETENTION_MS: i64 = 30 * 86_400_000;

fn cli_sample_age(row: &Value, time_key: &str, now: i64) -> Option<i64> {
    (row["source"] == "antigravity_cli")
        .then(|| {
            row[time_key]
                .as_str()
                .and_then(date_ms)
                .map(|at| now.saturating_sub(at))
        })
        .flatten()
}

fn stale_cli_sample(row: &Value, time_key: &str, now: i64) -> bool {
    cli_sample_age(row, time_key, now)
        .is_some_and(|age| age > agents::antigravity::statusline::STALE_AFTER_SECONDS * 1000)
}

fn expired_cli_sample(row: &Value, time_key: &str, now: i64) -> bool {
    cli_sample_age(row, time_key, now).is_some_and(|age| age >= WINDOW_RETENTION_MS)
}

/// An absent reading is not a reset (agent-hud-native-open
/// RetainedUsageProvider.retainingReadings): a Claude window this reading
/// left out keeps its last reading, and the time it was read, while the same
/// account is signed in and for at most 30 days. Codex's app-server reports
/// its complete window inventory, so a window it leaves out retires at once;
/// the other providers' rules belong to their own readers.
fn retain_windows(previous: Option<&Entry>, reading: &mut AccountReading, now: i64) {
    if reading.provider != "claude" {
        return;
    }
    let (Some(previous), Some(account)) = (previous, reading.account.as_ref()) else {
        return;
    };
    let account = account["id"].clone();
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
        .filter(|row| row["accountId"] == account)
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
    // Failed account reads use the same provider due calculation. Swift's 5s
    // local-source retry is separate; account requests are never <60s apart.
    pause.max(Duration::from_secs(60).saturating_sub(elapsed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    static ACTIVE: AtomicUsize = AtomicUsize::new(0);
    static MOST: AtomicUsize = AtomicUsize::new(0);
    static DONE: AtomicUsize = AtomicUsize::new(0);
    static GATE: Mutex<Option<CollectionGate>> = Mutex::new(None);

    fn reading(provider: &'static str) -> Result<AccountReading, String> {
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
    fn first(_: &dyn agents::Host) -> Result<AccountReading, String> {
        reading("first")
    }
    fn second(_: &dyn agents::Host) -> Result<AccountReading, String> {
        reading("second")
    }
    fn third(_: &dyn agents::Host) -> Result<AccountReading, String> {
        reading("third")
    }
    static FAKE: [(&str, Reader); 3] = [("first", first), ("second", second), ("third", third)];

    fn control() -> Control {
        Control {
            stopped: false,
            force: HashSet::new(),
            enabled: HashSet::new(),
            running: HashSet::new(),
            live: HashSet::new(),
            finished: HashMap::new(),
            entries: BTreeMap::new(),
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
        fn blocked(host: &dyn agents::Host) -> Result<AccountReading, String> {
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
        static READERS: [(&str, Reader); 1] = [("antigravity", blocked)];
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
        }
    }

    #[test]
    fn configured_homes_enable_undetected_accounts_and_changes_retire_old_readings() {
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
            "settings": {"codexHomes": ["~/work"]},
            "providers": [{"id":"codex", "enabled":true, "detected":false}],
            "sessions": [],
        });
        service.update_sources(&snapshot);
        {
            let mut state = service.shared.state.lock().unwrap();
            assert!(state.enabled.contains("codex"));
            assert_eq!(
                state.account_homes["codex"],
                vec![directory.path().join("work")]
            );
            state
                .entries
                .insert("codex", entry(AccountReading::empty("codex"), None, false));
        }
        // Routine snapshot updates preserve the cache; edits retire its old homes.
        service.update_sources(&snapshot);
        assert!(service
            .shared
            .state
            .lock()
            .unwrap()
            .entries
            .contains_key("codex"));
        snapshot["settings"]["codexHomes"] = json!([]);
        service.update_sources(&snapshot);
        {
            let state = service.shared.state.lock().unwrap();
            assert!(!state.entries.contains_key("codex"));
            assert!(!state.enabled.contains("codex"));
            assert!(state.account_homes.is_empty());
        }
        snapshot["settings"]["codexHomes"] = json!(["~/work"]);
        snapshot["providers"][0]["enabled"] = json!(false);
        service.update_sources(&snapshot);
        assert!(!service
            .shared
            .state
            .lock()
            .unwrap()
            .enabled
            .contains("codex"));
        assert!(service
            .validate_account_homes(&json!(["relative"]))
            .is_err());
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
        fn unread(_: &dyn agents::Host) -> Result<AccountReading, String> {
            panic!("the collection gate keeps this fixture read-only")
        }
        static READERS: [(&str, Reader); 2] = [("codex", unread), ("antigravity", unread)];
        for (provider, setting) in [
            ("codex", "codexHomes"),
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
    fn readings_run_in_opens_source_order() {
        let order = READERS.iter().map(|(id, _)| *id).collect::<Vec<_>>();
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

        let mut late = AccountReading::empty("claude");
        late.account = Some(json!({"id":"acct-a"}));
        retain_windows(Some(&previous), &mut late, now + 31 * 86_400_000);
        assert!(
            late.quotas.is_empty(),
            "a window unseen for 30 days retires"
        );

        let mut codex = AccountReading::empty("codex");
        codex.account = Some(json!({"id":"acct-a"}));
        retain_windows(Some(&previous), &mut codex, now);
        assert!(codex.quotas.is_empty(), "Codex reports its whole inventory");
    }
}
