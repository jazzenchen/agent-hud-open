//! A log's persisted presentation head: what the session list shows of it without reading the log.
use crate::{database::metering, LiveLog, Log, TimelineBucket, Usage};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    hash::{Hash, Hasher},
    path::PathBuf,
    time::SystemTime,
};

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
        serde_json::to_vec(&self.live).unwrap().hash(&mut hash);
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
        // Its way back, as its agent says; without one an app link may stand
        // in (the presentation adds it).
        if let Some(target) = agent_hud_agents::session_target(
            &self.source,
            &self.id,
            self.cwd.as_deref(),
            self.client.as_deref(),
        ) {
            session["navigationTarget"] = target;
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
