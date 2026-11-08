//! Where one session's tokens went, as the usage ledger recorded them: by
//! model, by 15-minute period, by turn and the part of each turn its
//! sub-agents spent (agent-hud-native-open `SessionUsage` and `SessionUsageBuilder`).
//! It is built on demand from the ledger's calls and marks, never from the
//! logs themselves.
use crate::{metering, pricing, Usage};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

mod builder;
mod json;
mod turn_calls;

pub(crate) use json::tokens_json;
pub(crate) use turn_calls::{decimal_text, turn_calls, turn_summary};

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

/// The previous call's total prompt and the part it read from cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CachePrompt {
    pub total: u64,
    pub read: u64,
}

impl From<Usage> for CachePrompt {
    fn from(usage: Usage) -> Self {
        Self {
            total: prompt(usage),
            read: usage.read,
        }
    }
}

/// Uncached input on a call that read fewer cached tokens than the preceding
/// call, read less than half its prompt, and sent at least half that size uncached.
pub(crate) fn recached_input(sent: u64, read: u64, previous: Option<CachePrompt>) -> u64 {
    match previous {
        Some(previous)
            if read < previous.read
                && u128::from(read) * 2 < u128::from(previous.total)
                && u128::from(sent) * 2 >= u128::from(previous.total) =>
        {
            sent
        }
        _ => 0,
    }
}

pub(crate) fn recached(usage: Usage, previous: Option<CachePrompt>) -> u64 {
    recached_input(usage.input + usage.write, usage.read, previous)
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
    previous_prompt: Option<CachePrompt>,
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

fn usd(pico: Option<i128>) -> Value {
    json!(pico.map(|pico| pico as f64 / 1e12))
}

#[cfg(test)]
mod tests;
