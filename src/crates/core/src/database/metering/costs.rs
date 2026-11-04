//! What a log's calls cost: at the list prices, at an account's own prices.

use super::{unsigned, usage};
use crate::{pricing, Usage};
use rusqlite::{params, Connection};

/// What a log's calls would cost at the vendors' API list prices: what the
/// calls with a list price cost, and how many tokens the others used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct ListCost {
    /// The priced calls' cost in pico US dollars; None when no call has a price.
    pub priced: Option<i128>,
    /// The tokens of the calls with no list price.
    pub unpriced_tokens: u64,
}

impl ListCost {
    /// The whole cost; None when a call has no list price or there was none.
    pub fn complete(&self) -> Option<i128> {
        self.priced.filter(|_| self.unpriced_tokens == 0)
    }
}

/// What a log's calls would cost at the vendors' API list prices: the
/// international list at off-peak rates, in pico US dollars, each call on the
/// tier its prompt reaches (agent-hud-native-open `ListPricing`). A call with
/// no list price leaves the others priced; None when the log has no calls.
pub(crate) fn list_cost(db: &Connection, path: &str) -> rusqlite::Result<Option<ListCost>> {
    let models = {
        let mut statement = db.prepare_cached(
            "SELECT DISTINCT source, model FROM usage_meter_event WHERE contribution=?1",
        )?;
        let rows = statement.query_map(params![path], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if models.is_empty() {
        return Ok(None);
    }
    let mut cost = ListCost::default();
    for (source, model) in models {
        let Some(starts) = pricing::tier_starts(&source, &model) else {
            let tokens: i64 = db
                .prepare_cached(
                    "SELECT COALESCE(SUM(input+output+cache_read+cache_write+reasoning),0)
                     FROM usage_meter_event WHERE contribution=?1 AND source=?2 AND model=?3",
                )?
                .query_row(params![path, source, model], |row| row.get(0))?;
            cost.unpriced_tokens += tokens as u64;
            continue;
        };
        // The tier a call is on: the last start its prompt reaches, -1 for the base rates.
        let tier = starts
            .iter()
            .enumerate()
            .rev()
            .map(|(index, from)| format!("WHEN input+cache_write+cache_read>={from} THEN {index} "))
            .collect::<String>();
        let tier = if tier.is_empty() {
            "-1".to_owned()
        } else {
            format!("CASE {tier}ELSE -1 END")
        };
        let mut statement = db.prepare(&format!(
            "SELECT {tier} AS tier, SUM(input), SUM(output), SUM(cache_read), SUM(cache_write), SUM(reasoning)
             FROM usage_meter_event WHERE contribution=?1 AND source=?2 AND model=?3 GROUP BY tier"
        ))?;
        let mut rows = statement.query(params![path, source, model])?;
        while let Some(row) = rows.next()? {
            let tier = usize::try_from(row.get::<_, i64>(0)?).ok();
            let used = usage(row, 1)?;
            let tokens = price_tokens(used);
            // Session costs are US dollars; a relay that prices in another
            // currency leaves its calls unpriced there.
            match pricing::cost_in_tier(&source, &model, &tokens, tier)
                .filter(|priced| priced.currency == "USD")
            {
                Some(priced) => {
                    cost.priced = Some(cost.priced.unwrap_or(0) + priced.amount_pico);
                }
                None => cost.unpriced_tokens += used.total(),
            }
        }
    }
    Ok(Some(cost))
}

/// The largest prompt `model`, used through `source`, was seen with.
pub(crate) fn largest_context(
    db: &Connection,
    source: &str,
    model: &str,
) -> rusqlite::Result<Option<u64>> {
    let mut statement = db.prepare_cached(
        "SELECT largest FROM usage_meter_model_context WHERE source=?1 AND model=?2",
    )?;
    let mut rows = statement.query(params![source, model])?;
    rows.next()?.map(|row| unsigned(row, 0)).transpose()
}

/// What one call would cost at the international list price, off-peak, on
/// the tier its prompt reaches, in pico US dollars (agent-hud-native-open
/// `ModelCatalog.cost(agentId:kinds:)`).
pub(crate) fn call_list_cost(source: &str, model: &str, used: Usage) -> Option<i128> {
    let starts = pricing::tier_starts(source, model)?;
    let tier = pricing::tier_of(&starts, used.input + used.write + used.read);
    pricing::cost_in_tier(source, model, &price_tokens(used), tier)
        .filter(|cost| cost.currency == "USD")
        .map(|cost| cost.amount_pico)
}

/// A ledger row's tokens as the price list counts them: input with cache
/// writes, output with reasoning.
pub(crate) fn price_tokens(used: Usage) -> pricing::PriceTokens {
    pricing::PriceTokens {
        tokens_in: used.input + used.write,
        tokens_out: used.output + used.reasoning,
        cache_read: used.read,
        cache_write: used.write,
        reasoning: used.reasoning,
    }
}

/// What a DeepSeek log's calls cost at the account's own prices
/// (agent-hud-native-open `UsageLedger.contributionCosts`): each call at its tier and
/// at DeepSeek's peak rates when made in Beijing working hours, in pico US
/// dollars on the international list and pico yuan on the China list. A
/// currency is None when a call has no price in it; None for a log of
/// another client or without calls.
pub(crate) fn account_cost(
    db: &Connection,
    path: &str,
    source: &str,
) -> rusqlite::Result<Option<[Option<i128>; 2]>> {
    if source != "deepseek" {
        return Ok(None);
    }
    let mut statement = db.prepare_cached(
        "SELECT model,at_ms,input,output,cache_read,cache_write,reasoning FROM usage_meter_event WHERE contribution=?1",
    )?;
    let mut rows = statement.query(params![path])?;
    let mut totals = [Some(0i128), Some(0i128)];
    let mut any = false;
    while let Some(row) = rows.next()? {
        any = true;
        let model: String = row.get(0)?;
        let at: i64 = row.get(1)?;
        let tokens = price_tokens(usage(row, 2)?);
        for (total, region) in totals
            .iter_mut()
            .zip([pricing::Region::International, pricing::Region::China])
        {
            let cost = pricing::cost_at(source, &model, &tokens, true, region, Some(at))
                .filter(|cost| cost.currency == region.currency());
            *total = total
                .zip(cost)
                .map(|(total, cost)| total + cost.amount_pico);
        }
    }
    Ok(any.then_some(totals))
}
