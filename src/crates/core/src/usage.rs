//! Token counts as the ledger keeps them, and the usage projection a snapshot
//! is drawn from until a source write changes it.
use crate::{AgentHudCore, LogHead, TimelineBucket, Usage};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
};

/// Usage changes and display-clock changes have separate lifetimes, as in
/// UsageLedger/UsageStore: only source writes invalidate this numeric projection.
pub(crate) struct UsageProjection {
    pub(crate) selected: Vec<PathBuf>,
    pub(crate) totals: Usage,
    pub(crate) timeline: Vec<Value>,
    pub(crate) titles: HashMap<String, String>,
    pub(crate) context_percents: HashMap<String, f64>,
    pub(crate) observed_providers: HashSet<String>,
    pub(crate) counts: HashMap<String, usize>,
}

impl AgentHudCore {
    /// The logs whose usage counts: one copy per session, internal ones
    /// (Codex's guardian reviews) included, as their usage counts in Open;
    /// `listed_sessions` leaves those out of every list.
    /// The logs whose usage counts (`select`), as the usage projection holds
    /// them: it keeps the selection it was built on until a change to the
    /// logs, the session map or the enabled clients drops it, so the queries a
    /// pass makes do not pick and sort every log again.
    pub(crate) fn selected(&self) -> Vec<&LogHead> {
        match &self.usage_projection {
            Some(projection) => projection
                .selected
                .iter()
                .filter_map(|path| self.logs.get(path))
                .collect(),
            None => self.select(),
        }
    }

    fn select(&self) -> Vec<&LogHead> {
        // A log is its session's mapped copy when the mapping holds exactly
        // (source, id, path); borrowed keys avoid two String clones per log.
        let mapped = self
            .session_map
            .iter()
            .map(|((source, id), path)| (source.as_str(), id.as_str(), path.as_path()))
            .collect::<HashSet<_>>();
        let mapped_copy = |log: &LogHead| {
            mapped.contains(&(log.source.as_str(), log.id.as_str(), log.path.as_path()))
        };
        let mut sessions: HashMap<(&str, &str), &LogHead> = HashMap::new();
        for log in self.logs.values().filter(|log| {
            log.source != "codex-index"
                && self
                    .enabled
                    .get(log.source.as_str())
                    .copied()
                    .unwrap_or(true)
        }) {
            let is_mapped = mapped_copy(log);
            sessions
                .entry((log.source.as_str(), &log.id))
                .and_modify(|current| {
                    let current_mapped = mapped_copy(current);
                    // Of a session's copies, a listed one before a gone one,
                    // then the most recently modified counts (agent-hud-native-open
                    // LedgerCopies); usage and the mapping only break ties.
                    if (
                        !log.gone,
                        log.modified,
                        log.usage.total(),
                        is_mapped,
                        std::cmp::Reverse(&log.path),
                    ) > (
                        !current.gone,
                        current.modified,
                        current.usage.total(),
                        current_mapped,
                        std::cmp::Reverse(&current.path),
                    ) {
                        *current = log;
                    }
                })
                .or_insert(log);
        }
        let mut logs: Vec<_> = sessions.into_values().collect();
        logs.sort_by(|a, b| {
            b.updated
                .cmp(&a.updated)
                .then_with(|| a.source.cmp(&b.source))
                .then_with(|| a.id.cmp(&b.id))
                .then_with(|| a.path.cmp(&b.path))
        });
        logs
    }

    pub(crate) fn listed_sessions(&self) -> Vec<&LogHead> {
        self.selected()
            .into_iter()
            .filter(|head| !head.gone && !head.archived && !head.live.is_internal)
            .collect()
    }

    /// Every session the logs hold, listed or not: archived, removed and
    /// internal ones too. A hook's completion for one of them is that
    /// session's, never a session of its own to present in its place.
    pub(crate) fn known_sessions(&self) -> HashSet<(String, String)> {
        self.logs
            .values()
            .map(|head| (head.source.clone(), head.id.clone()))
            .collect()
    }

    pub(crate) fn ensure_usage_projection(&mut self) {
        if self.usage_projection.is_some() {
            return;
        }
        self.statistics.revision = self.statistics.revision.wrapping_add(1);
        let logs = self.select();
        let mut totals = Usage::default();
        let mut buckets = BTreeMap::<i64, TimelineBucket>::new();
        let mut counts = HashMap::<String, usize>::new();
        for log in &logs {
            totals.add(log.usage);
            if !log.gone && !log.archived && !log.live.is_internal {
                *counts.entry(log.source.clone()).or_default() += 1;
            }
            for (at, value) in &log.buckets {
                buckets.entry(*at).or_default().add(*value);
            }
        }
        let timeline = buckets.into_iter().rev().take(48).collect::<Vec<_>>().into_iter().rev().map(|(at,value)|json!({"timestamp":DateTime::<Utc>::from_timestamp(at,0).unwrap().to_rfc3339(),"input":value.input,"output":value.output,"cache":value.cache,"tokens":value.input+value.output+value.cache})).collect();
        let wb_metering = &self.workbuddy_metering;
        let mut titles: HashMap<String, String> = self
            .logs
            .values()
            .filter(|log| log.source == "codex-index")
            .flat_map(|log| {
                log.titles
                    .iter()
                    .map(|(id, title)| (id.clone(), title.clone()))
            })
            .collect();
        titles.extend(wb_metering.titles.clone());
        let context_percents: HashMap<String, f64> = wb_metering
            .credits
            .iter()
            .filter(|&(_id, c)| c.size > 0)
            .map(|(id, c)| {
                (
                    id.clone(),
                    (c.used as f64 / c.size as f64 * 100.0).clamp(0.0, 100.0),
                )
            })
            .collect();
        self.usage_projection = Some(UsageProjection {
            selected: logs.iter().map(|log| log.path.clone()).collect(),
            totals,
            timeline,
            counts,
            titles,
            context_percents,
            observed_providers: self
                .logs
                .values()
                .filter(|log| log.observed && !log.live.is_internal)
                .map(|log| log.source.clone())
                .collect(),
        });
    }
}
