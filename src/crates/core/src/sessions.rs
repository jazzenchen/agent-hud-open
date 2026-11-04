//! Lightweight paging over complete contribution heads. No source I/O or account
//! collection is performed here; details decode only the requested checkpoint.
use super::*;

fn session_project(target: &Value, cwd: Option<&str>) -> Option<String> {
    if target["kind"] == "claude-project-thread" {
        target["projectId"]
            .as_str()
            .map(|id| format!("claude-project:{id}"))
    } else {
        crate::projects::project_id(cwd)
    }
}

fn normalize_project_filter(project: &str) -> String {
    if project.starts_with("claude-project:") {
        project.to_owned()
    } else {
        crate::projects::normalize(project)
    }
}
use std::hash::{Hash, Hasher};

impl AgentHudCore {
    /// Puts the questions the Claude desktop app asks in its own window among
    /// the requests the user is asked about, in place of the ones asked
    /// before. A next-step box whose session the host reaches is answerable
    /// (a card of its own id, which replaces the display-only one, as a host
    /// that starts sending replaces Open's); any other has the reason it can
    /// only be answered in the app (agent-hud-native-open
    /// ClaudeDesktopAsks.show/requests).
    pub(super) fn sync_claude_desktop_asks(&mut self, now: i64) {
        let mut active = HashMap::new();
        let mut standing = HashSet::new();
        let claude = agent_hud_agents::claude::AGENT.id;
        if self.enabled.get(claude).copied().unwrap_or(true) {
            self.reword_desktop_asks(now);
            // Every pass prepares the projection first; its selection saves
            // choosing among each session's copies again.
            let candidates = self
                .listed_sessions()
                .into_iter()
                .filter(|log| log.source == claude)
                .filter_map(|log| {
                    let ask = log.live.activity.as_ref()?.desktop_ask.as_ref()?;
                    // A question whose words this run could not read has no card.
                    if !ask.is_worded() {
                        return None;
                    }
                    if now - ask.at >= 1_800_000
                        || !matches!(
                            log.live.session_state(now, &log.source, None),
                            "running" | "waiting"
                        )
                    {
                        return None;
                    }
                    let key = crate::grok_cache::hash(&[&log.id, &ask.id]);
                    Some((key, log.id.clone(), log.cwd.clone(), ask.clone()))
                })
                .collect::<Vec<_>>();
            for (key, session, cwd, ask) in candidates {
                standing.insert(key.clone());
                if self.claude_desktop.settled.contains(&key) {
                    continue;
                }
                // Once answerable, a box stays so while it stands: a session
                // the host stops reaching for a while (the app pauses a
                // hidden session) refuses the answer with the host's reason
                // instead of its card being replaced back and forth, here
                // and on the phone.
                // Turning answering off takes effect at once, reached or not.
                let way = if let Some(off) = self.claude_answers_off() {
                    Err(off)
                } else if self.claude_desktop.reached.contains(&key) {
                    Ok(())
                } else {
                    self.desktop_ask_way(&session, &ask)
                };
                if way.is_ok() {
                    self.claude_desktop.reached.insert(key.clone());
                }
                let req_id = format!(
                    "claude-desktop-ask:{}{key}",
                    if way.is_ok() { "reply:" } else { "" }
                );
                let view = agent_hud_agents::claude::transcript::desktop_ask_view(
                    &req_id,
                    &session,
                    cwd.as_deref(),
                    &ask,
                    way.err(),
                );
                match self
                    .requests
                    .register_native(req_id.clone(), view.clone(), now)
                {
                    Some(ticket) => {
                        active.insert(req_id, (key, ticket));
                    }
                    None => {
                        // Shown already, or put away by its wait running out.
                        self.requests.refresh_native(&req_id, view);
                        if let Some(kept) = self.claude_desktop.asks.remove(&req_id) {
                            active.insert(req_id, kept);
                        }
                    }
                }
            }
        }
        for old_id in self.claude_desktop.asks.keys() {
            if !active.contains_key(old_id) {
                self.requests.settle_native(old_id);
            }
        }
        self.claude_desktop.asks = active;
        self.claude_desktop
            .settled
            .retain(|key| standing.contains(key));
        self.claude_desktop
            .reached
            .retain(|key| standing.contains(key));
    }
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
    pub(super) fn sessions(&mut self, params: Value) -> Result<Value, String> {
        let scope = params
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("recent");
        if !matches!(scope, "recent" | "active" | "all") {
            return Err("scope must be recent, active or all".into());
        }
        let offset = params
            .get("offset")
            .map(|v| v.as_u64().ok_or("offset must be a nonnegative integer"))
            .transpose()?
            .unwrap_or(0);
        let offset = usize::try_from(offset).map_err(|_| "offset is too large")?;
        let limit = params
            .get("limit")
            .map(|v| {
                v.as_u64()
                    .filter(|n| *n > 0)
                    .ok_or("limit must be positive")
            })
            .transpose()?
            .unwrap_or(100)
            .min(200) as usize;
        let roots = params
            .get("rootsOnly")
            .map(|v| v.as_bool().ok_or("rootsOnly must be a boolean"))
            .transpose()?
            .unwrap_or(true);
        // The Sessions page's rows also show their turns and context
        // (agent-hud-native-open `SessionCard`); other lists leave them out.
        let turn_bars = params
            .get("turnBars")
            .map(|v| v.as_bool().ok_or("turnBars must be a boolean"))
            .transpose()?
            .unwrap_or(false);
        let parent = params
            .get("parentId")
            .map(|v| v.as_str().ok_or("parentId must be a string"))
            .transpose()?;
        let source = params
            .get("source")
            .map(|v| v.as_str().ok_or("source must be a string"))
            .transpose()?;
        let project = params.get("project");
        if project.is_some_and(|v| !v.is_null() && !v.is_string()) {
            return Err("project must be a string or null".into());
        }
        fn strings(params: &Value, key: &str) -> Result<Option<HashSet<String>>, String> {
            params
                .get(key)
                .map(|value| {
                    let values = value
                        .as_array()
                        .ok_or_else(|| format!("{key} must be an array of strings"))?;
                    if values.len() > 512 {
                        return Err(format!("{key} has too many values"));
                    }
                    values
                        .iter()
                        .map(|value| {
                            value
                                .as_str()
                                .map(str::to_owned)
                                .ok_or_else(|| format!("{key} must be an array of strings"))
                        })
                        .collect()
                })
                .transpose()
        }
        let source_filter = strings(&params, "sources")?;
        let project_filter = strings(&params, "projects")?;
        let search = params
            .get("search")
            .map(|v| v.as_str().ok_or("search must be a string"))
            .transpose()?
            .unwrap_or("")
            .trim()
            .to_lowercase();
        let zone = params
            .get("timeZone")
            .map(|v| {
                v.as_str()
                    .and_then(|name| name.parse::<chrono_tz::Tz>().ok())
                    .ok_or("timeZone must be an IANA time-zone name")
            })
            .transpose()?
            .unwrap_or(chrono_tz::Tz::UTC);
        let now = Utc::now().timestamp_millis();
        self.refresh_sessions_revision(now);
        let presentation = self.presentation(now);
        let logs = self.listed_sessions();
        let ids = logs
            .iter()
            .map(|l| (l.source.as_str(), l.id.as_str()))
            .collect::<HashSet<_>>();
        let children = Self::active_subagents(&logs, now);
        let scoped = logs
            .into_iter()
            // Internal Project executors remain available by id and for usage,
            // but their original conversation is the root list entry.
            .filter(|log| !roots || parent.is_some() || !self.is_claude_project_worker(log))
            .filter(|log| {
                let related = if let Some(parent) = parent {
                    log.live.parent_id.as_deref() == Some(parent)
                } else {
                    !roots
                        || log
                            .live
                            .parent_id
                            .as_deref()
                            .is_none_or(|id| !ids.contains(&(log.source.as_str(), id)))
                };
                // A list's window is time alone: a turn that looks in flight
                // keeps no session in it (owner, 2026-10-26).
                let days = if scope == "active" { 1 } else { 7 };
                related
                    && (scope == "all"
                        || presentation
                            .activity_at(
                                log,
                                children
                                    .get(&(log.source.as_str(), log.id.as_str()))
                                    .copied(),
                            )
                            .is_some_and(|at| at >= now - days * 86_400_000))
            })
            .collect::<Vec<_>>();
        let mut extras = if parent.is_none() && self.hook_events.revision() != 0 {
            let existing = self.known_sessions();
            self.hook_events
                .extra_sessions(
                    now,
                    if scope == "all" {
                        30
                    } else if scope == "active" {
                        1
                    } else {
                        7
                    },
                    &existing,
                )
                .into_iter()
                .map(|row| self.protect_claude_project_extra(row))
                .filter(|row| {
                    self.enabled
                        .get(row["source"].as_str().unwrap_or(""))
                        .copied()
                        .unwrap_or(true)
                })
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        if parent.is_none() && self.enabled.get("claude").copied().unwrap_or(true) {
            let max_age = if scope == "active" { 1 } else { 7 };
            extras.extend(self.claude_project_rows().into_iter().filter(|row| {
                scope == "all"
                    || row["lastMessageAt"]
                        .as_str()
                        .and_then(metering::date_ms)
                        .is_some_and(|at| at >= now - max_age * 86_400_000)
            }));
        }
        let mut sources = scoped.iter().map(|l| l.source.clone()).collect::<Vec<_>>();
        sources.extend(
            extras
                .iter()
                .filter_map(|row| row["source"].as_str().map(str::to_owned)),
        );
        sources.sort();
        sources.dedup();
        // Projects as Open names them: a repository shared by its worktrees,
        // written with ~, in Finder's order, the sessions of no folder last.
        let project_of = |cwd: Option<&str>| crate::projects::project_id(cwd);
        let mut projects = scoped
            .iter()
            .map(|l| project_of(l.cwd.as_deref()))
            .collect::<Vec<_>>();
        projects.extend(
            extras
                .iter()
                .map(|row| session_project(&row["navigationTarget"], row["cwd"].as_str())),
        );
        projects.sort_by(crate::projects::compare);
        projects.dedup();
        let project_labels = extras
            .iter()
            .filter_map(|row| {
                let project = session_project(&row["navigationTarget"], row["cwd"].as_str())?;
                let label = row["desktopProject"]["projectLabel"].as_str()?.trim();
                (!label.is_empty()).then(|| (project, label.to_owned()))
            })
            .collect::<BTreeMap<_, _>>();
        let wanted = project.map(|project| project.as_str().map(normalize_project_filter));
        let project_filter = project_filter.map(|projects| {
            projects
                .iter()
                .map(|entry| normalize_project_filter(entry))
                .collect::<Vec<_>>()
        });
        let in_project = |target: &Value, cwd: Option<&str>| {
            let id =
                session_project(target, cwd).unwrap_or_else(|| crate::projects::UNASSIGNED.into());
            project_filter
                .as_ref()
                .is_none_or(|projects| projects.contains(&id))
        };
        let filtered = scoped
            .into_iter()
            .filter(|log| {
                source.is_none_or(|source| log.source == source)
                    && source_filter
                        .as_ref()
                        .is_none_or(|sources| sources.contains(&log.source))
                    && wanted.as_ref().is_none_or(|wanted| project_of(log.cwd.as_deref()) == *wanted)
                    && in_project(&Value::Null, log.cwd.as_deref())
                    && (search.is_empty()
                        || [
                            Some(log.id.as_str()),
                            Some(log.source.as_str()),
                            agent_hud_agents::by_id(&log.source).map(|agent| agent.name),
                            // The client as the list names it: "Codex Desktop".
                            crate::naming::clients::client_name(&log.source, log.client.as_deref()).as_deref(),
                            Some(log.title.as_str()),
                            Some(log.model.as_str()),
                            log.cwd.as_deref(),
                            self.usage_projection
                                .as_ref()
                                .and_then(|p| p.titles.get(&log.id))
                                .map(String::as_str),
                        ]
                        .into_iter()
                        .flatten()
                        .any(|field| field.to_lowercase().contains(&search))
                        // The model as people read it: "Opus 4.5".
                        || (!log.model.is_empty()
                            && crate::naming::model_names::consumer_name(&format!("{}-model:{}", log.source, log.model))
                                .to_lowercase()
                                .contains(&search)))
            })
            .collect::<Vec<_>>();
        // Keep the complete history as cheap heads; serialize only this page.
        enum Row<'a> {
            Log(&'a LogHead),
            Extra(Value),
        }
        let mut rows = filtered
            .into_iter()
            .map(Row::Log)
            .chain(
                extras
                    .into_iter()
                    .filter(|row| {
                        source.is_none_or(|source| row["source"] == source)
                            && source_filter.as_ref().is_none_or(|sources| {
                                sources.contains(row["source"].as_str().unwrap_or(""))
                            })
                            && wanted.as_ref().is_none_or(|wanted| {
                                session_project(&row["navigationTarget"], row["cwd"].as_str())
                                    == *wanted
                            })
                            && in_project(&row["navigationTarget"], row["cwd"].as_str())
                            && (search.is_empty()
                                || ["id", "source", "title", "model", "cwd", "project"]
                                    .iter()
                                    .any(|key| {
                                        row[*key].as_str().is_some_and(|field| {
                                            field.to_lowercase().contains(&search)
                                        })
                                    }))
                    })
                    .map(Row::Extra),
            )
            .collect::<Vec<_>>();
        // Lists order sessions by their last message, never by whether a turn
        // looks in flight (owner, 2026-10-26).
        let message_at = |row: &Row<'_>| match row {
            Row::Log(log) => log.last_message_at(),
            Row::Extra(value) => value["lastMessageAt"]
                .as_str()
                .or(value["updatedAt"].as_str())
                .and_then(metering::date_ms),
        };
        fn identity<'a>(row: &'a Row<'_>) -> (&'a str, &'a str, &'a str) {
            match row {
                Row::Log(log) => (&log.source, &log.id, log.path.to_str().unwrap_or("")),
                Row::Extra(value) => (
                    value["source"].as_str().unwrap_or(""),
                    value["id"].as_str().unwrap_or(""),
                    "",
                ),
            }
        }
        let state = |row: &Row<'_>| -> &'static str {
            match row {
                Row::Log(log) => presentation.session_state(
                    log,
                    children
                        .get(&(log.source.as_str(), log.id.as_str()))
                        .copied(),
                ),
                Row::Extra(value) => match value["state"].as_str() {
                    Some("waiting") => "waiting",
                    Some("running") => "running",
                    Some("idle") => "idle",
                    _ => "unverified",
                },
            }
        };
        // Order the complete persisted heads before paging. Rendering a small
        // HUD page must not silently cap its scope at the first 200 rows.
        let mut rows = rows
            .drain(..)
            .map(|row| {
                let state = state(&row);
                (row, state)
            })
            .collect::<Vec<_>>();
        rows.sort_unstable_by(|(a, _), (b, _)| {
            message_at(b)
                .cmp(&message_at(a))
                .then_with(|| identity(a).cmp(&identity(b)))
        });
        let total = rows.len();
        let claude_shares = self.claude_window_shares(now);
        let selected = self.selected();
        let family = session_breakdown::Family::new(&selected);
        let currency = self.deepseek_account_currency();
        // What every matching session spent, not only this page's, by the
        // local day of its last message (a later one under today): the
        // Sessions page's day headers and Today card. The six days before
        // today are named, older ones are Earlier. The rows are in order, so
        // each day covers the next of them. A day's money reads as a row's:
        // the whole cost while every session has one, else each session's
        // whole or priced cost, with the unpriced tokens of those with none.
        #[derive(Default)]
        struct Day {
            date: Option<chrono::NaiveDate>,
            sessions: usize,
            tokens: u64,
            usage: Option<Usage>,
            /// The sum of whole costs while every session has one.
            whole: Option<f64>,
            priced: Option<f64>,
            unpriced: u64,
            /// The most tokens first, each with its row's position.
            top: Vec<(u64, usize)>,
            rest: (usize, u64),
        }
        let today = statistics::local_date(zone, now);
        let named = today - chrono::Days::new(6);
        let mut days = Vec::<Day>::new();
        for (index, (row, _)) in rows.iter().enumerate() {
            let date = message_at(row)
                .map(|at| statistics::local_date(zone, at.min(now)))
                .filter(|date| *date >= named);
            if days.last().is_none_or(|day| day.date != date) {
                days.push(Day {
                    date,
                    whole: Some(0.0),
                    ..Day::default()
                });
            }
            let day = days.last_mut().unwrap();
            let (tokens, usage, cost, (priced, unpriced)) = match row {
                Row::Log(log) => {
                    let (usage, _) = family.usage(log);
                    let cost = family.list_cost(log);
                    (usage.total(), Some(usage), cost, family.priced_cost(log))
                }
                Row::Extra(value) => {
                    let kind = |key| value["usage"][key].as_u64().unwrap_or(0);
                    (
                        value["tokens"].as_u64().unwrap_or(0),
                        value["usage"].is_object().then(|| Usage {
                            input: kind("input"),
                            output: kind("output"),
                            read: kind("cacheRead"),
                            write: kind("cacheWrite"),
                            reasoning: kind("reasoning"),
                        }),
                        value["cost"].as_f64(),
                        (
                            value["pricedCost"].as_f64(),
                            value["unpricedTokens"].as_u64().unwrap_or(0),
                        ),
                    )
                }
            };
            day.sessions += 1;
            day.tokens += tokens;
            if let Some(usage) = usage {
                day.usage.get_or_insert_default().add(usage);
            }
            day.whole = day.whole.zip(cost).map(|(sum, cost)| sum + cost);
            if let Some(part) = cost.or(priced) {
                *day.priced.get_or_insert(0.0) += part;
            }
            if cost.is_none() {
                day.unpriced += unpriced;
            }
            if tokens > 0 {
                let at = day.top.iter().position(|(most, _)| *most < tokens);
                day.top.insert(at.unwrap_or(day.top.len()), (tokens, index));
                if day.top.len() > 5 {
                    let (tokens, _) = day.top.pop().unwrap();
                    day.rest = (day.rest.0 + 1, day.rest.1 + tokens);
                }
            }
        }
        let days = days
            .into_iter()
            .map(|day| {
                let top = day
                    .top
                    .iter()
                    .map(|(tokens, index)| {
                        let (id, title) = match &rows[*index].0 {
                            Row::Log(log) => (
                                log.id.as_str(),
                                self.usage_projection
                                    .as_ref()
                                    .and_then(|p| p.titles.get(&log.id))
                                    .unwrap_or(&log.title)
                                    .as_str(),
                            ),
                            Row::Extra(value) => (
                                value["id"].as_str().unwrap_or(""),
                                value["title"].as_str().unwrap_or(""),
                            ),
                        };
                        json!({"id": id, "title": title, "tokens": tokens})
                    })
                    .collect::<Vec<_>>();
                let start = day.date.map(|date| statistics::day_start(zone, date));
                Ok(json!({
                    "start": start.transpose()?.map(iso),
                    "sessions": day.sessions,
                    "tokens": day.tokens,
                    "usage": day.usage.map(Usage::json),
                    "cost": day.whole,
                    "pricedCost": day.priced,
                    "unpricedTokens": day.unpriced,
                    "top": top,
                    "rest": {"count": day.rest.0, "tokens": day.rest.1},
                }))
            })
            .collect::<Result<Vec<_>, String>>()?;
        if turn_bars {
            // Forgets the sessions no longer listed, so the map follows the logs.
            let listed = selected
                .iter()
                .map(|log| log.path.as_path())
                .collect::<HashSet<_>>();
            self.session_rows
                .borrow_mut()
                .retain(|path, _| listed.contains(path.as_path()));
        }
        let items = rows
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|(row, state)| match row {
                Row::Extra(mut value) => {
                    value["state"] = json!(state);
                    value
                }
                Row::Log(log) => {
                    let mut value = presentation.session(
                        log,
                        children
                            .get(&(log.source.as_str(), log.id.as_str()))
                            .copied(),
                    );
                    value["state"] = json!(state);
                    Self::session_totals(&mut value, log, &family, currency.as_deref());
                    if turn_bars {
                        if let Value::Object(fields) = self.session_row_usage(log, &family) {
                            for (key, field) in fields {
                                value[key] = field;
                            }
                        }
                    }
                    if let Some(title) = self
                        .usage_projection
                        .as_ref()
                        .and_then(|p| p.titles.get(&log.id))
                    {
                        value["title"] = json!(title);
                    }
                    if let Some(pct) = claude_shares.get(&log.path) {
                        value["pctOfWindow"] = json!(*pct);
                    }
                    value
                }
            })
            .collect::<Vec<_>>();
        let next = offset.saturating_add(items.len());
        Ok(
            json!({"kind":"sessions","revision":self.sessions_revision,"generatedAt":Utc::now().to_rfc3339(),"items":items,"total":total,"nextOffset":(next<total).then_some(next),"sources":sources,"projects":projects,"projectLabels":project_labels,"days":days}),
        )
    }
    pub(super) fn session_detail(&mut self, params: Value) -> Result<Value, String> {
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

impl crate::AgentHudCore {
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

    pub fn transcript_path(&self, id: &str) -> Result<PathBuf, String> {
        let log = self
            .selected()
            .into_iter()
            .find(|log| log.id == id)
            .ok_or_else(|| {
                l10n::text(
                    "找不到这个会话，请先刷新",
                    "Session was not found; refresh first",
                )
            })?;
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
            return Err(l10n::text(
                "这个会话日志已不在已启用客户端的目录中",
                "Transcript no longer belongs to an enabled local provider directory",
            ));
        }
        Ok(path)
    }
}
