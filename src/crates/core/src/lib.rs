//! Portable domain engine. No Tauri, Swift, system frameworks or process execution.
//! Source directories and operating-system actions are supplied by the platform boundary.
//! Files are streamed and cached by offset; incomplete trailing records are retried.
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fs::{self, File},
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
};

mod accounts;
mod billing_metrics;
mod clients;
mod compressed;
mod conversation;
mod conversation_catchup;
mod conversation_collect;
pub mod conversation_feed;
mod data_version;
mod database;
pub mod demo;
mod discovery;
mod hook_events;
mod ingestion;
mod island_events;
mod legacy;
mod log_files;
mod presentation;
mod pricing;
pub mod projects;
pub mod replies;
pub mod requests;
mod session_breakdown;
mod session_usage;
mod sessions;
mod settings;
mod snapshot;
mod source_batches;
mod source_db;
mod statistics;
mod usage;
mod usage_buckets;
mod window_shares;

pub use accounts::AccountProjection;
pub(crate) use agent_hud_agents::claude::jobs as claude_jobs;
pub use agent_hud_agents::grok::cache as grok_cache;
#[cfg(test)]
pub(crate) use agent_hud_agents::transcript::live as live_facts;
pub use agent_hud_agents::{naming, protobuf};
pub(crate) use agent_hud_agents::{
    LiveLog, Log, TimelineBucket, Usage, DATA_VERSION, PARSER_VERSION,
};
pub use agent_hud_l10n as l10n;
pub(crate) use database::{
    checkpoints::{self, LogHead, ParserCache},
    metering, staging,
};
pub use discovery::ProviderDiscovery;
pub(crate) use ingestion::{due, line, LogSession};
pub(crate) use legacy::{carried_usage, legacy_database, legacy_quota};
pub(crate) use log_files::{FileStamp, LogFiles};
pub(crate) use usage::UsageProjection;

/// A discovered source supplied by a platform adapter, never a frontend path.
pub struct SourceRoot {
    pub path: PathBuf,
    pub provider: &'static str,
}

pub(crate) const MAX_LINE: usize = 1024 * 1024;
pub struct AgentHudCore {
    requests: requests::Requests,
    hook_events: hook_events::HookEvidence,
    island_events: island_events::Tracker,
    source_notice: Option<String>,
    provider_roots: Vec<(PathBuf, &'static str)>,
    settings_path: PathBuf,
    platform: String,
    /// Shows `--demo`'s sample sources rather than this machine's.
    pub(crate) demo: bool,
    settings: Value,
    enabled: HashMap<String, bool>,
    logs: HashMap<PathBuf, LogHead>,
    session_map: HashMap<(String, String), PathBuf>,
    last_synced_at_ms: i64,
    parser_cache: ParserCache,
    archives: compressed::Archives,
    conversations: compressed::Archives,
    conversation_context: conversation::Contexts,
    sessions_revision: u64,
    sessions_fingerprint: Option<u64>,
    /// What the session fingerprint was last worked out from, and when.
    sessions_checked: Option<((u64, u64, u64), i64)>,
    /// The listed session rows of the last render and what they were built
    /// from; a render with no session input changed reuses them.
    listed_rows: Option<(u64, snapshot::ListedRows)>,
    /// Each Claude session's share of its 5-hour window, and what it was
    /// worked out from.
    window_shares: Option<(u64, HashMap<PathBuf, f64>)>,
    /// Bumped each time the indexes sessions are presented from (Claude
    /// Desktop's tabs, Claude Code's jobs, WorkBuddy's metering) are read.
    projection_sources: u64,
    database: Option<Connection>,
    storage_issue: Option<String>,
    /// The data version the ledger records (`data_version`).
    data_version: Option<String>,
    /// Why the ledger is being read through, until it has been.
    indexing_run: Option<data_version::Run>,
    /// Whether a pass has listed every log since launch, so that nothing
    /// waiting means the ledger has been read through.
    listed_once: bool,
    discovery: Vec<ProviderDiscovery>,
    reads: ingestion::Reads,
    usage_projection: Option<UsageProjection>,
    accounts: AccountProjection,
    /// The platform each client's models are priced on, from `accounts`.
    price_regions: pricing::PriceRegions,
    /// When usage older than the retention was last deleted.
    ledger_expired_at: Option<i64>,
    meters: metering::Meters,
    legacy_quota_database: Option<PathBuf>,
    account_samples: HashMap<String, String>,
    statistics: statistics::State,
    /// What rates, forecasts and cost windows were last worked out from, and
    /// when (`metrics_clock`).
    metrics_clock: Option<(u64, i64)>,
    /// What the last snapshot was rendered from and when it falls due again
    /// (`render_if_changed`).
    render: snapshot::Rendered,
    claude_desktop: crate::clients::claude_desktop::ClaudeDesktop,
    /// The host's way into a session (`replies`); None in the public build.
    session_replies: Option<std::sync::Arc<dyn replies::SessionReplies>>,
    /// The store that keeps conversations (`conversation_feed`); None in the
    /// public build, which keeps none.
    conversation_sink: Option<conversation_feed::Sink>,
    /// Where the store's knowledge of each log ends, read from it once and
    /// moved on by each delivery; and the spans it lacks, to read again.
    conversation_positions: Option<HashMap<PathBuf, u64>>,
    conversation_gaps: BTreeMap<PathBuf, (u64, u64)>,
    /// Each log's turn that started without the user's words, until its
    /// first prompt line names them (`conversation_collect::Gathered`).
    conversation_awaiting: HashMap<PathBuf, String>,
    workbuddy_metering: agent_hud_agents::buddy::transcript::WorkBuddyMeteringReading,
    /// What each listed session's row shows of its breakdown, until its logs move.
    session_rows: session_breakdown::SessionRows,
}

impl AgentHudCore {
    /// The loaded preference, available before the first collection pass.
    pub fn launch_at_login_enabled(&self) -> bool {
        self.settings["launchAtLogin"] == true
    }

    pub fn new(roots: Vec<SourceRoot>, app_data: PathBuf) -> Self {
        let settings_path = app_data.join("settings.json");
        let stored: Value = fs::read(&settings_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or(Value::Null);
        let mut settings = json!({"quotaDisplay":"consumed","codexHomes":[],"antigravityCliSettings":[],"appearance":"system","appearanceFollowsScreen":false,"language":"system","clientHooks":false,"readCopilotQuota":false,"approvalWaitMinutes":10,"launchAtLogin":true,"animations":true,"hudVisible":true,"showMenuBarIcon":true,"hudTools":false,"claudeAnswers":true});
        if let Some(values) = stored["settings"].as_object() {
            for (key, value) in values {
                if settings.get(key).is_some() {
                    settings[key] = value.clone();
                }
            }
        }
        let enabled = serde_json::from_value(stored["providers"].clone()).unwrap_or_default();
        let opened = checkpoints::open(&app_data);
        let (database, logs, session_map, last_synced_at_ms, storage_issue) = match opened {
            Ok((database, logs, session_map, last_synced_at_ms)) => {
                (Some(database), logs, session_map, last_synced_at_ms, None)
            }
            Err(error) => (
                None,
                HashMap::new(),
                HashMap::new(),
                0,
                Some(format!(
                    "{}{error}",
                    l10n::text("会话解析进度无法保存：", "Checkpoint storage unavailable: ")
                )),
            ),
        };
        let recorded = database.as_ref().and_then(data_version::recorded);
        let indexing_run = database.as_ref().and_then(|_| {
            data_version::run_at_launch(recorded.as_deref(), &logs, Utc::now().timestamp_millis())
        });
        let requests = requests::Requests::default();
        requests.set_hold_ms(
            settings["approvalWaitMinutes"]
                .as_u64()
                .filter(|v| [1, 3, 5, 10, 30, 60].contains(v))
                .unwrap_or(10)
                * 60_000,
        );
        Self {
            requests,
            hook_events: hook_events::HookEvidence::default(),
            island_events: island_events::Tracker::new(),
            source_notice: None,
            provider_roots: roots
                .into_iter()
                .map(|source| (source.path, source.provider))
                .collect(),
            settings_path,
            platform: "Desktop".into(),
            demo: false,
            settings,
            enabled,
            logs,
            session_map,
            last_synced_at_ms,
            parser_cache: ParserCache::default(),
            archives: compressed::Archives::default(),
            conversations: compressed::Archives::default(),
            conversation_context: conversation::Contexts::default(),
            sessions_revision: 0,
            sessions_fingerprint: None,
            sessions_checked: None,
            listed_rows: None,
            window_shares: None,
            projection_sources: 0,
            database,
            storage_issue,
            data_version: recorded,
            indexing_run,
            listed_once: false,
            discovery: Vec::new(),
            reads: Default::default(),
            usage_projection: None,
            accounts: AccountProjection::default(),
            price_regions: pricing::PriceRegions::default(),
            ledger_expired_at: None,
            meters: Default::default(),
            legacy_quota_database: None,
            account_samples: HashMap::new(),
            statistics: Default::default(),
            metrics_clock: None,
            render: Default::default(),
            claude_desktop: Default::default(),
            session_replies: None,
            conversation_sink: None,
            conversation_positions: None,
            conversation_gaps: BTreeMap::new(),
            conversation_awaiting: HashMap::new(),
            workbuddy_metering: Default::default(),
            session_rows: Default::default(),
        }
    }

    pub(crate) fn load_parser(&mut self, path: &Path) -> Result<(Log, usize), String> {
        if let Some(log) = self.parser_cache.take(path) {
            return Ok(log);
        }
        let (mut log, bytes) = checkpoints::read(
            self.database.as_ref().ok_or_else(|| {
                l10n::text(
                    "会话解析进度暂时无法保存",
                    "Parser checkpoint storage is unavailable",
                )
            })?,
            path,
        )?;
        // A checkpoint keeps no turn text; this run's words come from its head.
        if let Some(head) = self.logs.get(path) {
            log.live.keep_words(&head.live);
        }
        Ok((log, bytes))
    }

    pub fn set_platform(&mut self, name: &str) {
        self.platform = name.to_owned();
    }

    pub fn set_provider_discovery(&mut self, discovery: Vec<ProviderDiscovery>) {
        self.discovery = discovery;
    }

    /// Whether logs are still waiting to be read: while indexing, the
    /// collector reads again every 2 s (agent-hud-native-open
    /// UsageRefresh.indexingInterval).
    pub fn indexing(&self) -> bool {
        self.waiting_reads() > 0
    }

    /// Logs still to be read, leaving out those pausing after a failed read,
    /// which are neither read nor waited for meanwhile.
    pub(crate) fn waiting_reads(&self) -> usize {
        let now = Utc::now().timestamp_millis();
        self.reads
            .pending
            .iter()
            .filter(|path| !self.reads.failed.pausing(path, now))
            .count()
    }

    pub fn file_changes(&mut self, paths: Option<HashSet<PathBuf>>) {
        self.reads.listing.note_changes(paths);
    }

    /// The cheap half of collection, between passes: a stat-only look at the
    /// logs clients hold open, whose writes can reach the watcher late, and at
    /// Claude Desktop's session files. Reads no log; true when a pass is due.
    pub fn look(&mut self) -> bool {
        let active = self.active_logs();
        let mut due = self.reads.listing.stale(&self.provider_roots, &active);
        if due {
            // The pass rechecks the logs held open at once and reads them.
            self.reads.active_checked_at = None;
        }
        // Archiving a session in Claude Desktop changes no log. Its files are
        // stat-checked here and only the changed ones read again.
        if self.claude_desktop.index.refresh(&self.provider_roots) {
            self.usage_projection = None;
            due = true;
        }
        due
    }

    /// When a pass is due with no event to wake it: the next walk of every
    /// log folder, or the first retry of a log whose read failed.
    pub fn next_pass_due(&self) -> Option<std::time::Instant> {
        let now = Utc::now().timestamp_millis();
        let retry = self
            .reads
            .pending
            .iter()
            .filter_map(|path| self.reads.failed.retry_at(path))
            .min()
            .map(|at| {
                std::time::Instant::now()
                    + std::time::Duration::from_millis(u64::try_from(at - now).unwrap_or(0))
            });
        [self.reads.listing.next_reconcile(), retry]
            .into_iter()
            .flatten()
            .min()
    }

    /// Whether logs a hook pointed at still wait to be read.
    pub fn has_reminded_reads(&self) -> bool {
        !self.reads.forced.is_empty()
    }

    pub fn remind_paths(&mut self, paths: Vec<PathBuf>) -> usize {
        let mut accepted = HashSet::new();
        for path in paths {
            let Ok(real) = path.canonicalize() else {
                continue;
            };
            for (root, source) in &self.provider_roots {
                let Ok(real_root) = root.canonicalize() else {
                    continue;
                };
                if (real == real_root || real.starts_with(&real_root))
                    && log_files::accepts_source(&real, source)
                {
                    let mapped = if root.is_file() {
                        root.clone()
                    } else {
                        match real.strip_prefix(&real_root) {
                            Ok(tail) => root.join(tail),
                            Err(_) => continue,
                        }
                    };
                    self.reads.pending.insert(mapped.clone());
                    self.reads.forced.insert(mapped.clone());
                    accepted.insert(mapped);
                    break;
                }
            }
        }
        accepted.len()
    }

    pub fn requests(&self) -> requests::Requests {
        self.requests.clone()
    }

    /// The host's way to send words into a session as its user's next
    /// message, such as an extension's own channel; None leaves the
    /// Claude desktop app's questions to be answered in its window.
    pub fn set_session_replies(
        &mut self,
        replies: Option<std::sync::Arc<dyn replies::SessionReplies>>,
    ) {
        self.session_replies = replies;
    }

    /// The store that keeps conversations; from the next collection pass,
    /// each log read hands it what it holds of them.
    pub fn set_conversation_sink(&mut self, sink: Option<conversation_feed::Sink>) {
        self.conversation_sink = sink;
        self.conversation_positions = None;
        self.conversation_gaps.clear();
    }

    pub fn ingest_hook_events(&mut self, events: Vec<agent_hud_agents::HookEvent>) {
        self.hook_events
            .ingest(&events, Utc::now().timestamp_millis());
    }

    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.reload_preferences()?;
        match method {
            "snapshot" => {
                self.collect();
                self.render_if_changed(&params)
            }
            "request_snapshot" => self.render_if_changed(&params),
            "reminder_snapshot" => {
                self.read_reminded_logs();
                self.render_if_changed(&params)
            }
            "answer_request" => {
                let id = params["id"].as_str().ok_or("Request ID is required")?;
                let value = params.get("decision").ok_or("Decision is required")?;
                let decision = value
                    .as_str()
                    .map(|kind| json!({"kind":kind}))
                    .unwrap_or_else(|| value.clone());
                let now = Utc::now().timestamp_millis();
                // A Claude desktop question's answer goes into its session
                // first; a refusal leaves the card waiting with the reason.
                self.answer_desktop_ask(id, &decision, now)?;
                self.requests.answer(id, &decision, now)?;
                Ok(json!({"kind":"ack"}))
            }
            "refresh" => {
                self.reads.listing.note_changes(None);
                self.snapshot()
            }
            "statistics" => self.statistics(params),
            "usage_buckets" => self.usage_buckets(params),
            "set_setting" => {
                let key = params["key"].as_str().ok_or("Setting key is required")?;
                let value = params.get("value").ok_or("Setting value is required")?;
                self.apply_setting(key, value)?;
                self.render_snapshot()
            }
            "set_provider" => {
                let id = params["id"].as_str().ok_or("Provider ID is required")?;
                if agent_hud_agents::by_id(id).is_none() {
                    return Err("Provider ID is not in the supported client inventory".into());
                }
                let enabled = params["enabled"]
                    .as_bool()
                    .ok_or("enabled must be a boolean")?;
                if self.enabled.get(id).copied().unwrap_or(true) != enabled {
                    self.enabled.insert(id.into(), enabled);
                    self.usage_projection = None;
                    self.meters.view = None;
                }
                self.save()?;
                // A request never reads sources; the next collection pass
                // reads a newly enabled client's logs.
                self.render_snapshot()
            }
            "sessions" => self.sessions(params),
            "session_detail" => self.session_detail(params),
            _ => Err(format!("{method} is not supported by the local runtime")),
        }
    }
}

#[cfg(test)]
mod tests;
