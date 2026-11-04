//! Lightweight paging over complete contribution heads. No source I/O or account
//! collection is performed here; details decode only the requested checkpoint.
use super::*;

use std::hash::{Hash, Hasher};

mod detail;
mod list;

/// A log as the session list shows it, before its head is written. The
/// session view needs core's projects and demo, so it lives in core.
pub(crate) fn log_session(log: &Log, now: i64, subagents_at: Option<i64>) -> Value {
    let updated = log
        .live
        .activity_at(metering::date_ms(&log.updated), subagents_at)
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .map(|at| at.to_rfc3339())
        .unwrap_or_else(|| log.updated.clone());
    let mut session = json!({"id":log.id,"title":log.title(),"source":log.source,"model":log.model,"vendorName":crate::naming::model_names::vendor_name(&log.source),"modelName":(!log.model.is_empty()).then(||crate::naming::model_names::consumer_name(&format!("{}-model:{}",log.source,log.model))),"cwd":log.cwd,"updatedAt":updated,"state":log.live.session_state(now,&log.source,subagents_at),"tokens":log.usage.total(),"cost":null,"contextPercent":null,"logPath":log.origin_path.as_ref().unwrap_or(&log.path),"parentId":log.live.parent_id});
    session["project"] = json!(crate::projects::project_id(log.cwd.as_deref()));
    if let Some(client) = crate::naming::clients::client_name(&log.source, log.client.as_deref()) {
        session["client"] = json!(client);
    }
    session["origins"] = crate::naming::surfaces::resolve(
        &log.source,
        &log.prompts,
        log.client.as_deref(),
        &Default::default(),
    )
    .json();
    // The same way back as its row in the list (`LogHead::session`):
    // `open_session` goes where this says.
    if let Some(target) = agent_hud_agents::session_target(
        &log.source,
        &log.id,
        log.cwd.as_deref(),
        log.client.as_deref(),
    ) {
        session["navigationTarget"] = target;
    }
    // A demo session has a way back, so its reply card shows "Return to
    // session" as a real one does; the demo runtime goes nowhere with it.
    if crate::demo::is_demo(&log.path) {
        session["navigationTarget"] = json!({"kind":"demo","id":log.id});
    }
    if let Some(facts) = log.live.facts(&log.id, &log.source, now, subagents_at) {
        session["liveFacts"] = facts;
    }
    session
}

impl AgentHudCore {
    pub(super) fn refresh_sessions_revision(&mut self, now: i64) {
        self.ensure_usage_projection();
        self.requests.expire(now);
        self.sync_claude_desktop_asks(now);
        // The snapshot, the session list's pages and the session details of a
        // pass share one fingerprint: it holds while the logs, the requests and
        // the hook facts (their deadlines included) stay as they were, for up
        // to a second.
        let inputs = (
            self.statistics.revision,
            self.requests.revision(),
            self.hook_events.clock_fingerprint(now),
        );
        if self
            .sessions_checked
            .is_some_and(|(seen, at)| seen == inputs && (at..at + 1_000).contains(&now))
        {
            return;
        }
        self.sessions_checked = Some((inputs, now));
        let presentation = self.presentation(now);
        // The projection holds this pass's selection; both lists below sort it.
        let selected = self
            .usage_projection
            .as_ref()
            .expect("usage projection prepared")
            .selected
            .iter()
            .filter_map(|path| self.logs.get(path))
            .collect::<Vec<_>>();
        // A sub-agent no list shows (an internal review, an archived or gone
        // log) still adds to its parent's tokens and cost.
        let mut unlisted = selected
            .iter()
            .filter(|log| {
                log.live.parent_id.is_some() && (log.gone || log.archived || log.live.is_internal)
            })
            .map(|log| (log.path.as_path(), log.usage.total(), log.list_cost))
            .collect::<Vec<_>>();
        unlisted.sort_unstable();
        let logs = selected
            .into_iter()
            .filter(|head| !head.gone && !head.archived && !head.live.is_internal)
            .collect::<Vec<_>>();
        let children = Self::active_subagents(&logs, now);
        let titles = &self.usage_projection.as_ref().unwrap().titles;
        // Each log's part is hashed with its path and the parts are added, so
        // the logs need no sorting first.
        let mut listed = 0u64;
        for log in &logs {
            let child = children
                .get(&(log.source.as_str(), log.id.as_str()))
                .copied();
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            log.path.hash(&mut hash);
            log.fact_hash.hash(&mut hash);
            presentation.activity_at(log, child).hash(&mut hash);
            log.live
                .session_state(now, &log.source, child)
                .hash(&mut hash);
            log.live.clock_fingerprint(now).hash(&mut hash);
            presentation.recent(log, 1, child).hash(&mut hash);
            presentation.recent(log, 7, child).hash(&mut hash);
            titles.get(&log.id).hash(&mut hash);
            listed = listed.wrapping_add(hash.finish());
        }
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.requests.revision().hash(&mut hash);
        self.hook_events.clock_fingerprint(now).hash(&mut hash);
        unlisted.hash(&mut hash);
        (logs.len(), listed).hash(&mut hash);
        let fingerprint = hash.finish();
        if self.sessions_fingerprint != Some(fingerprint) {
            self.sessions_fingerprint = Some(fingerprint);
            self.sessions_revision = self.sessions_revision.wrapping_add(1);
        }
    }

    pub(crate) fn active_subagents<'a>(
        logs: &[&'a LogHead],
        now: i64,
    ) -> HashMap<(&'a str, &'a str), i64> {
        let mut active = HashMap::<(&str, &str), i64>::new();
        for log in logs {
            let Some(parent) = log.live.parent_id.as_deref() else {
                continue;
            };
            if !log.live.is_internal
                && matches!(
                    log.live.session_state(now, &log.source, None),
                    "running" | "waiting"
                )
            {
                if let Some(at) = log.live.last_event() {
                    active
                        .entry((&log.source, parent))
                        .and_modify(|value| *value = (*value).max(at))
                        .or_insert(at);
                }
            }
        }
        active
    }
}
