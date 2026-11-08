//! One turn in full: its summary, its calls, and its cost as exact decimals.
use super::{new_tokens, prompt, recached, usd, CachePrompt, Call, SessionUsage};
use crate::{iso, metering, Usage};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// What a picked turn's calls add up to (agent-hud-native-open's line above them):
/// how many, how many sub-agents made and from how many logs, the time from
/// the first to the last, their list price when every call has one, and the
/// call that added the most with its share of what the calls added (only
/// with more than one call).
pub(crate) fn turn_summary(calls: &[Call<'_>]) -> Value {
    let by_subagents = calls.iter().filter(|call| !call.own).collect::<Vec<_>>();
    let logs = by_subagents
        .iter()
        .map(|call| call.row.contribution.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();
    let span = match (calls.first(), calls.last()) {
        (Some(first), Some(last)) => (last.row.at - first.row.at).max(0),
        _ => 0,
    };
    let price = calls
        .iter()
        .map(|call| metering::call_list_cost(&call.row.source, &call.row.model, call.row.usage))
        .collect::<Option<Vec<_>>>()
        .filter(|prices| !prices.is_empty())
        .map(|prices| prices.into_iter().sum::<i128>());
    let added = calls
        .iter()
        .map(|call| new_tokens(call.row.usage))
        .sum::<u64>();
    let largest = calls
        .iter()
        .map(|call| new_tokens(call.row.usage))
        .max()
        .unwrap_or(0);
    let several = calls.len() > 1 && added > 0;
    json!({
        "calls": calls.len(),
        "subagentCalls": by_subagents.len(),
        "subagentLogs": logs,
        "spanMs": span,
        "listCost": usd(price),
        "added": added,
        "largestNew": several.then_some(largest),
        "largestShare": several.then(|| largest as f64 / added as f64),
    })
}

impl SessionUsage {
    /// agent-hud-native-open's `SessionUsage`, for a host that publishes it: every
    /// model and each model's periods by agent id (`source-model:model`), the
    /// sub-agents' part and the kept turns, with times in milliseconds and
    /// money as exact decimal strings. Tokens are Open's: `tokensIn` with cache
    /// writes, `tokensOut` with reasoning. `costs` is the session at its
    /// account's own prices, by currency, in pico units.
    pub(crate) fn open_json(&self, costs: &BTreeMap<String, i128>) -> Value {
        let agent = |(source, model): &(String, String)| format!("{source}-model:{model}");
        let mut models = self
            .models
            .iter()
            .map(|(model, usage)| (agent(model), *usage))
            .collect::<Vec<_>>();
        // Open sorts by tokens in and out, the most first, then by id.
        let size = |usage: &Usage| usage.input + usage.write + usage.output + usage.reasoning;
        models.sort_by(|(a_id, a), (b_id, b)| size(b).cmp(&size(a)).then_with(|| a_id.cmp(b_id)));
        let mut periods = self
            .model_periods
            .iter()
            .map(|(start, model, usage)| (*start, agent(model), *usage))
            .collect::<Vec<_>>();
        periods.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        let costs = (!costs.is_empty()).then(|| {
            costs
                .iter()
                .map(|(currency, pico)| (currency.clone(), json!(decimal_text(*pico))))
                .collect::<serde_json::Map<_, _>>()
        });
        json!({
            "models": models.into_iter()
                .map(|(id, usage)| json!({"agentId": id, "tokens": open_tokens(usage)}))
                .collect::<Vec<_>>(),
            "periods": periods.into_iter()
                .map(|(start, id, usage)| json!({"startMs": start, "agentId": id, "tokens": open_tokens(usage)}))
                .collect::<Vec<_>>(),
            "subagents": self.subagents.map(open_tokens),
            "calls": self.calls,
            "contextTokens": self.context,
            "costs": costs,
            "turns": self.turns.iter().map(|turn| json!({
                "startMs": turn.start,
                "endMs": turn.end,
                "tokens": open_tokens(turn.tokens),
                "calls": turn.calls,
                "contextTokens": turn.context,
                "compacted": turn.compacted,
                "subagents": turn.subagents.map(open_tokens),
                "peakContextTokens": turn.peak_context,
                "recachedTokens": turn.recached,
                "listCost": turn.list_cost.complete().map(decimal_text),
            })).collect::<Vec<_>>(),
            "turnCount": self.turn_count,
            "contextWindow": self.context_window,
            "listCost": self.list_cost.complete().map(decimal_text),
        })
    }
}

/// Tokens as agent-hud-native-open's `SessionUsage.Tokens` counts them.
fn open_tokens(usage: Usage) -> Value {
    json!({
        "tokensIn": usage.input + usage.write,
        "tokensOut": usage.output + usage.reasoning,
        "cacheReadTokens": usage.read,
        "cacheWriteTokens": usage.write,
        "reasoningTokens": usage.reasoning,
    })
}

/// Pico units as an exact decimal, the way Foundation's `NSDecimalNumber`
/// writes one: no exponent, no trailing zeros.
pub(crate) fn decimal_text(pico: i128) -> String {
    const PICOS: u128 = 1_000_000_000_000;
    let sign = if pico < 0 { "-" } else { "" };
    let magnitude = pico.unsigned_abs();
    let (whole, fraction) = (magnitude / PICOS, magnitude % PICOS);
    if fraction == 0 {
        return format!("{sign}{whole}");
    }
    let fraction = format!("{fraction:012}");
    format!("{sign}{whole}.{}", fraction.trim_end_matches('0'))
}

/// One turn's calls, oldest first, as the session page lists them
/// (agent-hud-native-open `TurnCall`): the session's and its sub-agents', each with
/// the tools its log names, its context and re-cached tokens when it is in
/// the session's own log, and its list price. `previous` is the prompt of the
/// own log's call before the turn.
pub(crate) fn turn_calls(calls: &[Call<'_>], mut previous: Option<CachePrompt>) -> Vec<Value> {
    calls
        .iter()
        .map(|call| {
            let row = call.row;
            let (mut context, mut again) = (Value::Null, Value::Null);
            if call.own {
                let sent = recached(row.usage, previous);
                again = json!((sent > 0).then_some(sent));
                context = json!(prompt(row.usage));
                previous = Some(row.usage.into());
            }
            let id = format!("{}-model:{}", row.source, row.model);
            json!({
                "id": row.key,
                "timestamp": iso(row.at),
                "model": row.model,
                "modelName": (!row.model.is_empty()).then(|| crate::naming::model_names::consumer_name(&id)),
                "source": row.source,
                "log": row.contribution,
                "own": call.own,
                "subagent": !call.own,
                "input": row.usage.input,
                "output": row.usage.output,
                "cacheRead": row.usage.read,
                "cacheWrite": row.usage.write,
                "reasoning": row.usage.reasoning,
                "total": row.usage.total(),
                "newTokens": new_tokens(row.usage),
                "context": context,
                "recached": again,
                "listCost": usd(metering::call_list_cost(&row.source, &row.model, row.usage)),
                "tools": row.tools,
            })
        })
        .collect()
}
