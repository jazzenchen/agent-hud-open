//! The snapshot every window, the menu bar and MCP draw from: one collection
//! pass, then sessions, quotas, balances, accounts and providers as of the
//! metrics clock.
use super::*;

/// A pass that changed nothing the snapshot shows, before any time it shows
/// falls due, renders nothing: the collector then sends pages the time alone.
/// Until every clock-driven change has a deadline, one is due this often.
pub(crate) const SAFETY_REFRESH_MS: i64 = 30_000;

/// The wall clock a snapshot is rendered at, in milliseconds. Tests pin it,
/// so two renders compare at moments they choose rather than whenever the
/// test thread happens to run.
pub(crate) fn wall_clock_ms() -> i64 {
    #[cfg(test)]
    if let Some(pinned) = PINNED_CLOCK.get() {
        return pinned;
    }
    Utc::now().timestamp_millis()
}

#[cfg(test)]
thread_local! {
    pub(crate) static PINNED_CLOCK: std::cell::Cell<Option<i64>> =
        const { std::cell::Cell::new(None) };
}

#[derive(Default)]
pub(crate) struct Rendered {
    /// Bumped when the account readings change.
    pub(crate) accounts: u64,
    /// What the last snapshot was rendered from, and when it falls due.
    last: Option<(u64, i64)>,
}

/// The session rows a render built, kept for the next one.
pub(crate) struct ListedRows {
    /// Each listed session's row with the log it came from.
    rows: Vec<(PathBuf, Value)>,
}

impl crate::AgentHudCore {
    pub(crate) fn snapshot(&mut self) -> Result<Value, String> {
        self.collect();
        self.render_snapshot()
    }

    /// The reading half of a collection pass: new log lines, the indexes the
    /// session list joins, and the notice the snapshot carries.
    pub(crate) fn collect(&mut self) {
        let previous_pending = self.reads.pending.len();
        let notice = self.scan().err();
        if previous_pending != self.reads.pending.len() {
            self.usage_projection = None;
        }
        self.finish_indexing_run();
        let remaining = self.waiting_reads();
        // The conversation store's missing spans wait until the logs are
        // indexed, so they never slow the first reads.
        if remaining == 0 {
            self.catch_up_conversations();
        }
        // Claude Desktop's archive and tabs are read every pass, as Open does,
        // so archiving a session there needs no log change to show.
        if self.claude_desktop.index.refresh(&self.provider_roots) {
            self.usage_projection = None;
        }
        if self.usage_projection.is_none() {
            self.read_projection_sources();
        }
        self.source_notice = notice.or_else(|| {
            (remaining > 0).then(|| {
                if l10n::chinese() {
                    format!("正在索引 {remaining} 个会话日志，索引完成前 Token 合计不完整")
                } else {
                    format!("Indexing {remaining} transcript files. Token totals are partial until indexing finishes.")
                }
            })
        });
    }

    /// The snapshot, or with `changesOnly` and nothing changed since the last
    /// one before its deadline, `{"unchanged": true}`.
    pub(crate) fn render_if_changed(&mut self, params: &Value) -> Result<Value, String> {
        let now = wall_clock_ms();
        if params["changesOnly"] == true
            && self
                .render
                .last
                .is_some_and(|(key, due)| key == self.render_key() && now < due)
        {
            return Ok(json!({"unchanged": true}));
        }
        self.render_snapshot()
    }

    /// When the next snapshot is due though nothing changes: the earliest
    /// clock-driven change the last one showed, or the safety refresh.
    pub fn next_due(&self) -> Option<i64> {
        self.render.last.map(|(_, due)| due)
    }

    /// What a snapshot is rendered from, apart from the clock: the logs and
    /// the ledger, the account readings, hook facts, requests, settings, and
    /// what the collection pass noticed.
    fn render_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.statistics.revision.hash(&mut hash);
        self.render.accounts.hash(&mut hash);
        self.hook_events.revision().hash(&mut hash);
        self.requests.revision().hash(&mut hash);
        self.settings.to_string().hash(&mut hash);
        self.source_notice.hash(&mut hash);
        self.storage_issue.hash(&mut hash);
        self.reads.pending.len().hash(&mut hash);
        (
            self.usage_projection.is_none(),
            self.meters.view.is_none(),
            self.meters.changes.len(),
            self.meters.samples.is_none(),
        )
            .hash(&mut hash);
        hash.finish()
    }

    /// The earliest time after `now` at which a clock-driven part of the
    /// snapshot changes: a session going quiet or idle, a request or a hook
    /// fact lapsing, the meter view's hour or week moving on.
    fn render_due(&self, now: i64) -> i64 {
        let projection = self
            .usage_projection
            .as_ref()
            .expect("usage projection prepared");
        let logs = projection
            .selected
            .iter()
            .filter_map(|path| self.logs.get(path))
            .filter(|head| !head.gone && !head.live.is_internal)
            .collect::<Vec<_>>();
        let children = Self::active_subagents(&logs, now);
        // Every state change a log can make without a new event falls within
        // half an hour of its last event or a sub-agent's.
        let live = logs
            .iter()
            .filter_map(|log| {
                let child = children
                    .get(&(log.source.as_str(), log.id.as_str()))
                    .copied();
                let recent = log.live.last_event().into_iter().chain(child).max()?;
                (now - recent < 1_800_000)
                    .then(|| log.live.next_change(now, &log.source, child))
                    .flatten()
            })
            .min();
        [
            Some(now + SAFETY_REFRESH_MS),
            live,
            self.hook_events.next_change(now),
            self.requests.next_deadline(),
            self.meters.view.as_ref().and_then(|view| view.valid_until),
        ]
        .into_iter()
        .flatten()
        .filter(|at| *at > now)
        .min()
        .unwrap_or(now + SAFETY_REFRESH_MS)
    }

    /// An account as its section header shows it (Open's AccountSectionHeader
    /// and ReportView.accountNotice): its name, its plan in the provider's tier
    /// names, whether it is the one signed in, and what its client said. A read
    /// that failed is the warning, in amber; otherwise the client's notice is
    /// the line under the name. Every account a reader reports is signed in
    /// now: the app keeps no former accounts, so `current` is always true.
    fn account_heading(&self, value: &Value) -> Value {
        let mut account = value.clone();
        let provider = account["provider"].as_str().unwrap_or_default().to_owned();
        let id = account["id"].as_str().unwrap_or_default();
        let name = account["label"]
            .as_str()
            .filter(|label| !label.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                let tail = id.rsplit(':').next().unwrap_or(id);
                l10n::text("账户 ", "Account ") + &tail.chars().take(6).collect::<String>()
            });
        account["name"] = json!(name);
        account["planLabel"] = json!(account["plan"]
            .as_str()
            .and_then(|plan| naming::plans::plan_label(&provider, plan)));
        account["current"] = json!(true);
        let notice = self
            .accounts
            .providers
            .get(&provider)
            .and_then(|state| state["notice"].as_str());
        let failed = account["stale"] == true;
        account["warning"] = json!(notice.filter(|_| failed));
        account["notice"] = json!(notice.filter(|_| !failed));
        account
    }

    pub(crate) fn render_snapshot(&mut self) -> Result<Value, String> {
        crate::l10n::follow(&self.settings["language"]);
        let notice = self.source_notice.clone();
        self.ensure_usage_projection();
        let now = self.prepare_meters()?;
        self.requests.expire(now);
        self.hook_events.ingest(&[], now);
        let deliveries = self.requests.deliveries(now);
        self.refresh_sessions_revision(now);
        let at = self.metrics_clock(now);
        // Sessions are presented as of the pass; what follows is as of now.
        let presented_at = now;
        let now = wall_clock_ms();
        let sessions = self.session_rows(presented_at, now, at);
        let approvals = self.requests.views(now);
        let providers = self.provider_rows(at);
        let pending_meter = self.pending_meter_bounds();
        // Pausing a failed read stops the indexing timer, but does not make
        // its unread history complete. Consumers use this fact, not notices.
        let (bytes_done, bytes_total) =
            data_version::progress(&self.reads.listing.files, &self.logs, now);
        let indexing = json!({
            "complete": self.database.is_some()
                && self.storage_issue.is_none()
                && self.reads.pending.is_empty(),
            "pending": self.reads.pending.len(),
            "kind": self.indexing_run.map(data_version::Run::name),
            "dataVersion": crate::DATA_VERSION,
            "bytesDone": bytes_done,
            "bytesTotal": bytes_total,
        });
        let quotas = self.quota_rows(at, now, &pending_meter);
        let relay_users = self.relay_users(at);
        let billing = self.billing_rows(at, &pending_meter, &relay_users);
        let accounts = self.account_rows();
        let relays = self.relay_rows(&relay_users);
        let island_events = self
            .island_events
            .update(&sessions, &quotas, &accounts, now);
        self.render.last = Some((self.render_key(), self.render_due(now)));
        let projection = self
            .usage_projection
            .as_ref()
            .expect("usage projection prepared");
        Ok(
            json!({"islandEvents":island_events,"schemaVersion":1,"statisticsRevision":self.statistics.revision,"sessionsRevision":self.sessions_revision,"sessionScope":"recent","indexing":indexing,"generatedAt":Utc::now().to_rfc3339(),"mode":if self.demo{"demo"}else{"live"},"connection":{"status":"connected","platform":self.platform,"message":notice.unwrap_or_else(|| l10n::text("本机会话日志与客户端事件；回调连接状态另行显示", "Local transcripts and client events; hook connection status is reported separately."))},"totals":projection.totals.json(),"quotas":quotas,"accounts":accounts,"sessions":sessions,"timeline":projection.timeline,"providers":providers,"approvals":approvals,"requestDeliveries":deliveries,"settings":self.settings,"language":if crate::l10n::chinese(){"zh"}else{"en"},"capabilities":serde_json::from_str::<Value>(include_str!("../../../packages/protocol/capabilities.json")).expect("checked-in capability manifest"),"billing":billing,"relays":relays}),
        )
    }

    /// Brings the meter view, the quota samples and the balance metrics up to
    /// date with the ledger, and returns the time the snapshot is read at.
    pub(crate) fn prepare_meters(&mut self) -> Result<i64, String> {
        if !self.accounts.billing.is_empty() {
            if let Some(db) = &self.database {
                // Billed events are DeepSeek's (metering writes each event with
                // its log's source), so only DeepSeek logs are looked up.
                let deepseek = self
                    .usage_projection
                    .as_ref()
                    .unwrap()
                    .selected
                    .iter()
                    .filter(|path| {
                        self.logs
                            .get(*path)
                            .is_some_and(|log| log.source == "deepseek")
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                self.meters.billing.prepare(
                    db,
                    self.statistics.revision,
                    &deepseek,
                    wall_clock_ms(),
                )?;
            }
        }
        let now = wall_clock_ms();
        self.statistics.observed_at = Some(now);
        if let Some(database) = &self.database {
            if self.meters.view.is_none() || !self.meters.changes.is_empty() {
                let paths = self
                    .usage_projection
                    .as_ref()
                    .unwrap()
                    .selected
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                let result = if let Some(view) = &mut self.meters.view {
                    view.update(
                        database,
                        &paths,
                        &self.meters.changes,
                        now,
                        &self.price_regions,
                    )
                } else {
                    metering::View::load(database, &paths, now, &self.price_regions)
                        .map(|view| self.meters.view = Some(view))
                };
                if result.is_err() {
                    self.meters.view = None;
                    self.storage_issue = Some(l10n::text(
                        "无法从本地数据库读取用量指标",
                        "Usage metrics could not be read from SQLite",
                    ))
                }
                self.meters.changes.clear();
            }
        }
        if let Some(view) = &mut self.meters.view {
            if view.valid_until.is_some_and(|until| now >= until) || now < view.loaded_at {
                view.refresh_clock(now);
            }
        }
        if self.meters.samples.is_none() {
            if let Some(database) = &self.database {
                match metering::samples(database, now) {
                    Ok(samples) => self.meters.samples = Some(samples),
                    Err(_) => {
                        self.storage_issue = Some(l10n::text(
                            "无法从本地数据库读取额度记录",
                            "Quota history could not be read from SQLite",
                        ))
                    }
                }
            }
        }
        Ok(now)
    }

    /// The recent sessions, most recently active first, with their titles,
    /// context, window shares and totals, and the sessions only client events
    /// know about.
    /// The session list. Its rows are built again only when a session input
    /// changed: the session fingerprint, the logs read, the indexes sessions
    /// are presented from, the hook facts, the requests, or the currency
    /// costs are shown in. The Claude window shares are laid over them and
    /// the sessions only a hook knows are added.
    fn session_rows(&mut self, presented_at: i64, now: i64, at: i64) -> Vec<Value> {
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
            sessions.extend(extras.into_iter().filter(|value| {
                self.enabled
                    .get(value["source"].as_str().unwrap_or(""))
                    .copied()
                    .unwrap_or(true)
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

    /// Every client Agent HUD knows, with what was found of it and its account,
    /// and `recent`: what its calls in the 24 hours before `at` used and cost
    /// at list prices (null when none were made).
    fn provider_rows(&self, at: i64) -> Vec<Value> {
        let projection = self
            .usage_projection
            .as_ref()
            .expect("usage projection prepared");
        let recent = self
            .meters
            .view
            .as_ref()
            .map(|view| view.recent(at))
            .unwrap_or_default();
        agent_hud_agents::ALL
            .iter()
            .map(|provider| {
                let id = provider.id;
                let mut discovery = self
                    .discovery
                    .iter()
                    .find(|found| found.id == id)
                    .cloned()
                    .unwrap_or_default();
                let has_sessions = projection.observed_providers.contains(id);
                if has_sessions {
                    discovery.usage = true;
                    if !discovery
                        .evidence
                        .iter()
                        .any(|item| item == "parsed-local-sessions")
                    {
                        discovery.evidence.push("parsed-local-sessions".into());
                    }
                }
                let detected = discovery.installed || discovery.configured || discovery.usage;
                // A source's notice is what its account reading says; how a
                // reader works is not the person's concern (Open shows none).
                let notice = Value::Null;
                let mut row = json!({"id":id,"name":provider.name,"recent":recent.get(id).map(metering::Recent::json),"detected":detected,"enabled":self.enabled.get(id).copied().unwrap_or(true),"notice":notice,"sessions":projection.counts.get(id).copied().unwrap_or_default(),"discovery":discovery,"dataStatus":if provider.sessions {"supported"} else {"detection-only"}});
                if let Some(account) = self.accounts.providers.get(id) {
                    row["accountStatus"] = account.clone();
                    row["notice"] = account["notice"].clone();
                    if account["status"] == "ready" {
                        row["detected"] = json!(true);
                        if !provider.sessions {
                            row["dataStatus"] = json!("account-only");
                        }
                    }
                }
                row
            })
            .collect()
    }

    /// The quota windows of enabled clients with their metrics, and WorkBuddy's
    /// ceilings, each with a short name distinct among its account's windows.
    fn quota_rows(
        &self,
        at: i64,
        now: i64,
        pending_meter: &HashMap<&str, Option<i64>>,
    ) -> Vec<Value> {
        let metering_complete = |source: &str, cutoff: i64| {
            self.database.is_some()
                && pending_meter
                    .get(source)
                    .is_none_or(|latest| latest.is_some_and(|latest| latest < cutoff))
        };
        let mut quotas = self
            .accounts
            .quotas
            .iter()
            .filter(|value| self.shown(value, "vendor"))
            .map(|value| {
                let mut row = value.clone();
                if let Some(view) = &self.meters.view {
                    let source = value["consumerSource"].as_str().unwrap_or("");
                    let cycle_start = value["resetAt"]
                        .as_str()
                        .and_then(metering::date_ms)
                        .zip(value["windowDurationSeconds"].as_f64())
                        .and_then(|(reset, duration)| reset.checked_sub((duration * 1000.0) as i64))
                        .unwrap_or(at - 7 * 86_400_000);
                    row["metrics"] = view.metrics(
                        value,
                        at,
                        metering_complete(source, cycle_start),
                        metering_complete(source, at - 7 * 86_400_000),
                        self.meters.samples.as_ref().unwrap_or(&Default::default()),
                    );
                }
                // legacyWindowId stays: panel layouts, Settings and get_hud name a
                // row by it (src/packages/hud/src/quota-rows.ts).
                if let Some(object) = row.as_object_mut() {
                    for key in ["consumerSource", "modelFamilies"] {
                        object.remove(key);
                    }
                }
                row
            })
            .collect::<Vec<_>>();
        if self.enabled.get("workbuddy").copied().unwrap_or(true) {
            let cache_path = self
                .settings_path
                .parent()
                .map(|dir| dir.join("workbuddy-ceilings.json"));
            for row in crate::clients::workbuddy::ceiling_quotas(
                self.logs.values(),
                cache_path.as_deref(),
                now,
            ) {
                quotas.push(row);
            }
        }
        crate::naming::window_names::settle_short_names(&mut quotas);
        quotas
    }

    /// The balances of enabled clients with their metrics and level; a
    /// relay's balance once under each agent that uses it.
    fn billing_rows(
        &self,
        at: i64,
        pending_meter: &HashMap<&str, Option<i64>>,
        relay_users: &HashMap<String, Vec<String>>,
    ) -> Vec<Value> {
        self.accounts
            .billing
            .iter()
            .flat_map(|value| under_agents(value, relay_users))
            .filter(|value| self.shown(value, "vendor"))
            .map(|mut row| {
                row["metrics"] =
                    self.meters
                        .billing
                        .metrics(&row, at, &self.provider_roots, pending_meter);
                row["level"] = json!(billing_metrics::balance_level(&row));
                if let Some(object) = row.as_object_mut() {
                    object.remove("consumerHome");
                    object.remove("consumerRoutes");
                }
                row
            })
            .collect::<Vec<_>>()
    }

    /// The agents that use each relay, by its host: the clients configured
    /// with it, then the agents whose calls in the last 30 days went through
    /// one of its routes.
    fn relay_users(&self, at: i64) -> HashMap<String, Vec<String>> {
        let relays = self
            .accounts
            .services
            .iter()
            .filter(|row| row["product"] == "relay")
            .collect::<Vec<_>>();
        if relays.is_empty() {
            return HashMap::new();
        }
        let strings = |value: &'_ Value| -> Vec<String> {
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        };
        let routes = relays
            .iter()
            .flat_map(|row| row["routes"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        let by_route = self
            .meters
            .view
            .as_ref()
            .map(|view| view.sources_by_route(at - 30 * 86_400_000, &routes))
            .unwrap_or_default();
        relays
            .iter()
            .filter_map(|row| {
                let host = row["provider"].as_str()?;
                let mut users = strings(&row["clients"]);
                for route in strings(&row["routes"]) {
                    for source in by_route.get(&route).into_iter().flatten() {
                        if !users.contains(source) {
                            users.push(source.clone());
                        }
                    }
                }
                Some((host.to_owned(), users))
            })
            .collect()
    }

    /// The relays, one per address, as Settings lists them: `{provider (the
    /// host), clients, agents (who uses it), consented, kind, balance,
    /// notice}` with when they were last read.
    fn relay_rows(&self, relay_users: &HashMap<String, Vec<String>>) -> Vec<Value> {
        let checked = &self
            .accounts
            .providers
            .get("relay")
            .map(|state| state["checkedAt"].clone());
        self.accounts
            .services
            .iter()
            .filter(|row| row["product"] == "relay")
            .map(|row| {
                let mut row = row.clone();
                row["checkedAt"] = checked.clone().unwrap_or(Value::Null);
                row["agents"] = json!(row["provider"]
                    .as_str()
                    .and_then(|host| relay_users.get(host))
                    .cloned()
                    .unwrap_or_default());
                // The price list and routes feed pricing; Settings has no use
                // for them.
                if let Some(object) = row.as_object_mut() {
                    object.remove("prices");
                    object.remove("routes");
                }
                row
            })
            .collect()
    }

    /// The accounts of enabled clients, as their section headers show them.
    fn account_rows(&self) -> Vec<Value> {
        self.accounts
            .accounts
            .iter()
            .filter(|value| self.shown(value, "provider") || self.shown(value, "vendor"))
            .map(|value| self.account_heading(value))
            .collect::<Vec<_>>()
    }

    /// Whether the client a row names under `key` is turned on.
    fn shown(&self, value: &Value, key: &str) -> bool {
        value[key]
            .as_str()
            .is_some_and(|id| self.enabled.get(id).copied().unwrap_or(true))
    }

    /// The time rates, forecasts and cost windows are worked out at: when what
    /// they are made of last changed (usage written, a quota or balance read),
    /// and at most a minute ago. A pass that brought nothing new then renders
    /// the snapshot it rendered before, and the host sends pages only the time.
    fn metrics_clock(&mut self, now: i64) -> i64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.statistics.revision.hash(&mut hasher);
        for rows in [&self.accounts.quotas, &self.accounts.billing] {
            serde_json::to_string(rows)
                .unwrap_or_default()
                .hash(&mut hasher);
        }
        let inputs = hasher.finish();
        match self.metrics_clock {
            Some((seen, at)) if seen == inputs && (at..at + 60_000).contains(&now) => at,
            _ => {
                self.metrics_clock = Some((inputs, now));
                now
            }
        }
    }
}

/// A balance as the agents see it: a relay's (`vendor` `relay`) once under
/// each agent that uses it, as that agent's, and none when no agent does;
/// any other as it is.
fn under_agents(row: &Value, relay_users: &HashMap<String, Vec<String>>) -> Vec<Value> {
    let Some(relay) = row["relay"].as_str().filter(|_| row["vendor"] == "relay") else {
        return vec![row.clone()];
    };
    relay_users
        .get(relay)
        .into_iter()
        .flatten()
        .map(|agent| {
            let mut row = row.clone();
            row["vendor"] = json!(agent);
            row["id"] = json!(format!("relay:{agent}:{relay}"));
            if let Some(object) = row.as_object_mut() {
                object.remove("clients");
            }
            row
        })
        .collect()
}

#[cfg(test)]
mod relay_balance_tests {
    use super::*;

    #[test]
    fn a_relays_balance_shows_under_each_agent_that_uses_it() {
        let relay = json!({"id": "relay:pool.example.net", "vendor": "relay", "relay": "pool.example.net",
            "clients": ["claude"], "consumerRoutes": ["pool.example.net"], "amount": 8.0, "currency": "USD"});
        let users = HashMap::from([(
            "pool.example.net".to_string(),
            vec!["claude".to_string(), "pi".to_string()],
        )]);
        let rows = under_agents(&relay, &users);
        assert_eq!(
            rows.iter()
                .map(|row| (
                    row["vendor"].clone(),
                    row["id"].clone(),
                    row["amount"].clone()
                ))
                .collect::<Vec<_>>(),
            [
                (
                    json!("claude"),
                    json!("relay:claude:pool.example.net"),
                    json!(8.0)
                ),
                (json!("pi"), json!("relay:pi:pool.example.net"), json!(8.0)),
            ]
        );
        assert!(rows[0]["clients"].is_null());
        assert!(
            under_agents(&relay, &HashMap::new()).is_empty(),
            "no agent uses it"
        );
        let own = json!({"id": "deepseek:a:CNY", "vendor": "deepseek", "amount": 1.0});
        assert_eq!(under_agents(&own, &users), std::slice::from_ref(&own));
    }
}
