//! One session in full: its head, totals, turns and calls, and where its
//! transcript is.
use super::log_session;
use crate::l10n::tr;
use crate::*;

impl AgentHudCore {
    fn titled_session(
        &self,
        log: &LogHead,
        now: i64,
        child: Option<i64>,
        family: &session_breakdown::Family,
        currency: Option<&str>,
    ) -> Value {
        let mut value = self.presentation(now).session(log, child);
        Self::session_totals(&mut value, log, family, currency);
        if let Some(proj) = self.usage_projection.as_ref() {
            if let Some(title) = proj.titles.get(&log.id) {
                value["title"] = json!(title);
            }
            if let Some(pct) = proj.context_percents.get(&log.id) {
                value["contextPercent"] = json!(*pct);
            }
        }
        value
    }

    pub(crate) fn session_detail(&mut self, params: Value) -> Result<Value, String> {
        let id = params["id"].as_str().ok_or("Session ID is required")?;
        let now = Utc::now().timestamp_millis();
        self.refresh_sessions_revision(now);
        let logs = self.listed_sessions();
        let selected = logs.iter().copied().find(|log| log.id == id);
        let Some(selected) = selected else {
            if let Some(detail) = self.claude_project_detail(id, now) {
                return Ok(detail);
            }
            let existing = self.known_sessions();
            let session = self
                .hook_events
                .extra_sessions(now, 30, &existing)
                .into_iter()
                .find(|session| {
                    session["id"] == id
                        && self
                            .enabled
                            .get(session["source"].as_str().unwrap_or(""))
                            .copied()
                            .unwrap_or(true)
                })
                .ok_or("Session was not found; refresh first")?;
            let session = self.protect_claude_project_extra(session);
            return Ok(
                json!({"session":session,"usage":Usage::default().json(),"turns":[],"calls":[],"parent":null,"children":[]}),
            );
        };
        let child_activity = Self::active_subagents(&logs, now);
        let source = selected.source.clone();
        let path = selected.path.clone();
        let every = self.selected();
        let family = session_breakdown::Family::new(&every);
        let currency = self.deepseek_account_currency();
        let subagents = family.descendants(&source, id);
        let breakdown = match &self.database {
            Some(db) => Self::session_breakdown(db, selected, &subagents, params["turn"].as_str())?,
            None => None,
        };
        let mut totals = json!({});
        Self::session_totals(&mut totals, selected, &family, currency.as_deref());
        // The sub-agents' part of the session's tokens (agent-hud-native-open's
        // "含子 agent" note), from the same logs as the total.
        let (_, subagent_usage) = family.usage(selected);
        // The session's own log apart from its sub-agents', which `usage` adds.
        let own = subagent_usage.map(|_| crate::session_usage::tokens_json(selected.usage));
        let credits = self.session_credits(id);
        let parent = selected
            .live
            .parent_id
            .as_deref()
            .and_then(|id| {
                logs.iter()
                    .copied()
                    .find(|log| log.id == id && log.source == source)
            })
            .map(|log| {
                self.titled_session(
                    log,
                    now,
                    child_activity
                        .get(&(log.source.as_str(), log.id.as_str()))
                        .copied(),
                    &family,
                    currency.as_deref(),
                )
            });
        let mut children = logs
            .iter()
            .copied()
            .filter(|log| log.source == source && log.live.parent_id.as_deref() == Some(id))
            .map(|log| {
                self.titled_session(
                    log,
                    now,
                    child_activity
                        .get(&(log.source.as_str(), log.id.as_str()))
                        .copied(),
                    &family,
                    currency.as_deref(),
                )
            })
            .collect::<Vec<_>>();
        children.sort_unstable_by(|a, b| {
            b["updatedAt"]
                .as_str()
                .cmp(&a["updatedAt"].as_str())
                .then_with(|| a["source"].as_str().cmp(&b["source"].as_str()))
                .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
        });
        let child = child_activity.get(&(source.as_str(), id)).copied();
        let title = self
            .usage_projection
            .as_ref()
            .and_then(|p| p.titles.get(id))
            .cloned();
        let (log, bytes) = self.load_parser(&path)?;
        let mut session = log_session(&log, now, child);
        self.presentation(now).overlay(
            &mut session,
            metering::date_ms(&log.updated),
            log.client.as_deref(),
            &log.prompts,
        );
        if self.claude_projects_supported {
            crate::clients::claude_project::overlay_worker(
                &self.claude_desktop.index,
                &mut session,
                log.live.claude_project_origin.as_ref(),
            );
        }
        if let Some(title) = title {
            session["title"] = json!(title);
        }
        for (key, value) in totals.as_object().unwrap() {
            session[key] = value.clone();
        }
        session["startedAt"] = log
            .live
            .started_at()
            .and_then(DateTime::<Utc>::from_timestamp_millis)
            .map(|at| json!(at.to_rfc3339()))
            .unwrap_or(Value::Null);
        let cursor = params
            .get("messageCursor")
            .map(|v| {
                v.as_u64()
                    .ok_or("messageCursor must be a nonnegative byte offset")
            })
            .transpose()?
            .unwrap_or(0);
        let limit = params
            .get("messageLimit")
            .map(|v| {
                v.as_u64()
                    .filter(|n| *n > 0)
                    .ok_or("messageLimit must be positive")
            })
            .transpose()?
            .unwrap_or(100)
            .min(200) as usize;
        let page = conversation::page(
            log.origin_path.as_deref().unwrap_or(&path),
            &source,
            &log.id,
            cursor,
            limit,
            params["messageTail"] == true,
            &mut self.conversations,
            &mut self.conversation_context,
        );
        let usage = session["usage"].clone();
        let mut result = json!({"session":session,"usage":usage,"own":own,"subagents":subagent_usage.map(crate::session_usage::tokens_json),"turns":log.turns,"calls":[],"parent":parent,"children":children,"credits":credits});
        // The ledger's breakdown replaces the parser's recent usage entries.
        if let Some(breakdown) = breakdown {
            for (key, value) in breakdown.as_object().unwrap() {
                result[key] = value.clone();
            }
        }
        match page {
            Ok(page) => {
                for (key, value) in page.as_object().unwrap() {
                    result[key] = value.clone();
                }
            }
            Err(error) => {
                result["messages"] = json!([]);
                result["messageNotice"] = json!(error);
            }
        }
        if let Some(database) = &self.database {
            let offset = params
                .get("callOffset")
                .map(|v| v.as_u64().ok_or("callOffset must be nonnegative"))
                .transpose()?
                .unwrap_or(0)
                .min(i64::MAX as u64);
            let limit = params
                .get("callLimit")
                .map(|v| {
                    v.as_u64()
                        .filter(|n| *n > 0)
                        .ok_or("callLimit must be positive")
                })
                .transpose()?
                .unwrap_or(200)
                .min(500);
            let (total, calls) = metering::session_calls(
                database,
                path.to_string_lossy().as_ref(),
                &source,
                offset,
                limit,
            )?;
            let next = offset + calls.len() as u64;
            result["calls"] = json!(calls);
            result["callsTotal"] = json!(total);
            result["nextCallOffset"] = json!((next < total).then_some(next));
        }
        self.parser_cache.insert(log, bytes);
        Ok(result)
    }
}

impl crate::AgentHudCore {
    pub fn transcript_path(&self, id: &str) -> Result<PathBuf, String> {
        let log = self
            .selected()
            .into_iter()
            .find(|log| log.id == id)
            .ok_or_else(|| tr!("Session was not found; refresh first"))?;
        let path = log
            .origin_path
            .as_ref()
            .unwrap_or(&log.path)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let allowed = self
            .provider_roots
            .iter()
            .filter_map(|(root, _)| root.canonicalize().ok())
            .any(|root| path.starts_with(root));
        if !allowed {
            return Err(tr!(
                "Transcript no longer belongs to an enabled local provider directory"
            ));
        }
        Ok(path)
    }
}
