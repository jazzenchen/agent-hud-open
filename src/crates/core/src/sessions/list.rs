//! The session list: a page of the sessions a scope, its filters and a search
//! select, with the day totals and projects they fall under.
use crate::*;

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

impl AgentHudCore {
    pub(crate) fn sessions(&mut self, params: Value) -> Result<Value, String> {
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
}
