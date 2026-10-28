//! A session's tokens, its money and its breakdown on the session page: its
//! own log and the sub-agent logs under it, the breakdown read from the usage
//! ledger by query.
use super::*;
use crate::session_usage::{self, Builder, Call};
use std::hash::{Hash, Hasher};

/// One call of a session's logs as the breakdown takes it. Only the session's
/// own log (`own_key`) marks its turns; a sub-agent's prompts are steps of the
/// turn that started it. Its own log records every call.
fn own_call<'a>(row: &'a metering::CallRow, own_key: &str) -> Call<'a> {
    Call {
        row,
        own: row.contribution == own_key,
        call_log: row.contribution == own_key,
    }
}

/// The rows' look at their breakdowns (`AgentHudCore::session_row_usage`) by
/// the session's own log: a key of its logs' read offsets, and the value.
#[derive(Default)]
pub(crate) struct SessionRows(pub(crate) std::sync::Mutex<HashMap<PathBuf, (u64, Value)>>);

impl SessionRows {
    /// Forgets the sessions no longer listed, so the map follows the logs.
    pub(crate) fn retain(&self, listed: &HashSet<&Path>) {
        if let Ok(mut rows) = self.0.lock() {
            rows.retain(|path, _| listed.contains(path.as_path()));
        }
    }
}

/// The sub-agents with calls in a turn, for the session page's timeline: each
/// one's log (the calls' `log`), name and type, the log that started it, when
/// it began and was last heard from, and whether it still runs. Claude Code
/// keeps a sub-agent's description, type and end beside its log
/// (`agent-*.meta.json`); other clients' child sessions use their titles.
fn turn_agents(own: &LogHead, subagents: &[&LogHead], calls: &[Call<'_>], now: i64) -> Vec<Value> {
    let called = calls
        .iter()
        .filter(|call| !call.own)
        .map(|call| call.row.contribution.as_str())
        .collect::<HashSet<_>>();
    let logs = || std::iter::once(own).chain(subagents.iter().copied());
    let at = |ms: Option<i64>| {
        ms.and_then(DateTime::<Utc>::from_timestamp_millis)
            .map(|at| at.to_rfc3339())
    };
    subagents
        .iter()
        .filter_map(|log| {
            let path = log.path.to_string_lossy().into_owned();
            if !called.contains(path.as_str()) {
                return None;
            }
            let meta = fs::read(log.path.with_extension("meta.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .unwrap_or(Value::Null);
            let name = meta["description"]
                .as_str()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| log.title.clone());
            let parent = log.live.parent_id.as_deref().and_then(|parent| {
                logs()
                    .find(|other| other.source == log.source && other.id == parent)
                    .map(|other| other.path.to_string_lossy().into_owned())
            });
            let finished = meta["finished"].as_bool().unwrap_or(false);
            let running = !finished && log.live.session_state(now, &log.source, None) == "running";
            Some(json!({
                "log": path,
                "name": name,
                "kind": meta["agentType"].as_str(),
                "parent": parent,
                "startedAt": at(log.live.started_at()),
                "lastAt": at(log.live.last_event()),
                "running": running,
            }))
        })
        .collect()
}

/// The selected logs by the session that started them, so a session's
/// sub-agents can be found at any depth. Decided 2026-10-21, as
/// agent-hud-native-open does: a session counts its sub-agents in its tokens,
/// its breakdown and its money.
pub(crate) struct Family<'a> {
    children: HashMap<(&'a str, &'a str), Vec<&'a LogHead>>,
}

impl<'a> Family<'a> {
    pub(crate) fn new(logs: &[&'a LogHead]) -> Self {
        let mut children = HashMap::<(&str, &str), Vec<&LogHead>>::new();
        for log in logs {
            if let Some(parent) = log
                .live
                .parent_id
                .as_deref()
                .filter(|parent| *parent != log.id)
            {
                children
                    .entry((log.source.as_str(), parent))
                    .or_default()
                    .push(log);
            }
        }
        Self { children }
    }

    /// The sub-agent logs under a session, at any depth.
    pub(crate) fn descendants(&self, source: &str, id: &str) -> Vec<&'a LogHead> {
        let mut found = Vec::<&LogHead>::new();
        let mut seen = HashSet::<&Path>::new();
        let mut queue = vec![(source.to_owned(), id.to_owned())];
        while let Some((source, id)) = queue.pop() {
            for child in self
                .children
                .get(&(source.as_str(), id.as_str()))
                .into_iter()
                .flatten()
            {
                if seen.insert(child.path.as_path()) {
                    found.push(child);
                    queue.push((child.source.clone(), child.id.clone()));
                }
            }
        }
        found
    }

    /// A session's tokens with its sub-agents' at any depth, and the
    /// sub-agents' part of them; None when no sub-agent spent any. Each log
    /// still counts once in totals that add every log; only the session's
    /// row and page add its sub-agents to it.
    pub(crate) fn usage(&self, head: &LogHead) -> (Usage, Option<Usage>) {
        let mut subagents = None::<Usage>;
        for log in self.descendants(&head.source, &head.id) {
            if log.usage.total() > 0 {
                subagents.get_or_insert_with(Usage::default).add(log.usage);
            }
        }
        let mut total = head.usage;
        if let Some(part) = subagents {
            total.add(part);
        }
        (total, subagents)
    }

    /// What a session's calls and its sub-agents' would cost at API list
    /// prices, in US dollars; None when one with calls has no list price.
    pub(crate) fn list_cost(&self, head: &LogHead) -> Option<f64> {
        let mut total = None::<i128>;
        for log in std::iter::once(head).chain(self.descendants(&head.source, &head.id)) {
            match log.list_cost {
                Some(cost) => total = Some(total.unwrap_or(0) + cost),
                None if log.usage.total() == 0 => {}
                None => return None,
            }
        }
        total.map(|pico| pico as f64 / 1e12)
    }

    /// What the calls with a list price, of a session and its sub-agents,
    /// cost at API list prices in US dollars (None when no call has one),
    /// and the tokens of the calls with none.
    pub(crate) fn priced_cost(&self, head: &LogHead) -> (Option<f64>, u64) {
        let mut priced = None::<i128>;
        let mut unpriced = 0;
        for log in std::iter::once(head).chain(self.descendants(&head.source, &head.id)) {
            // A log priced before the priced part was kept has only its whole
            // cost until it is priced again.
            let part = match log.unpriced_tokens {
                Some(_) => log.priced_cost,
                None => log.list_cost,
            };
            if let Some(cost) = part {
                priced = Some(priced.unwrap_or(0) + cost);
            }
            unpriced += log.unpriced_tokens.unwrap_or(0);
        }
        (priced.map(|pico| pico as f64 / 1e12), unpriced)
    }
}

impl AgentHudCore {
    /// The currency of the DeepSeek balance this Mac's Harness calls are billed
    /// to, when one is linked and switched on.
    pub(crate) fn deepseek_account_currency(&self) -> Option<String> {
        if !self.enabled.get("deepseek").copied().unwrap_or(true) {
            return None;
        }
        self.accounts
            .billing
            .iter()
            .find(|row| billing_metrics::bound(row, &self.provider_roots))
            .and_then(|row| row["currency"].as_str())
            .filter(|currency| matches!(*currency, "USD" | "CNY"))
            .map(str::to_owned)
    }

    /// A session's tokens and money, as its row and its page show them
    /// (agent-hud-native-open `sessionTokens`, `SessionMoney`): `tokens` and `usage`
    /// (the same by kind) with its sub-agents', `cost` at API list prices with
    /// its sub-agents (null when a call has no list price), `pricedCost` for
    /// the calls that have one with `unpricedTokens` for the rest, and, for a
    /// DeepSeek session billed to a linked account, `accountCost` at the
    /// account's own prices in its currency (null when a call has no price).
    pub(crate) fn session_totals(
        value: &mut Value,
        head: &LogHead,
        family: &Family,
        currency: Option<&str>,
    ) {
        let (usage, _) = family.usage(head);
        value["tokens"] = json!(usage.total());
        value["usage"] = usage.json();
        value["cost"] = json!(family.list_cost(head));
        let (priced, unpriced) = family.priced_cost(head);
        value["pricedCost"] = json!(priced);
        value["unpricedTokens"] = json!(unpriced);
        if let Some(currency) = currency.filter(|_| head.source == "deepseek") {
            value["accountCost"] = head
                .account_cost_in(currency)
                .map(|amount| json!({"amount": amount, "currency": currency}))
                .unwrap_or(Value::Null);
        }
    }

    /// What WorkBuddy's own metering charged a session (agent-hud-native-open
    /// `SessionCredits`): its credits, the part no one model could be pinned
    /// on, two decimals each, and the context it metered.
    pub(crate) fn session_credits(&self, id: &str) -> Value {
        let Some(credits) = self
            .workbuddy_metering
            .credits
            .get(id)
            .filter(|credits| credits.total > 0.0)
        else {
            return Value::Null;
        };
        // Two decimals, with the noise a floating-point sum leaves behind
        // rounded away rather than shown.
        let text = |value: f64| format!("{:.2}", (value * 100.0).round() / 100.0);
        json!({
            "total": credits.total,
            "text": text(credits.total),
            "unattributed": credits.unattributed,
            "unattributedText": (credits.unattributed > 0.005).then(|| text(credits.unattributed)),
            "attributedShare": (1.0 - credits.unattributed / credits.total).max(0.0),
            "byModel": credits.by_model,
            "used": (credits.size > 0).then_some(credits.used),
            "size": (credits.size > 0).then_some(credits.size),
        })
    }

    /// The breakdown of the session whose own log is `own`, with the logs of
    /// `subagents`, and the calls it was built from; None without calls.
    fn session_usage(
        db: &Connection,
        own: &LogHead,
        subagents: &[&LogHead],
    ) -> Result<Option<(session_usage::SessionUsage, Vec<metering::CallRow>)>, String> {
        let own_paths = vec![own.path.to_string_lossy().into_owned()];
        let mut paths = own_paths.clone();
        paths.extend(
            subagents
                .iter()
                .map(|log| log.path.to_string_lossy().into_owned()),
        );
        let failed = |_| l10n::text("无法读取会话用量", "Could not read session usage");
        let (prompts, compactions) = metering::marks(db, &own_paths).map_err(failed)?;
        let rows = metering::calls_of(db, &paths, None, None).map_err(failed)?;
        let mut builder = Builder::new(prompts, compactions);
        for row in &rows {
            builder.add(own_call(row, &own_paths[0]));
        }
        if builder.is_empty() {
            return Ok(None);
        }
        let usage = builder.build(|source, model, reported| {
            let largest = reported
                .is_none()
                .then(|| metering::largest_context(db, source, model).ok().flatten())
                .flatten();
            pricing::context_window(source, model, reported, largest)
        });
        Ok(Some((usage, rows)))
    }

    /// What the session's row shows of its breakdown (`SessionUsage::row`),
    /// kept until one of its logs, its sub-agents' included, is read further,
    /// so an unchanged session costs no query on the next page. Null without
    /// a ledger or calls.
    pub(crate) fn session_row_usage(&self, own: &LogHead, family: &Family) -> Value {
        let Some(db) = &self.database else {
            return Value::Null;
        };
        let subagents = family.descendants(&own.source, &own.id);
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for log in std::iter::once(own).chain(subagents.iter().copied()) {
            log.path.hash(&mut hash);
            (log.offset, log.length, log.usage.total()).hash(&mut hash);
        }
        let key = hash.finish();
        let Ok(mut rows) = self.session_rows.0.lock() else {
            return Value::Null;
        };
        if let Some((kept, value)) = rows.get(&own.path) {
            if *kept == key {
                return value.clone();
            }
        }
        let value = match Self::session_usage(db, own, &subagents) {
            Ok(Some((usage, _))) => usage.row(),
            Ok(None) => Value::Null,
            // A failed read is not kept; the next page tries again.
            Err(_) => return Value::Null,
        };
        rows.insert(own.path.clone(), (key, value.clone()));
        value
    }

    /// The breakdown of the session whose own log is `own`, with the logs of
    /// `subagents`: its turns (or 15-minute periods), context, models and list
    /// cost; and, for `turn` (a kept turn's id), that turn's calls and what
    /// they add up to.
    pub(crate) fn session_breakdown(
        db: &Connection,
        own: &LogHead,
        subagents: &[&LogHead],
        turn: Option<&str>,
    ) -> Result<Option<Value>, String> {
        let Some((usage, rows)) = Self::session_usage(db, own, subagents)? else {
            return Ok(None);
        };
        let own_paths = vec![own.path.to_string_lossy().into_owned()];
        let failed = |_| l10n::text("无法读取会话用量", "Could not read session usage");
        let call = |row| own_call(row, &own_paths[0]);
        let mut value = usage.json();
        // The session at its account's own prices (agent-hud-native-open
        // `UsageLedger` costs): a currency only when every priced log of the
        // session and its sub-agents has a price in it.
        let priced = std::iter::once(own)
            .chain(subagents.iter().copied())
            .filter_map(|log| log.account_cost)
            .collect::<Vec<_>>();
        let mut costs = BTreeMap::new();
        if !priced.is_empty() {
            for (index, currency) in ["USD", "CNY"].into_iter().enumerate() {
                if let Some(total) = priced
                    .iter()
                    .try_fold(0i128, |sum, cost| Some(sum + cost[index]?))
                {
                    costs.insert(currency.to_owned(), total);
                }
            }
        }
        value["sessionUsage"] = usage.open_json(&costs);
        if let Some(turn) = turn.and_then(|id| {
            usage
                .turns
                .iter()
                .find(|turn| session_usage::SessionUsage::turn_id(turn) == id)
        }) {
            let previous = metering::prompt_before(db, &own_paths, turn.start).map_err(failed)?;
            let calls = rows
                .iter()
                .filter(|row| row.at >= turn.start && row.at <= turn.end)
                .map(call)
                .collect::<Vec<_>>();
            let summary = session_usage::turn_summary(&calls);
            let agents = turn_agents(own, subagents, &calls, Utc::now().timestamp_millis());
            let calls = session_usage::turn_calls(&calls, previous);
            let id = session_usage::SessionUsage::turn_id(turn);
            if let Some(entry) = value["turns"]
                .as_array_mut()
                .and_then(|turns| turns.iter_mut().find(|entry| entry["id"] == id.as_str()))
            {
                entry["calls"] = json!(calls);
            }
            value["turnCalls"] =
                json!({"turn": id, "calls": calls, "summary": summary, "agents": agents});
        }
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_family_costs_what_its_priced_calls_cost_and_counts_the_rest() {
        let head = |path: &str, id: &str, parent: Option<&str>| {
            let mut log = Log::new(PathBuf::from(path), "claude");
            log.id = id.into();
            let mut head = LogHead::from_log(&log);
            head.live.parent_id = parent.map(str::to_owned);
            head
        };
        let dollars = |amount: i128| amount * 1_000_000_000_000;
        let mut own = head("/a.jsonl", "s", None);
        own.set_list_cost(Some(metering::ListCost {
            priced: Some(dollars(2)),
            unpriced_tokens: 0,
        }));
        let mut helper = head("/b.jsonl", "h", Some("s"));
        helper.set_list_cost(Some(metering::ListCost {
            priced: Some(dollars(1)),
            unpriced_tokens: 500,
        }));
        // Priced before the priced part was kept: only its whole cost.
        let mut older = head("/c.jsonl", "o", Some("s"));
        older.list_cost = Some(dollars(3));
        older.list_cost_priced = true;
        let logs = [&own, &helper, &older];
        assert_eq!(Family::new(&logs).priced_cost(&own), (Some(6.0), 500));
        let alone = [&own];
        assert_eq!(Family::new(&alone).priced_cost(&own), (Some(2.0), 0));
    }

    fn core(home: &Path) -> AgentHudCore {
        let mut core = AgentHudCore::new(vec![], home.join("app"));
        core.provider_roots = vec![
            (home.join(".claude/projects"), "claude"),
            (home.join(".codex/sessions"), "codex"),
        ];
        core
    }

    fn iso(at: i64) -> String {
        DateTime::<Utc>::from_timestamp_millis(at)
            .unwrap()
            .to_rfc3339()
    }

    fn lines(rows: &[Value]) -> String {
        rows.iter().map(|row| format!("{row}\n")).collect()
    }

    #[test]
    fn a_claude_session_breaks_down_by_prompt_with_its_sub_agents_tools_and_compactions() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".claude/projects/p");
        fs::create_dir_all(&root).unwrap();
        let t = Utc::now().timestamp_millis() - 3_600_000;
        let id = "11111111-2222-3333-4444-555555555555";
        let user = |at: i64, text: &str| json!({"type":"user","sessionId":id,"timestamp":iso(at),"uuid":format!("u{at}"),"message":{"role":"user","content":text}});
        let reply = |at: i64, message: &str, content: Value, input: u64, write: u64, read: u64| json!({"type":"assistant","sessionId":id,"timestamp":iso(at),"message":{"id":message,"model":"claude-opus-4-5","role":"assistant","content":content,"usage":{"input_tokens":input,"output_tokens":20,"cache_creation_input_tokens":write,"cache_read_input_tokens":read}}});
        let path = root.join(format!("{id}.jsonl"));
        fs::write(&path, lines(&[
            user(t, "Fix the build"),
            reply(t + 1_000, "m1", json!([{"type":"text","text":"Looking."}]), 10, 3_000, 0),
            // The same response's next line asks for a tool; its usage did not change.
            reply(t + 1_100, "m1", json!([{"type":"tool_use","id":"tu1","name":"Bash","input":{}}]), 10, 3_000, 0),
            reply(t + 2_000, "m2", json!([{"type":"tool_use","id":"tu2","name":"Read","input":{}}]), 10, 500, 3_000),
            json!({"type":"system","subtype":"compact_boundary","sessionId":id,"timestamp":iso(t + 2_500)}),
            user(t + 3_000, "[Request interrupted by user]"),
            user(t + 10_000, "Now ship it"),
            reply(t + 11_000, "m3", json!([{"type":"text","text":"Shipped."}]), 10, 2_000, 0),
        ])).unwrap();
        // A sub-agent of the first turn, in its own log.
        fs::create_dir_all(root.join(format!("{id}/subagents"))).unwrap();
        fs::write(root.join(format!("{id}/subagents/agent-a1.jsonl")), lines(&[
            json!({"type":"user","sessionId":id,"isSidechain":true,"timestamp":iso(t + 1_200),"message":{"role":"user","content":"Check the tests"}}),
            json!({"type":"assistant","sessionId":id,"isSidechain":true,"timestamp":iso(t + 1_500),"message":{"id":"sa1","model":"claude-haiku-4-5","role":"assistant","content":[{"type":"tool_use","id":"tu3","name":"Grep","input":{}}],"usage":{"input_tokens":100,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}),
        ])).unwrap();
        fs::write(
            root.join(format!("{id}/subagents/agent-a1.meta.json")),
            r#"{"agentType":"Explore","description":"Check the tests","finished":true}"#,
        )
        .unwrap();
        let mut core = core(temp.path());
        core.request("snapshot", json!({})).unwrap();
        let detail = core.request("session_detail", json!({"id": id})).unwrap();
        assert_eq!(detail["turnsBy"], "prompt", "{detail:#}");
        assert_eq!(detail["turnCount"], 2, "an interruption starts no turn");
        assert_eq!(detail["callCount"], 4);
        let turns = detail["turns"].as_array().unwrap();
        assert_eq!(
            turns[0]["callCount"], 3,
            "the sub-agent's call belongs to the turn that started it"
        );
        assert_eq!(turns[0]["compacted"], true);
        assert_eq!(turns[0]["subagent"], true);
        assert_eq!(turns[0]["context"], 3_510);
        assert_eq!(
            turns[1]["recached"], 2_010,
            "nothing of the 3,510 before it came back from the cache"
        );
        assert_eq!(detail["contextTokens"], 2_010);
        assert_eq!(detail["contextWindow"], 200_000);
        assert_eq!(detail["subagents"]["input"], 100);
        assert!(detail["listCost"].as_f64().is_some_and(|cost| cost > 0.0));
        let session_cost = detail["session"]["cost"].as_f64().unwrap();
        assert!(
            (session_cost - detail["listCost"].as_f64().unwrap()).abs() < 1e-9,
            "the session's money counts its sub-agents"
        );
        assert_eq!(detail["models"].as_array().unwrap().len(), 2);

        // The same breakdown as agent-hud-native-open's SessionUsage, for a host to publish.
        let open = &detail["sessionUsage"];
        assert_eq!(open["models"][0]["agentId"], "claude-model:claude-opus-4-5");
        assert_eq!(
            open["models"][0]["tokens"],
            json!({"tokensIn": 5_530, "tokensOut": 60, "cacheReadTokens": 3_000, "cacheWriteTokens": 5_500, "reasoningTokens": 0})
        );
        assert_eq!(
            open["models"][1]["agentId"],
            "claude-model:claude-haiku-4-5"
        );
        assert_eq!(open["subagents"]["tokensIn"], 100);
        assert_eq!(open["calls"], 4);
        assert_eq!(open["contextTokens"], 2_010);
        assert_eq!(open["contextWindow"], 200_000);
        assert_eq!(open["turnCount"], 2);
        assert_eq!(open["turns"][0]["compacted"], true);
        assert_eq!(open["turns"][0]["subagents"]["tokensIn"], 100);
        assert_eq!(open["turns"][1]["recachedTokens"], 2_010);
        assert_eq!(
            open["costs"],
            Value::Null,
            "only an account's own prices make costs"
        );
        let periods = open["periods"].as_array().unwrap();
        assert!(periods.windows(2).all(|pair| (
            pair[0]["startMs"].as_i64(),
            pair[0]["agentId"].as_str()
        ) < (
            pair[1]["startMs"].as_i64(),
            pair[1]["agentId"].as_str()
        )));
        let opus_in = periods
            .iter()
            .filter(|period| period["agentId"] == "claude-model:claude-opus-4-5")
            .map(|period| period["tokens"]["tokensIn"].as_u64().unwrap())
            .sum::<u64>();
        assert_eq!(opus_in, 5_530, "each model keeps its own periods");
        let list = open["listCost"].as_str().unwrap().parse::<f64>().unwrap();
        assert!((list - detail["listCost"].as_f64().unwrap()).abs() < 1e-9);

        // The calls of a picked turn carry the tools each asked for.
        let turn = turns[0]["id"].as_str().unwrap().to_owned();
        let picked = core
            .request("session_detail", json!({"id": id, "turn": turn}))
            .unwrap();
        let calls = picked["turnCalls"]["calls"].as_array().unwrap();
        let tools = calls
            .iter()
            .map(|call| call["tools"].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            tools,
            vec![json!(["Bash"]), json!(["Grep"]), json!(["Read"])]
        );
        assert_eq!(calls[1]["own"], false);
        assert_eq!(
            calls[1]["context"],
            Value::Null,
            "a sub-agent's call is not the session's context"
        );
        assert_eq!(picked["turns"][0]["calls"].as_array().unwrap().len(), 3);
        // Its sub-agents, for the timeline: named from the meta file beside the log.
        let agents = picked["turnCalls"]["agents"].as_array().unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0]["log"], calls[1]["log"]);
        assert_eq!(agents[0]["name"], "Check the tests");
        assert_eq!(agents[0]["kind"], "Explore");
        assert_eq!(agents[0]["running"], false);
        assert_eq!(agents[0]["parent"], calls[0]["log"]);
        assert_eq!(agents[0]["startedAt"], iso(t + 1_200));
        // The own-calls page names them too.
        let page = picked["calls"].as_array().unwrap();
        assert!(page.iter().any(|call| call["tools"] == json!(["Read"])));

        // A tool line written after a scan joins its call on the next.
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(
            file,
            "{}",
            reply(
                t + 11_100,
                "m3",
                json!([{"type":"tool_use","id":"tu4","name":"Edit","input":{}}]),
                10,
                2_000,
                0
            )
        )
        .unwrap();
        core.request("refresh", json!({})).unwrap();
        let last = detail["turns"][1]["id"].as_str().unwrap().to_owned();
        let picked = core
            .request("session_detail", json!({"id": id, "turn": last}))
            .unwrap();
        assert_eq!(picked["turnCalls"]["calls"][0]["tools"], json!(["Edit"]));
    }

    #[test]
    fn a_session_counts_its_sub_agents_tokens_and_every_total_counts_each_log_once() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".claude/projects/p");
        let t = Utc::now().timestamp_millis() - 600_000;
        let id = "11111111-2222-3333-4444-777777777777";
        fs::create_dir_all(root.join(format!("{id}/subagents"))).unwrap();
        fs::write(root.join(format!("{id}.jsonl")), lines(&[
            json!({"type":"user","sessionId":id,"timestamp":iso(t),"message":{"role":"user","content":"Go"}}),
            json!({"type":"assistant","sessionId":id,"timestamp":iso(t + 500),"message":{"id":"m1","model":"claude-opus-4-5","role":"assistant","content":[{"type":"text","text":"Done."}],"usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":300,"cache_read_input_tokens":4_000}}}),
        ])).unwrap();
        fs::write(root.join(format!("{id}/subagents/agent-a1.jsonl")), lines(&[
            json!({"type":"user","sessionId":id,"isSidechain":true,"timestamp":iso(t + 100),"message":{"role":"user","content":"Look"}}),
            json!({"type":"assistant","sessionId":id,"isSidechain":true,"timestamp":iso(t + 200),"message":{"id":"sa1","model":"claude-haiku-4-5","role":"assistant","content":[{"type":"text","text":"Seen."}],"usage":{"input_tokens":100,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":50}}}),
        ])).unwrap();
        let mut core = core(temp.path());
        let snapshot = core.request("snapshot", json!({})).unwrap();
        let page = core
            .request("sessions", json!({"scope": "recent"}))
            .unwrap();
        let rows = page["items"].as_array().unwrap();
        assert_eq!(
            rows.len(),
            1,
            "the sub-agent nests under its parent: {page:#}"
        );
        let row = &rows[0];
        let usage = &row["usage"];
        // 10, 20, 300 and 4,000 of its own; 100, 5 and 50 of its sub-agent's.
        assert_eq!(usage["input"], 110);
        assert_eq!(usage["output"], 25);
        assert_eq!(usage["cacheWrite"], 300);
        assert_eq!(usage["cacheRead"], 4_050);
        assert_eq!(usage["reasoning"], 0);
        assert_eq!(
            usage["total"], row["tokens"],
            "the kinds add up to the row's tokens"
        );
        assert_eq!(
            row["tokens"], 4_485,
            "the parent counts its sub-agent's tokens (decided 2026-10-21)"
        );
        // The HUD's rows carry the same; the sub-agent's own row only its own.
        let live = snapshot["sessions"].as_array().unwrap();
        let parent = live.iter().find(|session| session["id"] == id).unwrap();
        assert_eq!(parent["tokens"], 4_485);
        assert_eq!(parent["usage"], *usage);
        let child = live
            .iter()
            .find(|session| session["parentId"] == id)
            .expect("the sub-agent's row");
        assert_eq!(child["tokens"], 155);

        // Totals that add every log count the sub-agent's log once.
        assert_eq!(
            snapshot["totals"]["total"], 4_485,
            "{:#}",
            snapshot["totals"]
        );
        let listed = rows
            .iter()
            .map(|row| row["tokens"].as_u64().unwrap())
            .sum::<u64>();
        assert_eq!(listed, 4_485, "the listed sessions add up to the total");
        let now = Utc::now();
        let statistics = core
            .request(
                "statistics",
                json!({
                    "from": (now - chrono::Duration::days(1)).to_rfc3339(),
                    "to": (now + chrono::Duration::minutes(1)).to_rfc3339(),
                    "timeZone": "UTC",
                    "tokenKinds": ["cacheWrite", "input", "reasoning", "output", "cacheRead"],
                }),
            )
            .unwrap();
        let sum = |tokens: &Value| {
            ["cacheWrite", "input", "reasoning", "output", "cacheRead"]
                .iter()
                .map(|kind| tokens[*kind].as_u64().unwrap_or(0))
                .sum::<u64>()
        };
        assert_eq!(
            sum(&statistics["totals"]["tokens"]),
            4_485,
            "{:#}",
            statistics["totals"]
        );
        let agents = statistics["providers"].as_array().unwrap();
        assert_eq!(
            agents
                .iter()
                .map(|agent| sum(&agent["tokens"]))
                .sum::<u64>(),
            4_485,
            "{agents:#?}"
        );

        // The page counts the same and names the sub-agents' part.
        let detail = core.request("session_detail", json!({"id": id})).unwrap();
        assert_eq!(detail["session"]["tokens"], 4_485);
        assert_eq!(detail["session"]["usage"], *usage);
        assert_eq!(detail["usage"], *usage);
        assert_eq!(detail["subagents"]["total"], 155);
        assert_eq!(detail["subagents"]["cacheRead"], 50);
        assert_eq!(
            detail["own"]["total"], 4_330,
            "the session's own log beside its sub-agents'"
        );
        assert_eq!(detail["children"][0]["tokens"], 155);
        // A sub-agent's own page counts only it, with no sub-agents' part.
        let child_detail = core
            .request("session_detail", json!({"id": child["id"]}))
            .unwrap();
        assert_eq!(child_detail["usage"]["total"], 155);
        assert_eq!(child_detail["subagents"], Value::Null);
    }

    #[test]
    fn the_sessions_page_rows_show_their_turns_until_a_log_moves_and_other_lists_do_not() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".claude/projects/p");
        let t = Utc::now().timestamp_millis() - 600_000;
        let id = "11111111-2222-3333-4444-999999999999";
        fs::create_dir_all(root.join(format!("{id}/subagents"))).unwrap();
        let user = |at: i64, text: &str| json!({"type":"user","sessionId":id,"timestamp":iso(at),"uuid":format!("u{at}"),"message":{"role":"user","content":text}});
        let reply = |at: i64, message: &str, input: u64, write: u64, read: u64| json!({"type":"assistant","sessionId":id,"timestamp":iso(at),"message":{"id":message,"model":"claude-opus-4-5","role":"assistant","content":[{"type":"text","text":"Done."}],"usage":{"input_tokens":input,"output_tokens":20,"cache_creation_input_tokens":write,"cache_read_input_tokens":read}}});
        let path = root.join(format!("{id}.jsonl"));
        fs::write(&path, lines(&[
            user(t, "Go"),
            reply(t + 500, "m1", 10, 3_000, 0),
            json!({"type":"system","subtype":"compact_boundary","sessionId":id,"timestamp":iso(t + 600)}),
            user(t + 1_000, "Again"),
            reply(t + 1_500, "m2", 10, 100, 3_000),
        ])).unwrap();
        fs::write(root.join(format!("{id}/subagents/agent-a1.jsonl")), lines(&[
            json!({"type":"user","sessionId":id,"isSidechain":true,"timestamp":iso(t + 100),"message":{"role":"user","content":"Look"}}),
            json!({"type":"assistant","sessionId":id,"isSidechain":true,"timestamp":iso(t + 200),"message":{"id":"sa1","model":"claude-haiku-4-5","role":"assistant","content":[{"type":"text","text":"Seen."}],"usage":{"input_tokens":100,"output_tokens":5}}}),
        ])).unwrap();
        let mut core = core(temp.path());
        core.request("snapshot", json!({})).unwrap();
        let plain = core
            .request("sessions", json!({"scope": "recent"}))
            .unwrap();
        assert!(
            plain["items"][0].get("turnBars").is_none(),
            "only the Sessions page asks: {plain:#}"
        );
        let page = core
            .request("sessions", json!({"scope": "recent", "turnBars": true}))
            .unwrap();
        let row = &page["items"][0];
        assert_eq!(row["turnCount"], 2, "{row:#}");
        assert_eq!(row["contextTokens"], 3_110);
        assert_eq!(row["contextFill"], 3_110.0 / 200_000.0);
        assert_eq!(row["turnBars"]["own"]["cacheWrite"], json!([3_000, 100]));
        assert_eq!(
            row["turnBars"]["subagents"]["input"],
            json!([100, 0]),
            "the sub-agent helped the first turn"
        );
        assert_eq!(row["turnBars"]["compacted"], json!([0]));
        // The page's turns agree with the row's bars.
        let detail = core.request("session_detail", json!({"id": id})).unwrap();
        assert_eq!(detail["turns"][0]["newTokens"], 3_000 + 10 + 20 + 105);
        assert_eq!(detail["turns"][0]["subagentNew"], 105);
        let picked = core
            .request(
                "session_detail",
                json!({"id": id, "turn": detail["turns"][0]["id"]}),
            )
            .unwrap();
        assert_eq!(
            picked["turnCalls"]["summary"]["subagentCalls"], 1,
            "{:#}",
            picked["turnCalls"]
        );
        assert_eq!(picked["turnCalls"]["summary"]["calls"], 2);
        assert_eq!(
            picked["turnCalls"]["calls"][0]["newTokens"], 105,
            "the sub-agent's call added all but its cache reads"
        );
        let models = detail["models"].as_array().unwrap();
        let opus = models
            .iter()
            .find(|model| model["model"] == "claude-opus-4-5")
            .unwrap();
        assert_eq!(
            opus["cacheHitRate"],
            3_000.0 / (3_000.0 + 3_100.0 + 20.0),
            "cache reads of every prompt"
        );

        // Unchanged, the row comes back as it was; a log read further moves it.
        let again = core
            .request("sessions", json!({"scope": "recent", "turnBars": true}))
            .unwrap();
        assert_eq!(again["items"][0]["turnBars"], row["turnBars"]);
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{}", user(t + 2_000, "Ship it")).unwrap();
        writeln!(file, "{}", reply(t + 2_500, "m3", 10, 50, 3_100)).unwrap();
        core.request("refresh", json!({})).unwrap();
        let moved = core
            .request("sessions", json!({"scope": "recent", "turnBars": true}))
            .unwrap();
        assert_eq!(moved["items"][0]["turnCount"], 3);
        assert_eq!(
            moved["items"][0]["turnBars"]["own"]["cacheWrite"],
            json!([3_000, 100, 50])
        );
        assert!(core.request("sessions", json!({"turnBars": 1})).is_err());
    }

    #[test]
    fn a_sessions_page_lists_sub_agents_the_snapshot_no_longer_carries() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".claude/projects/p");
        let t = Utc::now().timestamp_millis() - 3 * 86_400_000;
        let id = "11111111-2222-3333-4444-888888888888";
        fs::create_dir_all(root.join(format!("{id}/subagents"))).unwrap();
        fs::write(root.join(format!("{id}.jsonl")), lines(&[
            json!({"type":"user","sessionId":id,"timestamp":iso(t),"message":{"role":"user","content":"Go"}}),
            json!({"type":"assistant","sessionId":id,"timestamp":iso(t + 500),"message":{"id":"m1","model":"claude-opus-4-5","role":"assistant","content":[{"type":"text","text":"Done."}],"usage":{"input_tokens":10,"output_tokens":20}}}),
        ])).unwrap();
        fs::write(root.join(format!("{id}/subagents/agent-a1.jsonl")), lines(&[
            json!({"type":"user","sessionId":id,"isSidechain":true,"timestamp":iso(t + 100),"message":{"role":"user","content":"Look"}}),
            json!({"type":"assistant","sessionId":id,"isSidechain":true,"timestamp":iso(t + 200),"message":{"id":"sa1","model":"claude-haiku-4-5","role":"assistant","content":[{"type":"text","text":"Seen."}],"usage":{"input_tokens":100,"output_tokens":5}}}),
        ])).unwrap();
        let mut core = core(temp.path());
        let snapshot = core.request("snapshot", json!({})).unwrap();
        assert_eq!(
            snapshot["sessions"],
            json!([]),
            "three days old: not in the HUD's sessions"
        );
        let detail = core.request("session_detail", json!({"id": id})).unwrap();
        let children = detail["children"].as_array().unwrap();
        assert_eq!(
            children.len(),
            1,
            "the page still has its sub-agent: {detail:#}"
        );
        assert_eq!(children[0]["parentId"], id);
        assert_eq!(children[0]["tokens"], 105);
        assert_eq!(detail["session"]["tokens"], 135);
    }

    #[test]
    fn a_codex_thread_counts_the_agents_it_spawned_at_any_depth() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".codex/sessions/2026/10/21");
        fs::create_dir_all(&root).unwrap();
        let at = iso(Utc::now().timestamp_millis() - 60_000);
        let parent = "11111111-1111-4111-8111-111111111111";
        let child = "22222222-2222-4222-8222-222222222222";
        let grandchild = "33333333-3333-4333-8333-333333333333";
        let rollout = |id: &str, spawned_by: Option<&str>, input: u64, output: u64| {
            let source = match spawned_by {
                Some(parent) => {
                    json!({"subagent":{"thread_spawn":{"parent_thread_id":parent,"agent_nickname":"Ada"}}})
                }
                None => json!("cli"),
            };
            lines(&[
                json!({"timestamp":at,"type":"session_meta","payload":{"id":id,"timestamp":at,"cwd":"/w/repo","source":source,"originator":"codex_cli_rs"}}),
                json!({"timestamp":at,"type":"turn_context","payload":{"model":"gpt-5"}}),
                json!({"timestamp":at,"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":input,"output_tokens":output}}}}),
            ])
        };
        fs::write(
            root.join(format!("rollout-a-{parent}.jsonl")),
            rollout(parent, None, 100, 10),
        )
        .unwrap();
        fs::write(
            root.join(format!("rollout-b-{child}.jsonl")),
            rollout(child, Some(parent), 30, 3),
        )
        .unwrap();
        fs::write(
            root.join(format!("rollout-c-{grandchild}.jsonl")),
            rollout(grandchild, Some(child), 7, 1),
        )
        .unwrap();
        let mut core = core(temp.path());
        let snapshot = core.request("snapshot", json!({})).unwrap();
        assert_eq!(snapshot["totals"]["total"], 151, "each thread counts once");
        let roots = core
            .request("sessions", json!({"scope": "recent"}))
            .unwrap();
        let rows = roots["items"].as_array().unwrap();
        assert_eq!(rows.len(), 1, "{roots:#}");
        assert_eq!(rows[0]["id"], parent);
        assert_eq!(rows[0]["tokens"], 151, "its agent and that agent's agent");
        let children = core
            .request("sessions", json!({"scope": "recent", "parentId": parent}))
            .unwrap();
        assert_eq!(
            children["items"][0]["tokens"], 41,
            "a spawned agent counts the agent it spawned"
        );
        let detail = core
            .request("session_detail", json!({"id": parent}))
            .unwrap();
        assert_eq!(detail["subagents"]["total"], 41);
        assert_eq!(
            detail["children"].as_array().unwrap().len(),
            1,
            "only direct children are listed"
        );
        assert_eq!(detail["children"][0]["tokens"], 41);
    }

    #[test]
    fn a_review_no_list_shows_still_adds_to_its_thread_and_moves_the_list() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".codex/sessions/2026/10/21");
        fs::create_dir_all(&root).unwrap();
        let at = iso(Utc::now().timestamp_millis() - 60_000);
        let parent = "44444444-4444-4444-8444-444444444444";
        let review = "55555555-5555-4555-8555-555555555555";
        let count = |input: u64| json!({"timestamp":at,"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":input,"output_tokens":0}}}});
        let meta = |id: &str, extra: Value| {
            let mut payload = json!({"id":id,"timestamp":at,"cwd":"/w/repo","source":"cli","originator":"codex_cli_rs"});
            for (key, value) in extra.as_object().unwrap() {
                payload[key] = value.clone();
            }
            json!({"timestamp":at,"type":"session_meta","payload":payload})
        };
        let model = json!({"timestamp":at,"type":"turn_context","payload":{"model":"gpt-5"}});
        fs::write(
            root.join(format!("rollout-a-{parent}.jsonl")),
            lines(&[meta(parent, json!({})), model.clone(), count(100)]),
        )
        .unwrap();
        let path = root.join(format!("rollout-b-{review}.jsonl"));
        fs::write(
            &path,
            lines(&[
                meta(
                    review,
                    json!({"source":{"subagent":{"other":"guardian"}},"parent_thread_id":parent}),
                ),
                model,
                count(20),
            ]),
        )
        .unwrap();
        let mut core = core(temp.path());
        core.request("snapshot", json!({})).unwrap();
        let page = core.request("sessions", json!({"scope": "all"})).unwrap();
        assert_eq!(page["total"], 1, "the review is not listed: {page:#}");
        assert_eq!(
            page["items"][0]["tokens"], 120,
            "but its tokens are its thread's"
        );
        let detail = core
            .request("session_detail", json!({"id": parent}))
            .unwrap();
        assert_eq!(
            detail["children"],
            json!([]),
            "nor named among its sub-agents"
        );
        assert_eq!(detail["subagents"]["total"], 20);

        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{}", count(50)).unwrap();
        core.request("refresh", json!({})).unwrap();
        let after = core.request("sessions", json!({"scope": "all"})).unwrap();
        assert_ne!(
            after["revision"], page["revision"],
            "the thread's row changed"
        );
        assert_eq!(after["items"][0]["tokens"], 150);
    }

    #[test]
    fn a_codex_rollout_reports_its_context_window_and_tools_before_each_count() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".codex/sessions/2026/10/21");
        fs::create_dir_all(&root).unwrap();
        let t = Utc::now().timestamp_millis() - 3_600_000;
        let count = |at: i64, input: u64, cached: u64, output: u64| json!({"timestamp":iso(at),"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":input,"cached_input_tokens":cached,"output_tokens":output},"model_context_window":258_400}}});
        fs::write(root.join("rollout-x.jsonl"), lines(&[
            json!({"timestamp":iso(t - 1_000),"type":"session_meta","payload":{"id":"thread-1","cwd":"/w","originator":"codex_cli_rs","timestamp":iso(t - 1_000)}}),
            json!({"timestamp":iso(t - 900),"type":"turn_context","payload":{"model":"gpt-5"}}),
            json!({"timestamp":iso(t),"type":"event_msg","payload":{"type":"task_started","turn_id":"a"}}),
            json!({"timestamp":iso(t + 100),"type":"response_item","payload":{"type":"function_call","name":"exec_command","call_id":"c1"}}),
            json!({"timestamp":iso(t + 150),"type":"response_item","payload":{"type":"local_shell_call","call_id":"c2"}}),
            count(t + 200, 1_000, 0, 50),
            // A count that does not move the totals closes nothing.
            count(t + 250, 1_000, 0, 50),
            json!({"timestamp":iso(t + 300),"type":"response_item","payload":{"type":"web_search_call"}}),
            count(t + 400, 3_000, 900, 80),
            json!({"timestamp":iso(t + 500),"type":"compacted","payload":{}}),
        ])).unwrap();
        let mut core = core(temp.path());
        core.request("snapshot", json!({})).unwrap();
        let detail = core
            .request("session_detail", json!({"id": "thread-1"}))
            .unwrap();
        assert_eq!(detail["turnCount"], 1, "{detail:#}");
        assert_eq!(
            detail["contextWindow"], 258_400,
            "the window the rollout reported"
        );
        assert_eq!(detail["contextTokens"], 2_000);
        assert!((detail["contextFill"].as_f64().unwrap() - 2_000.0 / 258_400.0).abs() < 1e-12);
        assert_eq!(detail["turns"][0]["compacted"], true);
        let turn = detail["turns"][0]["id"].as_str().unwrap().to_owned();
        let picked = core
            .request("session_detail", json!({"id": "thread-1", "turn": turn}))
            .unwrap();
        let tools = picked["turnCalls"]["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|call| call["tools"].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            tools,
            vec![json!(["exec_command", "shell"]), json!(["web_search"])]
        );
    }

    #[test]
    fn a_codex_turn_marks_only_the_users_words_as_prompts() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".codex/sessions/2026/10/21");
        fs::create_dir_all(&root).unwrap();
        let t = Utc::now().timestamp_millis() - 3_600_000;
        // Desktop rollouts name each part's role; older ones name none.
        let user = |at: i64, text: &str, kind: Option<&str>| {
            let mut payload = json!({"type":"message","role":"user","content":[{"type":"input_text","text":text}]});
            if let Some(kind) = kind {
                payload["internal_chat_message_metadata_passthrough"] =
                    json!({"content_item_kinds":[kind]});
            }
            json!({"timestamp":iso(at),"type":"response_item","payload":payload})
        };
        let reply = "<send_user_message_question_reply>[{\"question\":\"Which host?\",\"answer\":\"wsl\"}]</send_user_message_question_reply>";
        fs::write(root.join("rollout-x.jsonl"), lines(&[
            json!({"timestamp":iso(t - 1_000),"type":"session_meta","payload":{"id":"thread-1","cwd":"/w","originator":"codex_cli_rs","timestamp":iso(t - 1_000)}}),
            json!({"timestamp":iso(t),"type":"event_msg","payload":{"type":"task_started","turn_id":"a"}}),
            user(t + 10, "# AGENTS.md instructions for /w\n\n<INSTRUCTIONS>\nBe brief.\n</INSTRUCTIONS>", Some("agents_md.instructions")),
            user(t + 20, "<environment_context>\n  <cwd>/w</cwd>\n</environment_context>", None),
            user(t + 30, "<external_codex_apps_open_page>{\"page_id\":null}</external_codex_apps_open_page>", Some("additional_content.codex_apps_open_page")),
            user(t + 40, "Fix the build", Some("user.text")),
            user(t + 50, reply, Some("user.text")),
            user(t + 60, "<turn_aborted>\nThe user interrupted the previous turn on purpose.\n</turn_aborted>", Some("generic.turn_aborted")),
            user(t + 70, "An older rollout's words", None),
        ])).unwrap();
        let mut core = core(temp.path());
        core.request("snapshot", json!({})).unwrap();
        let detail = core
            .request(
                "session_detail",
                json!({"id": "thread-1", "messageTail": true}),
            )
            .unwrap();
        let users = detail["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "user")
            .collect::<Vec<_>>();
        assert_eq!(users.len(), 7, "{detail:#}");
        let prompts = users
            .iter()
            .filter(|m| m["isPrompt"] == true)
            .map(|m| m["text"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            prompts,
            vec!["Fix the build", reply, "An older rollout's words"]
        );
    }

    #[test]
    fn claudes_interruption_note_is_not_a_prompt() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".claude/projects/p");
        fs::create_dir_all(&root).unwrap();
        let t = Utc::now().timestamp_millis() - 3_600_000;
        let id = "11111111-2222-3333-4444-777777777777";
        let user = |at: i64, text: &str| json!({"type":"user","sessionId":id,"timestamp":iso(at),"uuid":format!("u{at}"),"message":{"role":"user","content":text}});
        fs::write(root.join(format!("{id}.jsonl")), lines(&[
            user(t, "Fix the build"),
            json!({"type":"assistant","sessionId":id,"timestamp":iso(t + 1_000),"message":{"id":"m1","model":"claude-opus-4-5","role":"assistant","content":[{"type":"text","text":"Looking."}],"usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}),
            user(t + 2_000, "[Request interrupted by user]"),
        ])).unwrap();
        let mut core = core(temp.path());
        core.request("snapshot", json!({})).unwrap();
        let detail = core
            .request("session_detail", json!({"id": id, "messageTail": true}))
            .unwrap();
        let users = detail["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "user")
            .collect::<Vec<_>>();
        assert_eq!(users.len(), 2, "{detail:#}");
        assert_eq!(users[0]["isPrompt"], true);
        assert_eq!(users[1]["isPrompt"], false);
        assert_eq!(
            users[1]["turnId"], users[0]["turnId"],
            "the note stays in the turn it stopped"
        );
    }

    #[test]
    fn a_claude_page_that_begins_inside_a_turn_keeps_that_turn() {
        // Claude Code names the turn only on the prompt line: replies, tool
        // calls and their results carry no prompt id.
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".claude/projects/p");
        fs::create_dir_all(&root).unwrap();
        let t = Utc::now().timestamp_millis() - 3_600_000;
        let session = |id: &str, prompt: &str, words: &[&str]| {
            let mut rows = vec![
                json!({"type":"user","sessionId":id,"timestamp":iso(t),"uuid":"line-0","promptId":prompt,"message":{"role":"user","content":"Fix the build"}}),
            ];
            for (n, said) in words.iter().enumerate() {
                let at = t + 1_000 * (n as i64 + 1);
                rows.push(json!({"type":"assistant","sessionId":id,"timestamp":iso(at),"uuid":format!("a{n}"),"message":{"id":format!("m{n}"),"model":"claude-opus-4-5","role":"assistant","content":[{"type":"text","text":said},{"type":"tool_use","id":format!("tu{n}"),"name":"Bash","input":{}}],"usage":{"input_tokens":10,"output_tokens":20}}}));
                rows.push(json!({"type":"user","sessionId":id,"timestamp":iso(at + 500),"uuid":format!("r{n}"),"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":format!("tu{n}"),"content":"ok"}]}}));
            }
            fs::write(root.join(format!("{id}.jsonl")), lines(&rows)).unwrap();
        };
        let a = "11111111-2222-3333-4444-aaaaaaaaaaaa";
        let b = "11111111-2222-3333-4444-bbbbbbbbbbbb";
        session(a, "prompt-a", &["One.", "Two.", "Three."]);
        session(b, "prompt-b", &["Other."]);
        let turns_of = |detail: &Value| {
            detail["messages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| (m["turnId"].clone(), m["turnStartedAt"].clone()))
                .collect::<Vec<_>>()
        };
        let mut core = core(temp.path());
        core.request("snapshot", json!({})).unwrap();
        let first = core
            .request("session_detail", json!({"id": a, "messageLimit": 2}))
            .unwrap();
        let cursor = first["nextMessageCursor"].as_u64().unwrap();
        // Another session's page between two pages of this one.
        core.request("session_detail", json!({"id": b, "messageTail": true}))
            .unwrap();
        let next = core
            .request("session_detail", json!({"id": a, "messageCursor": cursor}))
            .unwrap();
        let started = json!(iso(t));
        assert!(!turns_of(&next).is_empty());
        assert!(
            turns_of(&next)
                .iter()
                .all(|turn| *turn == (json!("prompt-a"), started.clone())),
            "the rest of the turn stays in it: {next:#}"
        );
        // A new engine, as after a launch, remembers nothing and looks back.
        let mut fresh = super::tests::core(temp.path());
        fresh.request("snapshot", json!({})).unwrap();
        let again = fresh
            .request("session_detail", json!({"id": a, "messageCursor": cursor}))
            .unwrap();
        assert_eq!(turns_of(&again), turns_of(&next), "{again:#}");
    }

    #[test]
    fn a_claude_turn_is_its_prompt_id_and_request_copies_are_skipped() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".claude/projects/p");
        fs::create_dir_all(&root).unwrap();
        let t = Utc::now().timestamp_millis() - 3_600_000;
        let id = "11111111-2222-3333-4444-888888888888";
        // Claude Code 2.1.299+ writes what it sent to the model as
        // api-request-blob lines: a user copy holds the context and the
        // prompt again, with no uuid, prompt id or time.
        let copy = |role: &str, content: Value| json!({"type":"api-request-blob","hash":"h","sessionId":id,"message":{"role":role,"content":content}});
        fs::write(root.join(format!("{id}.jsonl")), lines(&[
            json!({"type":"user","sessionId":id,"timestamp":iso(t),"uuid":"line-1","promptId":"prompt-1","message":{"role":"user","content":"Fix the build"}}),
            copy("user", json!([{"type":"text","text":"<system-reminder>context</system-reminder>"},{"type":"text","text":"Fix the build"}])),
            json!({"type":"assistant","sessionId":id,"timestamp":iso(t + 1_000),"uuid":"line-2","promptId":"prompt-1","message":{"id":"m1","model":"claude-opus-4-5","role":"assistant","content":[{"type":"text","text":"Looking."}],"usage":{"input_tokens":10,"output_tokens":20}}}),
            copy("assistant", json!([{"type":"text","text":"Looking."}])),
            json!({"type":"assistant","sessionId":id,"timestamp":iso(t + 2_000),"uuid":"line-3","promptId":"prompt-1","message":{"id":"m2","model":"claude-opus-4-5","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"Done."}],"usage":{"input_tokens":10,"output_tokens":20}}}),
        ])).unwrap();
        let mut core = core(temp.path());
        let snapshot = core.request("snapshot", json!({})).unwrap();
        let session = snapshot["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|session| session["id"] == id)
            .unwrap();
        assert_eq!(
            session["liveFacts"]["turnId"], "prompt-1",
            "the turn has the name Claude Code's hooks report as prompt_id"
        );
        let detail = core
            .request("session_detail", json!({"id": id, "messageTail": true}))
            .unwrap();
        let rows = detail["messages"].as_array().unwrap();
        let said = rows
            .iter()
            .map(|row| (row["role"].as_str().unwrap(), row["text"].as_str().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(
            said,
            vec![
                ("user", "Fix the build"),
                ("assistant", "Looking."),
                ("assistant", "Done.")
            ],
            "a request copy is neither a prompt nor a message: {detail:#}"
        );
        assert!(
            rows.iter().all(|row| row["turnId"] == "prompt-1"),
            "the copy of the prompt does not end the turn: {detail:#}"
        );
    }

    #[test]
    fn a_ledger_written_before_parser_6_gains_marks_and_tools_on_the_read_again() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".claude/projects/p");
        fs::create_dir_all(&root).unwrap();
        let t = Utc::now().timestamp_millis() - 3_600_000;
        let id = "11111111-2222-3333-4444-666666666666";
        fs::write(root.join(format!("{id}.jsonl")), lines(&[
            json!({"type":"user","sessionId":id,"timestamp":iso(t),"message":{"role":"user","content":"Go"}}),
            json!({"type":"assistant","sessionId":id,"timestamp":iso(t + 500),"message":{"id":"m1","model":"claude-opus-4-5","content":[{"type":"tool_use","id":"x","name":"Bash","input":{}}],"usage":{"input_tokens":10,"output_tokens":5}}}),
        ])).unwrap();
        core(temp.path()).request("snapshot", json!({})).unwrap();
        // What parser 5 left: calls without tools, no marks.
        let db = Connection::open(temp.path().join("app/ingestion.sqlite")).unwrap();
        db.execute_batch(
            "DELETE FROM usage_meter_mark; UPDATE usage_meter_event SET tools=NULL;
             UPDATE ingestion_logs SET head=json_set(head,'$.parser_version',5), payload=json_set(payload,'$.parser_version',5);",
        )
        .unwrap();
        drop(db);
        let mut core = core(temp.path());
        core.request("snapshot", json!({})).unwrap();
        let detail = core.request("session_detail", json!({"id": id})).unwrap();
        assert_eq!(detail["turnCount"], 1, "{detail:#}");
        assert_eq!(detail["calls"][0]["tools"], json!(["Bash"]));
    }

    #[test]
    fn workbuddy_credits_show_two_decimals_and_the_unattributed_part() {
        let temp = tempfile::tempdir().unwrap();
        let mut core = core(temp.path());
        core.workbuddy_metering.credits.insert(
            "workbuddy:s".into(),
            agent_hud_agents::buddy::transcript::SessionCredits {
                total: 12.344999,
                unattributed: 0.5,
                by_model: Default::default(),
                used: 40_000,
                size: 200_000,
            },
        );
        let credits = core.session_credits("workbuddy:s");
        assert_eq!(credits["text"], "12.34");
        assert_eq!(credits["unattributedText"], "0.50");
        assert_eq!(
            (credits["used"].as_u64(), credits["size"].as_u64()),
            (Some(40_000), Some(200_000))
        );
        core.workbuddy_metering
            .credits
            .get_mut("workbuddy:s")
            .unwrap()
            .unattributed = 0.004;
        assert_eq!(
            core.session_credits("workbuddy:s")["unattributedText"],
            Value::Null,
            "noise is attributed"
        );
        assert_eq!(core.session_credits("workbuddy:none"), Value::Null);
    }
}
