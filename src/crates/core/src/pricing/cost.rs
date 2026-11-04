//! What calls cost: at peak or off-peak rates, on the tier their prompt
//! reaches, by kind of token, and the context window a model has.
use super::overrides::route_cost;
use super::quote::{quote, scaled};
use super::*;

/// Beijing working hours, as in the source. Public holidays are not known to
/// this snapshot and therefore still count as peak when they fall on a weekday.
pub fn is_peak(at_ms: i64) -> bool {
    let Some(date) = chrono::Utc.timestamp_millis_opt(at_ms).single() else {
        return false;
    };
    let local = date.with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap());
    local.weekday().num_days_from_monday() < 5
        && ((9..12).contains(&local.hour()) || (14..18).contains(&local.hour()))
}

fn factor(model: &Model, at_ms: Option<i64>) -> i128 {
    if model.peak_hours && at_ms.is_some_and(is_peak) {
        2
    } else {
        1
    }
}

/// `tiered` is true for a single call, false for a pre-summed bucket whose prompt is unknown.
pub(crate) fn cost_at(
    source: &str,
    model: &str,
    tokens: &PriceTokens,
    tiered: bool,
    region: Region,
    at_ms: Option<i64>,
) -> Option<ModelCost> {
    consumer_cost(
        &format!("{source}-model:{model}"),
        tokens,
        tiered,
        region,
        at_ms,
    )
}

/// The prompt sizes from which `model`, used through `source`, is priced on
/// each higher tier of the international list; None when it has no list price.
/// A call is in the last tier whose start its prompt reaches.
pub(crate) fn tier_starts(source: &str, model: &str) -> Option<Vec<u64>> {
    match quote(&format!("{source}-model:{model}"), Region::International)?.1 {
        Quote::Flat(..) => Some(Vec::new()),
        Quote::Listed(listing, _) => {
            Some(listing.price.tiers.iter().map(|tier| tier.from).collect())
        }
    }
}

/// The tier a call whose prompt (fresh input, cache writes and cache reads)
/// is `prompt` tokens is on, given `tier_starts`: the last start it reaches,
/// None for the base rates.
pub(crate) fn tier_of(starts: &[u64], prompt: u64) -> Option<usize> {
    starts.iter().rposition(|from| prompt >= *from)
}

/// The context window a call ran in (agent-hud-native-open
/// `ModelCatalog.contextWindow(agentId:reported:largestSeen:)`): what the
/// client reported, else the published window. Claude Code does not report
/// it, and a Claude model runs with 200K or 1M, so a model this Mac has seen
/// hold more than its published window counts as 1M.
pub(crate) fn context_window(
    source: &str,
    model: &str,
    reported: Option<u64>,
    largest_seen: Option<u64>,
) -> Option<u64> {
    if let Some(reported) = reported.filter(|window| *window > 0) {
        return Some(reported);
    }
    let consumer = format!("{source}-model:{model}");
    // Only a Claude model gets Claude's 200K when none is published: Claude
    // Code can run other models, through a relay or on this computer.
    let claude = consumer.starts_with("claude-model:")
        && name_of(&consumer).is_some_and(|name| name.contains("claude"));
    let published = listed(&consumer)
        .and_then(|model| model.context_window)
        .or_else(|| claude.then_some(200_000))?;
    Some(if largest_seen.unwrap_or(0) > published {
        published.max(1_000_000)
    } else {
        published
    })
}

/// What calls that all fall in `tier` (an index into `tier_starts`, None for
/// the base rates) cost together on the international list at off-peak
/// rates. Within one tier the price is linear, so the sum of the calls'
/// tokens costs what the calls cost one by one.
pub(crate) fn cost_in_tier(
    source: &str,
    model: &str,
    tokens: &PriceTokens,
    tier: Option<usize>,
) -> Option<ModelCost> {
    match quote(&format!("{source}-model:{model}"), Region::International)?.1 {
        Quote::Flat(currency, rates) => Some(route_cost(&currency, rates, tokens)),
        Quote::Listed(listing, times) => {
            let amount = listing.price.rates(tier).amount(KindTokens::from(tokens))?;
            Some(ModelCost::new(
                listing.region.currency(),
                scaled(amount, times),
            ))
        }
    }
}

/// A call's list price split by token kind, in `KINDS` order (cache write,
/// input, reasoning, output, cache read): the rates `cost_at` would use, so
/// the call's tier and peak factor apply to every kind. The kinds can differ
/// from `cost_at`'s total by rounding when a relay multiplier applies.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KindCost {
    pub currency: String,
    pub picos: [i128; 5],
}

pub(crate) fn kind_cost_at(
    source: &str,
    model: &str,
    tokens: &PriceTokens,
    region: Region,
    at_ms: Option<i64>,
) -> Option<KindCost> {
    let kinds = KindTokens::from(tokens);
    match quote(&format!("{source}-model:{model}"), region)?.1 {
        Quote::Flat(currency, rates) => {
            // route_cost: fresh input, output with reasoning, cache read, cache write.
            let rate = |kind: TokenKind| match kind {
                TokenKind::CacheWrite => rates[3],
                TokenKind::Input => rates[0],
                TokenKind::Reasoning | TokenKind::Output => rates[1],
                TokenKind::CacheRead => rates[2],
            };
            let mut picos = [0_i128; 5];
            for (index, kind) in KINDS.into_iter().enumerate() {
                picos[index] = i128::try_from(kinds.0[index])
                    .ok()?
                    .checked_mul(rate(kind))?;
            }
            Some(KindCost { currency, picos })
        }
        Quote::Listed(listing, times) => {
            let tier = listing.price.tier(kinds);
            let rates = listing.price.rates(tier);
            let factor = factor(listing.model, at_ms);
            let mut picos = [0_i128; 5];
            for (index, kind) in KINDS.into_iter().enumerate() {
                let amount = i128::try_from(kinds.0[index].checked_mul(rates.rate(kind).into())?)
                    .ok()?
                    .checked_mul(factor)?;
                picos[index] = scaled(amount, times);
            }
            Some(KindCost {
                currency: listing.region.currency().to_owned(),
                picos,
            })
        }
    }
}

pub fn consumer_cost(
    consumer: &str,
    tokens: &PriceTokens,
    tiered: bool,
    region: Region,
    at_ms: Option<i64>,
) -> Option<ModelCost> {
    let (listing, times) = match quote(consumer, region)?.1 {
        Quote::Flat(currency, rates) => return Some(route_cost(&currency, rates, tokens)),
        Quote::Listed(listing, times) => (listing, times),
    };
    let kinds = KindTokens::from(tokens);
    let tier = tiered.then(|| listing.price.tier(kinds)).flatten();
    let amount = listing
        .price
        .rates(tier)
        .amount(kinds)?
        .checked_mul(factor(listing.model, at_ms))?;
    Some(ModelCost::new(
        listing.region.currency(),
        scaled(amount, times),
    ))
}
