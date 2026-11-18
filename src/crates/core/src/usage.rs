//! Token counts as the ledger keeps them, and the usage projection a snapshot
//! is drawn from until a source write changes it.
use crate::{AgentHudCore, LogHead, TimelineBucket, Usage};
use agent_hud_agents::split;
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
    /// The selected logs that hold an earlier part of a session, each with
    /// the log the session lists as (`earlier_segments`): counted, never
    /// listed.
    pub(crate) earlier: HashMap<PathBuf, PathBuf>,
    pub(crate) totals: Usage,
    pub(crate) timeline: Vec<Value>,
    pub(crate) titles: HashMap<String, String>,
    pub(crate) context_percents: HashMap<String, f64>,
    pub(crate) observed_providers: HashSet<String>,
    /// The clients those sessions belong to (`agents::split`).
    pub(crate) observed_clients: HashSet<String>,
    pub(crate) counts: HashMap<String, usize>,
}

/// A WorkBuddy or CodeBuddy transcript its client made and never wrote to,
/// such as one of a `-p` run that failed at once. These clients name a
/// session only in its lines, so it holds none: kept, it was counted and
/// listed under its bare file name, with no `workbuddy:` id, folder or title.
fn unwritten_buddy_transcript(log: &LogHead) -> bool {
    matches!(log.source.as_str(), "workbuddy" | "codebuddy") && !log.observed && log.length == 0
}

/// The part of its session a log holds, "" for the whole of it: each
/// segment of a session Codex split across several rollouts counts, and
/// copies of one segment count once (`agent_hud_agents::transcript::segments`).
pub(crate) fn segment(log: &LogHead) -> &str {
    agent_hud_agents::transcript::segments::of(&log.source, &log.path)
}

/// The selected logs that hold an earlier segment of their session
/// (`segment`), each with the newest segment of it, which stands for the
/// session in every list. The newest segment's name sorts last.
pub(crate) fn earlier_segments<'a>(
    logs: &[&'a LogHead],
) -> HashMap<&'a std::path::Path, &'a std::path::Path> {
    let mut sessions = HashMap::<(&str, &str), Vec<&LogHead>>::new();
    for log in logs.iter().copied().filter(|log| !segment(log).is_empty()) {
        sessions
            .entry((log.source.as_str(), log.id.as_str()))
            .or_default()
            .push(log);
    }
    let mut earlier = HashMap::new();
    for segments in sessions.into_values().filter(|segments| segments.len() > 1) {
        let newest = segments
            .iter()
            .copied()
            .max_by_key(|log| segment(log))
            .unwrap();
        for log in segments.into_iter().filter(|log| log.path != newest.path) {
            earlier.insert(log.path.as_path(), newest.path.as_path());
        }
    }
    earlier
}

impl UsageProjection {
    /// Whether a session list shows the selected log: not gone, archived,
    /// internal (Codex's guardian reviews) or an earlier segment of a session.
    pub(crate) fn lists(&self, head: &LogHead) -> bool {
        !head.gone
            && !head.archived
            && !head.live.is_internal
            && !self.earlier.contains_key(&head.path)
    }
}

impl AgentHudCore {
    /// The logs whose usage counts: one copy per session, or per segment of
    /// a session Codex split across several rollouts (`segment`), internal
    /// ones (Codex's guardian reviews) included, as their usage counts in
    /// Open; `listed_sessions` leaves those and earlier segments out of
    /// every list.
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
        let mut sessions: HashMap<(&str, &str, &str), &LogHead> = HashMap::new();
        for log in self.logs.values().filter(|log| {
            log.source != "codex-index"
                && !unwritten_buddy_transcript(log)
                && self.log_on(&log.source, log.client.as_deref())
        }) {
            let is_mapped = mapped_copy(log);
            sessions
                .entry((log.source.as_str(), &log.id, segment(log)))
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
        if let Some(projection) = &self.usage_projection {
            return self
                .selected()
                .into_iter()
                .filter(|head| projection.lists(head))
                .collect();
        }
        let selected = self.select();
        let earlier = earlier_segments(&selected);
        selected
            .into_iter()
            .filter(|head| {
                !head.gone
                    && !head.archived
                    && !head.live.is_internal
                    && !earlier.contains_key(head.path.as_path())
            })
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
        let earlier = earlier_segments(&logs);
        let mut totals = Usage::default();
        let mut buckets = BTreeMap::<i64, TimelineBucket>::new();
        let mut counts = HashMap::<String, usize>::new();
        for log in &logs {
            totals.add(log.usage);
            if !log.gone
                && !log.archived
                && !log.live.is_internal
                && !earlier.contains_key(log.path.as_path())
            {
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
        let earlier = earlier
            .into_iter()
            .map(|(log, newest)| (log.to_path_buf(), newest.to_path_buf()))
            .collect();
        self.usage_projection = Some(UsageProjection {
            selected: logs.iter().map(|log| log.path.clone()).collect(),
            earlier,
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
            observed_clients: self
                .logs
                .values()
                .filter(|log| log.observed && !log.live.is_internal)
                .map(|log| split::of_log(&log.source, log.client.as_deref()).to_owned())
                .collect(),
        });
    }
}
