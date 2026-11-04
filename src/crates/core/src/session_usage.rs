//! Where one session's tokens went, as the usage ledger recorded them: by
//! model, by 15-minute period, by turn and the part of each turn its
//! sub-agents spent (agent-hud-native-open `SessionUsage` and `SessionUsageBuilder`).
//! It is built on demand from the ledger's calls and marks, never from the
//! logs themselves.
use crate::{iso, metering, pricing, Usage};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

/// The newest turns a breakdown keeps; `turn_count` still counts the older ones.
pub(crate) const TURN_LIMIT: usize = 200;
const PERIOD_MS: i64 = 900_000;

/// A call's prompt: fresh input, cache writes and cache reads.
pub(crate) fn prompt(usage: Usage) -> u64 {
    usage.input + usage.write + usage.read
}

/// What calls added (agent-hud-native-open `TokenKinds.new`): everything but cache
/// reads, which are the context read back.
pub(crate) fn new_tokens(usage: Usage) -> u64 {
    usage.total() - usage.read
}

/// The turns a session row draws (agent-hud-native-open `TurnSparkline.limit`).
pub(crate) const ROW_BARS: usize = 64;

/// The kinds a turn adds, as the charts stack them from the base up
/// (agent-hud-native-open `stackedKinds`), with their names on the page.
const NEW_KINDS: [(&str, fn(Usage) -> u64); 4] = [
    ("cacheWrite", |usage| usage.write),
    ("input", |usage| usage.input),
    ("reasoning", |usage| usage.reasoning),
    ("output", |usage| usage.output),
];

/// `bars`' new tokens as one list per kind present (a kind no bar added is
/// left out); None when no bar added anything.
fn kind_lists(bars: &[Usage]) -> Option<Value> {
    let mut lists = serde_json::Map::new();
    for (name, kind) in NEW_KINDS {
        let values = bars.iter().map(|usage| kind(*usage)).collect::<Vec<_>>();
        if values.iter().any(|value| *value > 0) {
            lists.insert(name.into(), json!(values));
        }
    }
    (!lists.is_empty()).then_some(Value::Object(lists))
}

/// What a call in the session's own log sent again after `previous`, the
/// prompt of the call before it: its whole input (fresh and cache writes) when
/// it read back less than half of that prompt from the cache and sent at least
/// half of it again. The cache had lapsed or the model changed; a compaction
/// shrinks the prompt instead, so it re-caches nothing.
pub(crate) fn recached(usage: Usage, previous: Option<u64>) -> u64 {
    let sent = usage.input + usage.write;
    match previous {
        Some(previous) if usage.read * 2 < previous && sent * 2 >= previous => sent,
        _ => 0,
    }
}

/// Calls priced together per model and tier at the international list, off
/// peak, in pico US dollars (agent-hud-native-open `ListPricing.Tally`); within one
/// tier the price is linear, so the sum costs what the calls cost one by one.
#[derive(Clone, Default)]
pub(crate) struct ListTally {
    groups: HashMap<(String, String, Option<usize>), Usage>,
    unpriced_tokens: u64,
}

impl ListTally {
    pub(crate) fn add(&mut self, source: &str, model: &str, usage: Usage) {
        let Some(starts) = pricing::tier_starts(source, model) else {
            self.unpriced_tokens += usage.total();
            return;
        };
        let tier = pricing::tier_of(&starts, prompt(usage));
        self.groups
            .entry((source.to_owned(), model.to_owned(), tier))
            .or_default()
            .add(usage);
    }

    /// What the calls with a list price cost, and the tokens of the others.
    pub(crate) fn cost(&self) -> metering::ListCost {
        let mut cost = metering::ListCost {
            priced: None,
            unpriced_tokens: self.unpriced_tokens,
        };
        for ((source, model, tier), usage) in &self.groups {
            match pricing::cost_in_tier(source, model, &metering::price_tokens(*usage), *tier)
                .filter(|priced| priced.currency == "USD")
            {
                Some(priced) => cost.priced = Some(cost.priced.unwrap_or(0) + priced.amount_pico),
                None => cost.unpriced_tokens += usage.total(),
            }
        }
        cost
    }
}

/// One call as the builder takes it.
pub(crate) struct Call<'a> {
    pub row: &'a metering::CallRow,
    /// The session's own log rather than a sub-agent's. Only it marks the
    /// session's turns (a sub-agent's prompts are steps of the turn that
    /// started it), and it records every call, so its prompt is the session's
    /// context.
    pub own: bool,
}

#[derive(Clone)]
struct TurnSums {
    start: i64,
    end: i64,
    tokens: Usage,
    calls: u64,
    subagents: Option<Usage>,
    context: Option<u64>,
    peak: u64,
    recached: u64,
    tally: ListTally,
}

#[derive(Clone, Default)]
struct PeriodSums {
    tokens: Usage,
    calls: u64,
}

/// Folds one session's calls, in time order, into its breakdown. Calls go to
/// the turn of the latest prompt at or before them, sub-agents' calls
/// included; calls before the first prompt form a turn of their own.
pub(crate) struct Builder {
    prompts: Vec<i64>,
    compactions: Vec<i64>,
    models: BTreeMap<(String, String), Usage>,
    periods: BTreeMap<i64, BTreeMap<(String, String), PeriodSums>>,
    subagents: Option<Usage>,
    calls: u64,
    context: Option<u64>,
    window: Option<u64>,
    latest: Option<(String, String)>,
    tally: ListTally,
    previous_prompt: Option<u64>,
    turns: BTreeMap<i64, TurnSums>,
    prompt_index: i64,
}

/// One kept turn.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Turn {
    pub start: i64,
    pub end: i64,
    pub tokens: Usage,
    pub calls: u64,
    /// The prompt of the turn's last call in the session's own log.
    pub context: Option<u64>,
    /// The client compacted the conversation during the turn.
    pub compacted: bool,
    /// What sub-agents spent in the turn; already part of `tokens`.
    pub subagents: Option<Usage>,
    /// The largest prompt of the turn's own calls, when above `context`.
    pub peak_context: Option<u64>,
    /// Prompt tokens the own log sent again because the cache lapsed.
    pub recached: Option<u64>,
    /// At list prices: what the priced calls cost and the others' tokens.
    pub list_cost: metering::ListCost,
}

impl Turn {
    /// The session's own part of the turn: its tokens without the sub-agents'.
    pub(crate) fn own(&self) -> Usage {
        let part = self.subagents.unwrap_or_default();
        Usage {
            input: self.tokens.input.saturating_sub(part.input),
            output: self.tokens.output.saturating_sub(part.output),
            read: self.tokens.read.saturating_sub(part.read),
            write: self.tokens.write.saturating_sub(part.write),
            reasoning: self.tokens.reasoning.saturating_sub(part.reasoning),
        }
    }
}

/// One session's breakdown.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SessionUsage {
    /// Per (source, model), the most input and output first.
    pub models: Vec<((String, String), Usage)>,
    /// Per 15-minute period start, oldest first, summed over models.
    pub periods: Vec<(i64, Usage, u64)>,
    /// Each model's 15-minute periods with tokens, oldest first.
    pub model_periods: Vec<(i64, (String, String), Usage)>,
    /// What sub-agents spent; already part of `models` and `periods`.
    pub subagents: Option<Usage>,
    pub calls: u64,
    pub context: Option<u64>,
    /// The newest `TURN_LIMIT` turns with calls, oldest first.
    pub turns: Vec<Turn>,
    pub turn_count: usize,
    pub context_window: Option<u64>,
    /// At list prices, sub-agents included: what the priced calls cost and
    /// the others' tokens.
    pub list_cost: metering::ListCost,
}

impl Builder {
    /// `prompts` and `compactions`: the session's own marks.
    pub(crate) fn new(mut prompts: Vec<i64>, compactions: Vec<i64>) -> Self {
        prompts.sort_unstable();
        Self {
            prompts,
            compactions,
            models: BTreeMap::new(),
            periods: BTreeMap::new(),
            subagents: None,
            calls: 0,
            context: None,
            window: None,
            latest: None,
            tally: ListTally::default(),
            previous_prompt: None,
            turns: BTreeMap::new(),
            prompt_index: -1,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.calls == 0
    }

    /// Calls must arrive oldest first.
    pub(crate) fn add(&mut self, call: Call<'_>) {
        let row = call.row;
        let tokens = row.usage;
        let model = (row.source.clone(), row.model.clone());
        self.calls += 1;
        self.models.entry(model.clone()).or_default().add(tokens);
        let period = self
            .periods
            .entry(row.at.div_euclid(PERIOD_MS) * PERIOD_MS)
            .or_default()
            .entry(model.clone())
            .or_default();
        period.tokens.add(tokens);
        period.calls += 1;
        if !call.own {
            self.subagents
                .get_or_insert_with(Usage::default)
                .add(tokens);
        }
        let sent = prompt(tokens);
        let mut again = 0;
        if call.own {
            again = recached(tokens, self.previous_prompt);
            self.previous_prompt = Some(sent);
            self.context = Some(sent);
            if let Some(reported) = row.context_window {
                self.window = Some(reported);
            }
            self.latest = Some(model);
        }
        self.tally.add(&row.source, &row.model, tokens);
        if self.prompts.is_empty() {
            return;
        }
        while self.prompt_index + 1 < self.prompts.len() as i64
            && self.prompts[(self.prompt_index + 1) as usize] <= row.at
        {
            self.prompt_index += 1;
        }
        let start = if self.prompt_index >= 0 {
            self.prompts[self.prompt_index as usize]
        } else {
            row.at
        };
        let turn = self
            .turns
            .entry(self.prompt_index)
            .or_insert_with(|| TurnSums {
                start,
                end: row.at,
                tokens: Usage::default(),
                calls: 0,
                subagents: None,
                context: None,
                peak: 0,
                recached: 0,
                tally: ListTally::default(),
            });
        turn.end = turn.end.max(row.at);
        turn.tokens.add(tokens);
        turn.calls += 1;
        if !call.own {
            turn.subagents
                .get_or_insert_with(Usage::default)
                .add(tokens);
        }
        if call.own {
            turn.context = Some(sent);
            turn.peak = turn.peak.max(sent);
        }
        turn.recached += again;
        turn.tally.add(&row.source, &row.model, tokens);
    }

    /// `window` resolves the latest own model's context window from what the
    /// client reported (`pricing::context_window`).
    pub(crate) fn build(
        self,
        window: impl FnOnce(&str, &str, Option<u64>) -> Option<u64>,
    ) -> SessionUsage {
        let compacted = self
            .compactions
            .iter()
            .map(|at| {
                self.prompts
                    .iter()
                    .rposition(|prompt| prompt <= at)
                    .map_or(-1, |i| i as i64)
            })
            .collect::<std::collections::HashSet<_>>();
        let turn_count = self.turns.len();
        // Only the turns a breakdown keeps are priced.
        let turns = self
            .turns
            .iter()
            .skip(turn_count.saturating_sub(TURN_LIMIT))
            .map(|(index, turn)| Turn {
                start: turn.start,
                end: turn.end,
                tokens: turn.tokens,
                calls: turn.calls,
                context: turn.context,
                compacted: compacted.contains(index),
                subagents: turn.subagents,
                peak_context: turn
                    .context
                    .and_then(|context| (turn.peak > context).then_some(turn.peak)),
                recached: (turn.recached > 0).then_some(turn.recached),
                list_cost: turn.tally.cost(),
            })
            .collect();
        let mut models = self.models.into_iter().collect::<Vec<_>>();
        models.sort_by(|(a_id, a), (b_id, b)| {
            (b.input + b.write + b.output + b.reasoning)
                .cmp(&(a.input + a.write + a.output + a.reasoning))
                .then_with(|| a_id.cmp(b_id))
        });
        let model_periods = self
            .periods
            .iter()
            .flat_map(|(start, models)| {
                models
                    .iter()
                    .map(move |(model, sums)| (*start, model.clone(), sums.tokens))
            })
            .collect();
        let periods = self
            .periods
            .into_iter()
            .map(|(start, models)| {
                let mut tokens = Usage::default();
                let mut calls = 0;
                for sums in models.values() {
                    tokens.add(sums.tokens);
                    calls += sums.calls;
                }
                (start, tokens, calls)
            })
            .collect();
        let context_window = self
            .latest
            .as_ref()
            .and_then(|(source, model)| window(source, model, self.window));
        SessionUsage {
            models,
            periods,
            model_periods,
            subagents: self.subagents,
            calls: self.calls,
            context: self.context,
            turns,
            turn_count,
            context_window,
            list_cost: self.tally.cost(),
        }
    }
}

fn usd(pico: Option<i128>) -> Value {
    json!(pico.map(|pico| pico as f64 / 1e12))
}

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
pub(crate) fn turn_calls(calls: &[Call<'_>], mut previous: Option<u64>) -> Vec<Value> {
    calls
        .iter()
        .map(|call| {
            let row = call.row;
            let (mut context, mut again) = (Value::Null, Value::Null);
            if call.own {
                let sent = recached(row.usage, previous);
                again = json!((sent > 0).then_some(sent));
                context = json!(prompt(row.usage));
                previous = Some(prompt(row.usage));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_is_an_exact_decimal_without_exponent() {
        assert_eq!(decimal_text(0), "0");
        assert_eq!(decimal_text(1_500_000_000_000), "1.5");
        assert_eq!(decimal_text(120), "0.00000000012");
        assert_eq!(decimal_text(-2_000_000_000_000), "-2");
        assert_eq!(decimal_text(12_345_678_901_234_567), "12345.678901234567");
    }

    fn row(
        at: i64,
        model: &str,
        input: u64,
        write: u64,
        read: u64,
        output: u64,
    ) -> metering::CallRow {
        metering::CallRow {
            contribution: "/logs/s.jsonl".into(),
            key: at.to_string(),
            source: "claude".into(),
            model: model.into(),
            at,
            usage: Usage {
                input,
                output,
                read,
                write,
                reasoning: 0,
            },
            context_window: None,
            tools: Vec::new(),
        }
    }

    #[test]
    fn a_call_re_caches_when_it_reads_back_less_than_half_and_sends_half_again() {
        let usage = |input, write, read| Usage {
            input,
            write,
            read,
            ..Usage::default()
        };
        assert_eq!(
            recached(usage(10, 90, 0), Some(100)),
            100,
            "cache lapsed: everything sent again"
        );
        assert_eq!(
            recached(usage(10, 0, 90), Some(100)),
            0,
            "read back from the cache"
        );
        assert_eq!(
            recached(usage(5, 10, 0), Some(100)),
            0,
            "a compaction shrinks the prompt"
        );
        assert_eq!(
            recached(usage(10, 90, 0), None),
            0,
            "the first call has nothing to lose"
        );
        assert_eq!(
            recached(usage(20, 30, 49), Some(100)),
            50,
            "exactly half sent again"
        );
    }

    #[test]
    fn calls_fold_into_prompt_turns_with_context_peaks_compactions_and_sub_agents() {
        let calls = [
            row(500, "claude-opus-4-5", 1_000, 0, 0, 10), // before any prompt: a turn of its own
            row(1_100, "claude-opus-4-5", 10, 1_000, 1_000, 20),
            row(1_200, "claude-opus-4-5", 10, 100, 2_000, 30),
            row(1_300, "claude-opus-4-5", 10, 0, 1_000, 30), // compacted: the prompt shrank
            row(2_100, "claude-opus-4-5", 10, 1_200, 0, 40), // cache lapsed: re-cached
        ];
        let helper = row(1_150, "claude-haiku-4-5", 50, 0, 0, 5);
        let mut builder = Builder::new(vec![2_000, 1_000], vec![1_250]);
        for (index, call) in calls.iter().enumerate() {
            builder.add(Call {
                row: call,
                own: true,
            });
            if index == 1 {
                builder.add(Call {
                    row: &helper,
                    own: false,
                });
            }
        }
        let usage = builder.build(|source, model, reported| {
            pricing::context_window(source, model, reported, None)
        });
        assert_eq!(usage.turn_count, 3);
        assert_eq!(usage.calls, 6);
        let [before, first, second] = usage.turns.as_slice() else {
            panic!("three turns")
        };
        assert_eq!(
            (before.start, before.calls, before.compacted),
            (500, 1, false)
        );
        assert_eq!((first.start, first.end, first.calls), (1_000, 1_300, 4));
        assert_eq!(first.context, Some(1_010), "the turn's last own call");
        assert_eq!(
            first.peak_context,
            Some(2_110),
            "above the context the turn ended with"
        );
        assert!(first.compacted);
        assert_eq!(first.subagents.map(|usage| usage.input), Some(50));
        assert_eq!(first.recached, None);
        assert_eq!(
            second.recached,
            Some(1_210),
            "read back nothing of the 1,010 before it"
        );
        assert_eq!(second.peak_context, None);
        assert_eq!(usage.context, Some(1_210));
        let helped = usage
            .turns
            .iter()
            .filter_map(|turn| turn.subagents)
            .map(|usage| usage.total())
            .sum::<u64>();
        assert_eq!(
            helped, 55,
            "only the turn that started it holds the sub-agent's part"
        );
        assert_eq!(
            usage.context_window,
            Some(200_000),
            "Opus 4.5's published window"
        );
        assert_eq!(usage.models[0].0 .1, "claude-opus-4-5");
        assert!(usage.list_cost.complete().is_some_and(|cost| cost > 0));
        let turns = usage
            .turns
            .iter()
            .filter_map(|turn| turn.list_cost.complete())
            .sum::<i128>();
        assert_eq!(
            Some(turns),
            usage.list_cost.complete(),
            "turns cost what the session costs"
        );
        let page = usage.json();
        assert_eq!(page["turnsBy"], "prompt");
        assert_eq!(page["turns"][2]["index"], 3);
        assert_eq!(page["turns"][1]["callCount"], 4);
        assert_eq!(page["turns"][1]["subagents"]["input"], 50);
        assert_eq!(
            page["turns"][1]["subagentParts"],
            json!({"input": 50, "output": 5, "cache": 0}),
            "the chart fades this much"
        );
        assert_eq!(page["turns"][2]["subagentParts"], Value::Null);
        assert!((usage.context_fill().unwrap() - 1_210.0 / 200_000.0).abs() < 1e-12);
    }

    #[test]
    fn turns_count_what_they_added_and_a_row_draws_its_newest_turns_by_kind() {
        let mut helper = row(1_150, "claude-haiku-4-5", 50, 0, 7, 5);
        helper.usage.reasoning = 3;
        let calls = [
            row(1_100, "claude-opus-4-5", 10, 1_000, 1_000, 20),
            row(1_300, "claude-opus-4-5", 10, 0, 1_000, 30),
            row(2_100, "claude-opus-4-5", 5, 200, 900, 40),
        ];
        let mut builder = Builder::new(vec![1_000, 2_000], vec![1_250]);
        builder.add(Call {
            row: &calls[0],
            own: true,
        });
        builder.add(Call {
            row: &helper,
            own: false,
        });
        builder.add(Call {
            row: &calls[1],
            own: true,
        });
        builder.add(Call {
            row: &calls[2],
            own: true,
        });
        let usage = builder.build(|_, _, _| Some(10_000));
        let page = usage.json();
        let first = &page["turns"][0];
        // 20 input, 1,000 written, 50 output of its own; 50, 5 and 3 of the sub-agent's.
        assert_eq!(first["newTokens"], 1_128, "cache reads are not new");
        assert_eq!(first["subagentNew"], 58);
        assert_eq!(first["kinds"]["cacheRead"], 2_007);
        assert_eq!(
            first["own"],
            json!({"input":20,"output":50,"cacheRead":2_000,"cacheWrite":1_000,"reasoning":0,"total":3_070})
        );
        assert_eq!(page["turns"][1]["subagentNew"], Value::Null);

        let glance = usage.row();
        assert_eq!(glance["turnCount"], 2);
        assert_eq!(glance["contextTokens"], 1_105);
        assert!((glance["contextFill"].as_f64().unwrap() - 0.1105).abs() < 1e-12);
        let bars = &glance["turnBars"];
        assert_eq!(
            (bars["by"].as_str(), bars["count"].as_u64()),
            (Some("prompt"), Some(2))
        );
        assert_eq!(
            bars["own"],
            json!({"cacheWrite":[1_000,200],"input":[20,5],"output":[50,40]}),
            "a kind no turn added is left out"
        );
        assert_eq!(
            bars["subagents"],
            json!({"input":[50,0],"reasoning":[3,0],"output":[5,0]})
        );
        assert_eq!(bars["compacted"], json!([0]));

        // Past 64 turns a row keeps the newest; positions count from the first bar it keeps.
        let prompts = (0..70).map(|n| n * 1_000).collect::<Vec<_>>();
        let rows = (0..70)
            .map(|n| row(n * 1_000 + 10, "claude-opus-4-5", n as u64 + 1, 0, 0, 1))
            .collect::<Vec<_>>();
        let mut builder = Builder::new(prompts, vec![69_500]);
        for row in &rows {
            builder.add(Call { row, own: true });
        }
        let bars = builder.build(|_, _, _| None).row()["turnBars"].clone();
        assert_eq!(bars["count"], ROW_BARS);
        assert_eq!(bars["own"]["input"][0], 7, "turns 1 to 6 fall off");
        assert_eq!(bars["compacted"], json!([63]));
        assert_eq!(bars["subagents"], Value::Null);

        // A log that marks no prompts draws its periods.
        let mut builder = Builder::new(Vec::new(), Vec::new());
        builder.add(Call {
            row: &rows[0],
            own: true,
        });
        builder.add(Call {
            row: &rows[1],
            own: true,
        });
        let glance = builder.build(|_, _, _| None).row();
        assert_eq!(glance["turnCount"], 0);
        assert_eq!(glance["turnBars"]["by"], "period");
        assert_eq!(
            glance["turnBars"]["own"]["input"],
            json!([3]),
            "both calls fall in one quarter hour"
        );
    }

    #[test]
    fn a_turns_calls_sum_up_with_their_sub_agents_span_price_and_largest_call() {
        let own = row(1_000, "claude-opus-4-5", 100, 0, 0, 100);
        let large = row(4_000, "claude-opus-4-5", 500, 300, 9_000, 200);
        let mut helper = row(2_000, "claude-haiku-4-5", 40, 0, 0, 10);
        helper.contribution = "/logs/s/subagents/a.jsonl".into();
        let calls = [
            Call {
                row: &own,
                own: true,
            },
            Call {
                row: &helper,
                own: false,
            },
            Call {
                row: &large,
                own: true,
            },
        ];
        let summary = turn_summary(&calls);
        assert_eq!(summary["calls"], 3);
        assert_eq!(
            (
                summary["subagentCalls"].as_u64(),
                summary["subagentLogs"].as_u64()
            ),
            (Some(1), Some(1))
        );
        assert_eq!(summary["spanMs"], 3_000);
        assert_eq!(summary["added"], 1_250, "cache reads are not added");
        assert_eq!(summary["largestNew"], 1_000);
        assert!((summary["largestShare"].as_f64().unwrap() - 0.8).abs() < 1e-12);
        assert!(summary["listCost"].as_f64().is_some_and(|cost| cost > 0.0));
        let one = turn_summary(&calls[..1]);
        assert_eq!(
            (one["largestNew"].clone(), one["spanMs"].clone()),
            (Value::Null, json!(0)),
            "one call has no largest"
        );
        let unpriced = row(5_000, "no-such-model", 1, 0, 0, 1);
        let with_unpriced = [
            Call {
                row: &own,
                own: true,
            },
            Call {
                row: &unpriced,
                own: true,
            },
        ];
        assert_eq!(
            turn_summary(&with_unpriced)["listCost"],
            Value::Null,
            "no price unless every call has one"
        );
    }

    #[test]
    fn only_the_newest_turns_are_kept_and_a_log_without_prompts_falls_back_to_periods() {
        let prompts = (0..250).map(|n| n * 1_000).collect::<Vec<_>>();
        let rows = (0..250)
            .map(|n| row(n * 1_000 + 10, "claude-opus-4-5", 1, 0, 0, 1))
            .collect::<Vec<_>>();
        let mut builder = Builder::new(prompts, Vec::new());
        for row in &rows {
            builder.add(Call { row, own: true });
        }
        let usage = builder.build(|_, _, _| None);
        assert_eq!((usage.turns.len(), usage.turn_count), (TURN_LIMIT, 250));
        assert_eq!(usage.turns[0].start, 50_000);
        assert_eq!(usage.json()["turns"][0]["index"], 51);

        let mut builder = Builder::new(Vec::new(), Vec::new());
        for row in &rows[..3] {
            builder.add(Call { row, own: true });
        }
        let usage = builder.build(|_, _, _| None);
        assert_eq!(usage.turn_count, 0);
        let (by, entries) = usage.entries();
        assert_eq!((by, entries.len()), ("period", 1));
        assert_eq!(entries[0]["callCount"], 3);
    }

    #[test]
    fn a_model_without_a_list_price_leaves_the_whole_cost_unknown_and_the_rest_priced() {
        let priced = row(10, "claude-opus-4-5", 100, 0, 0, 10);
        let unpriced = row(20, "no-such-model", 100, 0, 0, 10);
        let mut builder = Builder::new(vec![0], Vec::new());
        builder.add(Call {
            row: &priced,
            own: true,
        });
        builder.add(Call {
            row: &unpriced,
            own: true,
        });
        let usage = builder.build(|_, _, _| None);
        assert_eq!(usage.list_cost.complete(), None);
        assert_eq!(usage.turns[0].list_cost.complete(), None);
        let alone = metering::call_list_cost("claude", "claude-opus-4-5", priced.usage);
        assert!(alone.is_some_and(|cost| cost > 0));
        assert_eq!(
            usage.list_cost.priced, alone,
            "the priced call keeps its cost"
        );
        assert_eq!(usage.list_cost.unpriced_tokens, unpriced.usage.total());
        assert_eq!(usage.turns[0].list_cost, usage.list_cost);
        let page = usage.json();
        assert_eq!(page["listCost"], Value::Null);
        assert_eq!(page["unpricedTokens"], unpriced.usage.total());
        assert!(page["pricedCost"].as_f64().is_some_and(|cost| cost > 0.0));
        let models = page["models"].as_array().unwrap();
        assert!(models
            .iter()
            .any(|model| model["model"] == "no-such-model" && model["priced"] == false));
        assert!(models
            .iter()
            .any(|model| model["model"] == "claude-opus-4-5" && model["priced"] == true));
        assert_eq!(
            usage.open_json(&BTreeMap::new())["listCost"],
            Value::Null,
            "the phone's format keeps a partly priced session unpriced"
        );
    }

    #[test]
    fn the_context_window_is_reported_else_published_and_claude_counts_as_1m_past_200k() {
        assert_eq!(
            pricing::context_window("codex", "gpt-5", Some(272_000), None),
            Some(272_000)
        );
        assert_eq!(
            pricing::context_window("claude", "claude-opus-4-5", None, Some(150_000)),
            Some(200_000)
        );
        assert_eq!(
            pricing::context_window("claude", "claude-opus-4-5", None, Some(250_000)),
            Some(1_000_000)
        );
        assert_eq!(
            pricing::context_window("claude", "claude-unlisted-9", None, None),
            Some(200_000),
            "any Claude model runs with 200K"
        );
        assert_eq!(
            pricing::context_window("pi", "no-such-model", None, None),
            None
        );
    }
}
