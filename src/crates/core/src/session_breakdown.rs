//! A session's tokens, its money and its breakdown on the session page: its
//! own log and the sub-agent logs under it, the breakdown read from the usage
//! ledger by query.
use super::*;
use crate::session_usage::{self, Builder, Call};
use std::hash::{Hash, Hasher};

mod family;

pub(crate) use family::Family;

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
            let meta = log.path.with_extension("meta.json");
            let meta = crate::demo::file(&meta)
                .or_else(|| fs::read(&meta).ok())
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
            builder.add(Call {
                row,
                own: row.contribution == own_paths[0],
            });
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
        let mut rows = self.session_rows.borrow_mut();
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
                .map(|row| Call {
                    row,
                    own: row.contribution == own_paths[0],
                })
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
mod tests;
