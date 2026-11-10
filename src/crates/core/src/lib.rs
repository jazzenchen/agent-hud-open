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
mod dispatch;
mod hook_events;
mod host;
mod ingestion;
mod island_events;
mod legacy;
mod log_files;
mod plan_value;
mod presentation;
mod prices;
mod pricing;
pub mod projects;
mod quota_history;
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
pub(crate) use ingestion::{line, needs_read};
pub(crate) use legacy::{carried_usage, legacy_database, legacy_quota};
pub(crate) use log_files::{FileStamp, LogFiles};
pub(crate) use usage::UsageProjection;

/// A discovered source supplied by a platform adapter, never a frontend path.
pub struct SourceRoot {
    pub path: PathBuf,
    pub provider: &'static str,
}

pub(crate) const MAX_LINE: usize = 1024 * 1024;
/// How often a working Grok Bot turn is looked at again, in ms.
pub(crate) const BOT_POLL_MS: i64 = 10_000;

/// A time in Unix ms as RFC 3339 text; null out of range.
pub(crate) fn iso(at: i64) -> Value {
    json!(DateTime::<Utc>::from_timestamp_millis(at).map(|at| at.to_rfc3339()))
}

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
    sessions_checked: Option<((u64, u64, u64, u64), i64)>,
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
    claude_projects_supported: bool,
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
    /// What each listed session's row shows of its breakdown (a key of its
    /// logs' read offsets, and the value), until its logs move.
    session_rows: std::cell::RefCell<HashMap<PathBuf, (u64, Value)>>,
}

impl AgentHudCore {
    pub fn new(roots: Vec<SourceRoot>, app_data: PathBuf) -> Self {
        let settings_path = app_data.join("settings.json");
        let stored: Value = fs::read(&settings_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or(Value::Null);
        let mut settings = json!({"quotaDisplay":"remaining","codexHomes":[],"claudeHomes":[],"antigravityCliSettings":[],"appearance":"system","appearanceFollowsScreen":false,"language":"system","clientHooks":false,"optInApprovals":[],"readCopilotQuota":false,"approvalWaitMinutes":10,"launchAtLogin":true,"animations":true,"hudVisible":true,"showMenuBarIcon":true,"hudTools":false,"claudeAnswers":true,"agentMessages":false,"agentMessagesAsUser":false,"agentMessagesLimit":10,"claudeMessages":true,"deepseekMessages":true,"relayReading":[],"customPrices":{},"relayMultipliers":{},"localKinds":{}});
        if let Some(values) = stored["settings"].as_object() {
            for (key, value) in values {
                if settings.get(key).is_some() {
                    settings[key] = value.clone();
                }
            }
        }
        let enabled = serde_json::from_value(stored["providers"].clone()).unwrap_or_default();
        let opened = database::open(&app_data);
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
        let indexing_run = database
            .as_ref()
            .and_then(|_| data_version::run_at_launch(recorded.as_deref(), &logs));
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
            meters: Default::default(),
            legacy_quota_database: None,
            account_samples: HashMap::new(),
            statistics: Default::default(),
            metrics_clock: None,
            render: Default::default(),
            claude_desktop: Default::default(),
            claude_projects_supported: false,
            session_replies: None,
            conversation_sink: None,
            conversation_positions: None,
            conversation_gaps: BTreeMap::new(),
            conversation_awaiting: HashMap::new(),
            workbuddy_metering: Default::default(),
            session_rows: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests;
