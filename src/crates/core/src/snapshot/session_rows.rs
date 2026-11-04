//! The session list a snapshot shows: the listed rows, rebuilt only when a
//! session input changed, with the window shares and hook-only sessions.
use super::*;

impl crate::AgentHudCore {
    /// The recent sessions, most recently active first, with their titles,
    /// context, window shares and totals, and the sessions only client events
    /// know about.
    /// The session list. Its rows are built again only when a session input
    /// changed: the session fingerprint, the logs read, the indexes sessions
    /// are presented from, the hook facts, the requests, or the currency
    /// costs are shown in. The Claude window shares are laid over them and
    /// the sessions only a hook knows are added.
    pub(super) fn session_rows(&mut self, presented_at: i64, now: i64, at: i64) -> Vec<Value> {
        use std::hash::{Hash, Hasher};
        let currency = self.deepseek_account_currency();
        let key = {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            (
                self.sessions_revision,
                self.statistics.revision,
                self.projection_sources,
                self.hook_events.revision(),
                self.requests.revision(),
                &currency,
            )
                .hash(&mut hash);
            hash.finish()
        };
        if self
            .listed_rows
            .as_ref()
            .is_none_or(|(seen, _)| *seen != key)
        {
            let rows = self.listed_session_rows(presented_at, now, currency.as_deref());
            self.listed_rows = Some((key, rows));
        }
        let shares_key = {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            (self.statistics.revision, self.render.accounts, at).hash(&mut hash);
            hash.finish()
        };
        if self
            .window_shares
            .as_ref()
            .is_none_or(|(seen, _)| *seen != shares_key)
        {
            self.window_shares = Some((shares_key, self.claude_window_shares(now)));
        }
        let (_, listed) = self.listed_rows.as_ref().expect("rows built above");
        let (_, shares) = self
            .window_shares
            .as_ref()
            .expect("shares worked out above");
        let mut sessions = listed
            .rows
            .iter()
            .map(|(path, row)| {
                let mut row = row.clone();
                if let Some(pct) = shares.get(path) {
                    row["pctOfWindow"] = json!(*pct);
                }
                row
            })
            .collect::<Vec<_>>();
        if self.hook_events.revision() != 0 {
            let extras = self
                .hook_events
                .extra_sessions(now, 1, &self.known_sessions());
            sessions.extend(
                extras
                    .into_iter()
                    .filter(|value| {
                        self.enabled
                            .get(value["source"].as_str().unwrap_or(""))
                            .copied()
                            .unwrap_or(true)
                    })
                    .map(|row| self.protect_claude_project_extra(row)),
            );
        }
        if self.enabled.get("claude").copied().unwrap_or(true) {
            sessions.extend(self.claude_project_rows().into_iter().filter(|row| {
                row["lastMessageAt"]
                    .as_str()
                    .and_then(metering::date_ms)
                    .is_some_and(|timestamp| timestamp >= presented_at - 86_400_000)
            }));
        }
        presentation::sort_sessions(&mut sessions);
        sessions
    }

    /// The rows of the sessions listed in the last day, built from every
    /// selected log, and which sessions the logs list at all.
    fn listed_session_rows(
        &self,
        presented_at: i64,
        now: i64,
        currency: Option<&str>,
    ) -> ListedRows {
        let presentation = self.presentation(presented_at);
        let projection = self
            .usage_projection
            .as_ref()
            .expect("usage projection prepared");
        // One lookup per selected log; `sort_sessions` orders the few listed
        // sessions later, so every log is not sorted first.
        let every = projection
            .selected
            .iter()
            .filter_map(|path| self.logs.get(path))
            .collect::<Vec<_>>();
        let logs = every
            .iter()
            .copied()
            .filter(|head| !head.gone && !head.archived && !head.live.is_internal)
            .collect::<Vec<_>>();
        let children = Self::active_subagents(&logs, now);
        let family = session_breakdown::Family::new(&every);
        let rows = logs
            .iter()
            .filter(|log| !self.is_claude_project_worker(log))
            .filter(|log| {
                presentation.recent(
                    log,
                    1,
                    children
                        .get(&(log.source.as_str(), log.id.as_str()))
                        .copied(),
                )
            })
            .map(|log| {
                let mut session = presentation.session(
                    log,
                    children
                        .get(&(log.source.as_str(), log.id.as_str()))
                        .copied(),
                );
                if let Some(title) = projection.titles.get(&log.id) {
                    session["title"] = json!(title);
                }
                if let Some(pct) = projection.context_percents.get(&log.id) {
                    session["contextPercent"] = json!(*pct);
                }
                Self::session_totals(&mut session, log, &family, currency);
                (log.path.clone(), session)
            })
            .collect();
        ListedRows { rows }
    }
}
