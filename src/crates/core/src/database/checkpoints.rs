//! Persisted presentation heads, parser cache, and incremental checkpoint/session-mapping queries.
use crate::{database::metering, LiveLog, Log, TimelineBucket, Usage};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::SystemTime,
};

const CACHE_LOGS: usize = 24;
const CACHE_BYTES: usize = 24 * 1024 * 1024;

#[derive(Clone, Default, Deserialize)]
struct OldTurn {
    timestamp: Option<String>,
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
    #[serde(default)]
    cache: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct LogHead {
    pub path: PathBuf,
    #[serde(default)]
    pub origin_path: Option<PathBuf>,
    pub source: String,
    pub id: String,
    pub title: String,
    pub model: String,
    pub cwd: Option<String>,
    /// The client that drives the session (`Log::client`).
    #[serde(default)]
    pub client: Option<String>,
    /// The session's first and latest prompts (`Log::prompts`).
    #[serde(default)]
    pub prompts: crate::naming::surfaces::PromptStamps,
    pub updated: String,
    pub usage: Usage,
    pub offset: u64,
    pub length: u64,
    pub modified: Option<SystemTime>,
    #[serde(default)]
    pub titles: HashMap<String, String>,
    #[serde(default)]
    pub observed: bool,
    #[serde(default)]
    pub parser_version: u32,
    #[serde(default)]
    pub reader_version: u16,
    #[serde(default)]
    pub live: LiveLog,
    #[serde(default)]
    pub waiting_for_more: bool,
    #[serde(default)]
    pub decoded_complete: bool,
    /// Missing source files retain their counted usage, but leave the session list.
    #[serde(default)]
    pub gone: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub meter_target_at: Option<i64>,
    #[serde(default)]
    pub buckets: BTreeMap<i64, TimelineBucket>,
    /// What the log's calls would cost at API list prices, in pico US dollars
    /// (`metering::list_cost`); None when unknown or not yet priced.
    #[serde(default)]
    pub list_cost: Option<i128>,
    /// Whether `list_cost` was worked out since the log's calls last changed;
    /// heads from before it existed are priced a few at a time.
    #[serde(default)]
    pub list_cost_priced: bool,
    /// What the calls with a list price cost, in pico US dollars; None when
    /// none has one.
    #[serde(default)]
    pub priced_cost: Option<i128>,
    /// The tokens of the calls with no list price; None until worked out,
    /// so heads priced before it existed are priced again.
    #[serde(default)]
    pub unpriced_tokens: Option<u64>,
    /// A DeepSeek log's cost at the account's own prices, in pico units of
    /// US dollars and of yuan (`metering::account_cost`).
    #[serde(default)]
    pub account_cost: Option<[Option<i128>; 2]>,
    // Unknown JSON fields (especially messages) are skipped by serde rather than
    // materialized as Value. Old UI turns are discarded after deriving buckets.
    #[serde(default, rename = "turns", skip_serializing)]
    old_turns: Vec<OldTurn>,
    #[serde(skip)]
    pub updated_at: Option<i64>,
    #[serde(skip)]
    pub fact_hash: u64,
}

impl LogHead {
    pub fn from_log(log: &Log) -> Self {
        let mut head = Self {
            path: log.path.clone(),
            origin_path: log.origin_path.clone(),
            source: log.source.clone(),
            id: log.id.clone(),
            title: log.title(),
            model: log.model.clone(),
            cwd: log.cwd.clone(),
            client: log.client.clone(),
            prompts: log.prompts.clone(),
            updated: log.updated.clone(),
            usage: log.usage,
            offset: log.offset,
            length: log.length,
            modified: log.modified,
            titles: log.titles.clone(),
            observed: log.observed,
            parser_version: log.parser_version,
            reader_version: log.reader_version,
            live: log.live.head(),
            waiting_for_more: log.waiting_for_more,
            decoded_complete: log.decoded_complete,
            gone: false,
            archived: log.archived,
            meter_target_at: log.meter_target_at,
            buckets: log.buckets.clone(),
            list_cost: None,
            list_cost_priced: false,
            priced_cost: None,
            unpriced_tokens: None,
            account_cost: None,
            old_turns: Vec::new(),
            updated_at: None,
            fact_hash: 0,
        };
        head.prepare();
        head
    }

    pub fn decode(payload: &str) -> Result<Self, serde_json::Error> {
        let mut head: Self = serde_json::from_str(payload)?;
        for turn in std::mem::take(&mut head.old_turns) {
            if let Some(at) = turn.timestamp.as_deref().and_then(metering::date_ms) {
                let key = at / 1000 / 3600 * 3600;
                head.buckets.entry(key).or_default().add(TimelineBucket {
                    input: turn.input,
                    output: turn.output,
                    cache: turn.cache,
                    entries: 1,
                });
            }
        }
        head.live = head.live.head();
        head.prepare();
        Ok(head)
    }

    fn prepare(&mut self) {
        self.updated_at = metering::date_ms(&self.updated);
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.path.hash(&mut hash);
        self.source.hash(&mut hash);
        self.id.hash(&mut hash);
        self.title.hash(&mut hash);
        self.model.hash(&mut hash);
        self.cwd.hash(&mut hash);
        self.activity_at(None).hash(&mut hash);
        self.usage.total().hash(&mut hash);
        self.list_cost.hash(&mut hash);
        self.priced_cost.hash(&mut hash);
        self.unpriced_tokens.hash(&mut hash);
        self.account_cost.hash(&mut hash);
        if let Ok(live) = serde_json::to_vec(&self.live) {
            live.hash(&mut hash);
        }
        self.live.hash_words(&mut hash);
        self.fact_hash = hash.finish();
    }

    /// Takes back the turn text `earlier`, this log's head as this run held
    /// it, kept in memory (`LiveLog::keep_words`).
    pub fn keep_words(&mut self, earlier: &LogHead) {
        if self.live.keep_words(&earlier.live) {
            self.prepare();
        }
    }

    pub fn activity_at(&self, child: Option<i64>) -> Option<i64> {
        self.live.activity_at(self.updated_at, child)
    }

    /// When the log's last message was written (`LiveLog::last_message_at`),
    /// else its last event, for a client that records no messages or a log
    /// read before the time was kept: what lists order sessions by.
    pub fn last_message_at(&self) -> Option<i64> {
        self.live.last_message_at.or_else(|| self.activity_at(None))
    }

    /// Records the log's list cost (`metering::list_cost`).
    pub(crate) fn set_list_cost(&mut self, cost: Option<metering::ListCost>) {
        self.list_cost = cost.and_then(|cost| cost.complete());
        self.priced_cost = cost.and_then(|cost| cost.priced);
        self.unpriced_tokens = Some(cost.map_or(0, |cost| cost.unpriced_tokens));
        self.list_cost_priced = true;
        self.prepare();
    }

    /// The log's list cost as it was recorded (`set_list_cost`).
    pub(crate) fn list_costs(&self) -> Option<metering::ListCost> {
        let unpriced_tokens = self.unpriced_tokens.unwrap_or(0);
        (self.priced_cost.is_some() || unpriced_tokens > 0).then_some(metering::ListCost {
            priced: self.priced_cost,
            unpriced_tokens,
        })
    }

    /// Works out the log's list cost and, for DeepSeek, its cost at the
    /// account's prices, from the ledger as it is now.
    pub fn price(&mut self, db: &Connection, path: &str) -> rusqlite::Result<()> {
        self.account_cost = metering::account_cost(db, path, &self.source)?;
        self.set_list_cost(metering::list_cost(db, path)?);
        Ok(())
    }

    /// The account-priced cost in `currency` ("USD" or "CNY"), as sessions
    /// show it; None when unknown.
    pub fn account_cost_in(&self, currency: &str) -> Option<f64> {
        let index = match currency {
            "USD" => 0,
            "CNY" => 1,
            _ => return None,
        };
        self.account_cost?[index].map(|pico| pico as f64 / 1e12)
    }

    /// The list cost in US dollars, as sessions show it.
    pub fn list_cost_usd(&self) -> Option<f64> {
        self.list_cost.map(|pico| pico as f64 / 1e12)
    }

    pub fn session(&self, now: i64, subagents_at: Option<i64>) -> Value {
        let updated = self
            .activity_at(subagents_at)
            .and_then(DateTime::<Utc>::from_timestamp_millis)
            .map(|at| at.to_rfc3339())
            .unwrap_or_else(|| self.updated.clone());
        let mut session = json!({"id":self.id,"title":self.title,"source":self.source,"model":self.model,"vendorName":crate::naming::model_names::vendor_name(&self.source),"modelName":(!self.model.is_empty()).then(||crate::naming::model_names::consumer_name(&format!("{}-model:{}",self.source,self.model))),"cwd":self.cwd,"updatedAt":updated,"state":self.live.session_state(now,&self.source,subagents_at),"tokens":self.usage.total(),"cost":self.list_cost_usd(),"contextPercent":null,"logPath":self.origin_path.as_ref().unwrap_or(&self.path),"parentId":self.live.parent_id});
        session["project"] = json!(crate::projects::project_id(self.cwd.as_deref()));
        session["lastMessageAt"] = json!(self
            .last_message_at()
            .and_then(DateTime::<Utc>::from_timestamp_millis)
            .map(|at| at.to_rfc3339())
            .unwrap_or_else(|| self.updated.clone()));
        // `tokens` by kind, from the same log: the Sessions page's Today card
        // splits its total by them (agent-hud-native-open `SessionsTodayCard`).
        // These are the log's own; `AgentHudCore::session_totals` adds its
        // sub-agents' wherever a session is shown.
        session["usage"] = self.usage.json();
        if let Some(client) =
            crate::naming::clients::client_name(&self.source, self.client.as_deref())
        {
            session["client"] = json!(client);
        }
        // Where its first and latest prompts were typed, as the log tells;
        // the presentation adds what hooks and deliveries report.
        session["origins"] = crate::naming::surfaces::resolve(
            &self.source,
            &self.prompts,
            self.client.as_deref(),
            &Default::default(),
        )
        .json();
        if self.source == "grok-bot" {
            if let Some(id) = self.id.rsplit(':').next() {
                session["navigationTarget"] = json!({"kind":"grok-bot-agent","id":id})
            }
        }
        if self.source == "cursor" {
            if let Some(id) = self.id.strip_prefix("cursor:") {
                session["navigationTarget"] = json!({"kind":"cursor-agent","id":id,"cwd":self.cwd})
            }
        }
        if self.source == "antigravity" && self.client.as_deref() != Some("CLI") {
            if let Some(id) = self.id.strip_prefix("antigravity:") {
                session["navigationTarget"] =
                    json!({"kind":"antigravity-conversation","id":id,"cwd":self.cwd})
            }
        }
        session
    }

    pub fn live_session(&self, now: i64, subagents_at: Option<i64>) -> Value {
        let mut session = self.session(now, subagents_at);
        if let Some(facts) = self.live.facts(&self.id, &self.source, now, subagents_at) {
            session["liveFacts"] = facts;
        }
        session
    }

    pub fn listed_recent(&self, now: i64, days: i64, child: Option<i64>) -> bool {
        matches!(
            self.live.session_state(now, &self.source, child),
            "running" | "waiting"
        ) || self
            .activity_at(child)
            .is_some_and(|at| at >= now - days * 86_400_000)
    }
}

#[derive(Default)]
pub(crate) struct ParserCache {
    entries: VecDeque<(PathBuf, Log, usize)>,
    bytes: usize,
}

impl ParserCache {
    pub fn take(&mut self, path: &Path) -> Option<(Log, usize)> {
        let index = self.entries.iter().position(|(key, _, _)| key == path)?;
        let (_, log, bytes) = self.entries.remove(index)?;
        self.bytes -= bytes;
        Some((log, bytes))
    }

    pub fn remove(&mut self, path: &Path) {
        self.take(path);
    }

    pub fn insert(&mut self, log: Log, bytes: usize) {
        self.remove(&log.path);
        if bytes > CACHE_BYTES {
            return;
        }
        while self.entries.len() >= CACHE_LOGS || self.bytes + bytes > CACHE_BYTES {
            if let Some((_, _, size)) = self.entries.pop_front() {
                self.bytes -= size;
            } else {
                break;
            }
        }
        self.bytes += bytes;
        self.entries.push_back((log.path.clone(), log, bytes));
    }
}

pub(crate) fn open(
    directory: &Path,
) -> Result<
    (
        Connection,
        HashMap<PathBuf, LogHead>,
        HashMap<(String, String), PathBuf>,
        i64,
    ),
    String,
> {
    super::open(directory)
}

pub(crate) fn read(database: &Connection, path: &Path) -> Result<(Log, usize), String> {
    let payload: String = database
        .query_row(
            "SELECT payload FROM ingestion_logs WHERE path=?1",
            [path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .map_err(|_| "Parser checkpoint could not be read".to_owned())?;
    let mut log: Log = serde_json::from_str(&payload)
        .map_err(|_| "Parser checkpoint could not be decoded".to_owned())?;
    log.restore_buckets();
    Ok((log, payload.len()))
}

pub(crate) fn upsert_log(
    db: &Connection,
    path: &str,
    payload: &str,
    head: &str,
    updated_at_ms: i64,
) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO ingestion_logs(path,payload,head,updated_at_ms) VALUES(?1,?2,?3,?4)
         ON CONFLICT(path) DO UPDATE SET payload=excluded.payload,head=excluded.head,updated_at_ms=excluded.updated_at_ms",
        params![path, payload, head, updated_at_ms],
    )?;
    Ok(())
}

pub(crate) fn update_head(
    db: &Connection,
    path: &str,
    head: &str,
    updated_at_ms: i64,
) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE ingestion_logs SET head=?1, updated_at_ms=?2 WHERE path=?3",
        params![head, updated_at_ms, path],
    )?;
    Ok(())
}

pub(crate) fn delete_log(db: &Connection, path: &str) -> rusqlite::Result<()> {
    db.execute("DELETE FROM ingestion_logs WHERE path=?1", [path])?;
    Ok(())
}

pub(crate) fn rename_log(
    db: &Connection,
    from_path: &str,
    to_path: &str,
    payload: &str,
    head: &str,
    updated_at_ms: i64,
) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE ingestion_logs SET path=?1,payload=?2,head=?3,updated_at_ms=?4 WHERE path=?5",
        params![to_path, payload, head, updated_at_ms, from_path],
    )?;
    Ok(())
}

pub(crate) fn upsert_session_mapping(
    db: &Connection,
    source: &str,
    session_id: &str,
    canonical_path: &str,
    parent_id: Option<&str>,
    updated_at_ms: i64,
) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO session_mapping(source,session_id,canonical_path,parent_id,updated_at_ms)
         VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(source,session_id) DO UPDATE SET
           canonical_path=excluded.canonical_path,
           parent_id=coalesce(excluded.parent_id,session_mapping.parent_id),
           updated_at_ms=excluded.updated_at_ms",
        params![source, session_id, canonical_path, parent_id, updated_at_ms],
    )?;
    Ok(())
}

pub(crate) struct SyncChanges {
    pub heads: Vec<(PathBuf, LogHead)>,
    pub mappings: Vec<((String, String), PathBuf)>,
    pub max_updated_at_ms: i64,
    pub changed: bool,
}

/// Queries all records modified after `since_ms` across `ingestion_logs`, `session_mapping`,
/// `usage_meter_event`, and `usage_meter_bucket`. The rows of the logs in `held` move the time
/// on but are neither read back nor changes: the run that wrote them holds them already.
pub(crate) fn sync_since(
    db: &Connection,
    since_ms: i64,
    held: &[&str],
) -> Result<SyncChanges, String> {
    let mut max_ts = since_ms;
    let mut changed = false;
    let mut heads = Vec::new();
    {
        let mut stmt = db
            .prepare(
                "SELECT path, head, updated_at_ms FROM ingestion_logs WHERE updated_at_ms > ?1",
            )
            .map_err(|_| "Could not read updated checkpoints")?;
        let rows = stmt
            .query_map([since_ms], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|_| "Could not query updated checkpoints")?;
        for row in rows {
            let (path_str, head_json, ts) =
                row.map_err(|_| "Could not decode updated checkpoint")?;
            max_ts = max_ts.max(ts);
            if held.contains(&path_str.as_str()) {
                continue;
            }
            if let Some(head_json) = head_json {
                let mut head: LogHead =
                    LogHead::decode(&head_json).map_err(|_| "Could not parse checkpoint head")?;
                let path = PathBuf::from(path_str);
                head.path = path.clone();
                heads.push((path, head));
                changed = true;
            }
        }
    }
    let mut mappings = Vec::new();
    {
        let mut stmt = db
            .prepare("SELECT source, session_id, canonical_path, updated_at_ms FROM session_mapping WHERE updated_at_ms > ?1")
            .map_err(|_| "Could not read updated session mappings")?;
        let rows = stmt
            .query_map([since_ms], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|_| "Could not query updated session mappings")?;
        for row in rows {
            let (source, session_id, canonical_path, ts) =
                row.map_err(|_| "Could not decode session mapping")?;
            max_ts = max_ts.max(ts);
            if held.contains(&canonical_path.as_str()) {
                continue;
            }
            mappings.push(((source, session_id), PathBuf::from(canonical_path)));
            changed = true;
        }
    }
    // Only the rows written since, through the updated_at_ms index: grouping
    // them in SQL had SQLite walk every row in contribution order instead.
    for sql in [
        "SELECT contribution, updated_at_ms FROM usage_meter_event WHERE updated_at_ms > ?1",
        "SELECT contribution, updated_at_ms FROM usage_meter_bucket WHERE updated_at_ms > ?1",
    ] {
        let Ok(mut stmt) = db.prepare_cached(sql) else {
            continue;
        };
        let Ok(rows) = stmt.query_map([since_ms], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        }) else {
            continue;
        };
        for (contribution, ts) in rows.flatten() {
            max_ts = max_ts.max(ts);
            changed |= !held.contains(&contribution.as_str());
        }
    }
    Ok(SyncChanges {
        heads,
        mappings,
        max_updated_at_ms: max_ts,
        changed,
    })
}
