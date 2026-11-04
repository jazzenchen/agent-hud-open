//! A session's usage as the session views read it: its entries, its JSON and
//! its list row.
use super::{kind_lists, new_tokens, prompt, usd, SessionUsage, Turn, PERIOD_MS, ROW_BARS};
use crate::{iso, pricing, Usage};
use serde_json::{json, Value};

/// A token count as the session page reads it.
pub(crate) fn tokens_json(usage: Usage) -> Value {
    json!({"input":usage.input,"output":usage.output,"cacheRead":usage.read,"cacheWrite":usage.write,"reasoning":usage.reasoning,"total":usage.total()})
}

/// A count in the three parts the page's chart stacks: fresh input, output
/// with reasoning, cache reads and writes.
fn chart_parts(tokens: Usage) -> (u64, u64, u64) {
    (
        tokens.input,
        tokens.output + tokens.reasoning,
        tokens.read + tokens.write,
    )
}

/// One chart entry: a turn or a 15-minute period, its tokens in the chart's
/// parts (`chart_parts`) and by kind.
fn entry(id: String, index: usize, start: i64, tokens: Usage) -> Value {
    let (input, output, cache) = chart_parts(tokens);
    json!({
        "id": id,
        "index": index,
        "timestamp": iso(start),
        "input": input,
        "output": output,
        "cache": cache,
        "cacheRead": tokens.read,
        "cacheWrite": tokens.write,
        "reasoning": tokens.reasoning,
        "total": tokens.total(),
        "kinds": tokens_json(tokens),
        "own": tokens_json(tokens),
        "newTokens": new_tokens(tokens),
        "context": null,
        "message": null,
    })
}

impl SessionUsage {
    /// How full the context of the latest call was, 0…1.
    pub(crate) fn context_fill(&self) -> Option<f64> {
        let (context, window) = (self.context?, self.context_window?);
        (window > 0).then(|| context as f64 / window as f64)
    }

    /// Every call's tokens, sub-agents' included.
    pub(crate) fn total(&self) -> Usage {
        self.models
            .iter()
            .fold(Usage::default(), |mut sum, (_, usage)| {
                sum.add(*usage);
                sum
            })
    }

    /// The share of prompt tokens served from the cache; None before any
    /// prompt was sent.
    pub(crate) fn cache_hit_rate(&self) -> Option<f64> {
        let total = self.total();
        let prompt = prompt(total);
        (prompt > 0).then(|| total.read as f64 / prompt as f64)
    }

    /// The id a kept turn goes by on the page.
    pub(crate) fn turn_id(turn: &Turn) -> String {
        format!("t{}", turn.start)
    }

    /// The chart entries: one per turn when the log marks its prompts, else
    /// one per 15-minute period (as Open's chart falls back).
    pub(crate) fn entries(&self) -> (&'static str, Vec<Value>) {
        if self.turns.is_empty() {
            let entries = self
                .periods
                .iter()
                .enumerate()
                .map(|(index, (start, tokens, calls))| {
                    let mut value = entry(format!("p{start}"), index + 1, *start, *tokens);
                    value["end"] = iso(start + PERIOD_MS);
                    value["callCount"] = json!(calls);
                    value
                })
                .collect();
            return ("period", entries);
        }
        let first = self.turn_count - self.turns.len();
        let entries = self
            .turns
            .iter()
            .enumerate()
            .map(|(offset, turn)| {
                let mut value = entry(
                    Self::turn_id(turn),
                    first + offset + 1,
                    turn.start,
                    turn.tokens,
                );
                value["end"] = iso(turn.end);
                value["callCount"] = json!(turn.calls);
                value["context"] = json!(turn.context);
                value["peakContext"] = json!(turn.peak_context);
                value["compacted"] = json!(turn.compacted);
                value["subagent"] = json!(turn.subagents.is_some());
                value["subagents"] = turn.subagents.map(tokens_json).unwrap_or(Value::Null);
                value["subagentNew"] = json!(turn.subagents.map(new_tokens));
                value["own"] = tokens_json(turn.own());
                // The same part in the chart's parts, which the chart fades
                // (agent-hud-native-open `SessionBarsChart.subagentOpacity`).
                value["subagentParts"] = turn
                    .subagents
                    .map(|tokens| {
                        let (input, output, cache) = chart_parts(tokens);
                        json!({"input": input, "output": output, "cache": cache})
                    })
                    .unwrap_or(Value::Null);
                value["recached"] = json!(turn.recached);
                value["listCost"] = usd(turn.list_cost.complete());
                value["pricedCost"] = usd(turn.list_cost.priced);
                value["unpricedTokens"] = json!(turn.list_cost.unpriced_tokens);
                value
            })
            .collect();
        ("prompt", entries)
    }

    /// The breakdown's fields of the session page.
    pub(crate) fn json(&self) -> Value {
        let (by, turns) = self.entries();
        json!({
            "turns": turns,
            "turnsBy": by,
            "turnCount": self.turn_count,
            "callCount": self.calls,
            "contextTokens": self.context,
            "contextWindow": self.context_window,
            "contextFill": self.context_fill(),
            "cacheHitRate": self.cache_hit_rate(),
            "listCost": usd(self.list_cost.complete()),
            "pricedCost": usd(self.list_cost.priced),
            "unpricedTokens": self.list_cost.unpriced_tokens,
            "models": self.models.iter().map(|((source, model), usage)| {
                let id = format!("{source}-model:{model}");
                let prompted = prompt(*usage);
                json!({"id": id, "provider": source, "model": model,
                       "name": crate::naming::model_names::consumer_name(&id), "tokens": tokens_json(*usage),
                       "cacheHitRate": (prompted > 0).then(|| usage.read as f64 / prompted as f64),
                       "priced": pricing::tier_starts(source, model).is_some()})
            }).collect::<Vec<_>>(),
        })
    }

    /// What a session's row shows of its breakdown (agent-hud-native-open
    /// `SessionCard` and `TurnSparkline`): how many turns, the newest
    /// `ROW_BARS` turns' new tokens by kind with the sub-agents' part apart,
    /// which of them were compacted (positions among the bars), and how full
    /// the context is. A log that marks no prompts shows its newest 15-minute
    /// periods instead.
    pub(crate) fn row(&self) -> Value {
        let (by, own, subagents, compacted) = if self.turns.is_empty() {
            let periods = &self.periods[self.periods.len().saturating_sub(ROW_BARS)..];
            (
                "period",
                periods
                    .iter()
                    .map(|(_, tokens, _)| *tokens)
                    .collect::<Vec<_>>(),
                Vec::new(),
                Vec::new(),
            )
        } else {
            let turns = &self.turns[self.turns.len().saturating_sub(ROW_BARS)..];
            (
                "prompt",
                turns.iter().map(Turn::own).collect(),
                turns
                    .iter()
                    .map(|turn| turn.subagents.unwrap_or_default())
                    .collect(),
                turns
                    .iter()
                    .enumerate()
                    .filter(|(_, turn)| turn.compacted)
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>(),
            )
        };
        json!({
            "turnCount": self.turn_count,
            "contextTokens": self.context,
            "contextFill": self.context_fill(),
            "turnBars": {
                "by": by,
                "count": own.len(),
                "own": kind_lists(&own),
                "subagents": kind_lists(&subagents),
                "compacted": compacted,
            },
        })
    }
}
