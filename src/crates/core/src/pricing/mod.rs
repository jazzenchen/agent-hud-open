//! Model pricing catalog and token cost estimation.
//!
//! `catalog.json` is a snapshot of the vendors' own price pages, taken on its
//! `checkedOn` date. Every rate is an integer number of pico currency units
//! per token, preserving exact calculations. Unknown models, ids no vendor
//! publishes a price for, and gateways deliberately have no estimate.

use chrono::{Datelike, FixedOffset, TimeZone, Timelike};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

mod cost;
mod external;
pub(crate) use external::set_external_prices;
pub use external::{parse_external_prices, ExternalPrices};
mod overrides;
mod quote;
mod regions;
pub(crate) use cost::{context_window, cost_at, cost_in_tier, kind_cost_at, tier_of, tier_starts};
pub(crate) use overrides::{
    claude_route, is_local_route, set_route_prices, set_user_prices, user_prices_from, RoutePrices,
    UserPrices,
};
pub(crate) use quote::{price_quote, Origin};
pub use regions::PriceRegions;

const PICOS_PER_UNIT: f64 = 1_000_000_000_000.0;

/// Log counts: input includes cache writes, output includes reasoning, and cache
/// reads are separate. The exclusive kinds are derived only once at this edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PriceTokens {
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub reasoning: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelCost {
    pub currency: String,
    pub amount: f64,
    pub amount_pico: i128,
}

impl ModelCost {
    fn new(currency: &str, amount_pico: i128) -> Self {
        Self {
            currency: currency.into(),
            amount: amount_pico as f64 / PICOS_PER_UNIT,
            amount_pico,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    #[default]
    International,
    China,
}

impl Region {
    pub fn currency(self) -> &'static str {
        match self {
            Self::International => "USD",
            Self::China => "CNY",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TokenKind {
    CacheWrite,
    Input,
    Reasoning,
    Output,
    CacheRead,
}

const KINDS: [TokenKind; 5] = [
    TokenKind::CacheWrite,
    TokenKind::Input,
    TokenKind::Reasoning,
    TokenKind::Output,
    TokenKind::CacheRead,
];

#[derive(Clone, Copy, Debug, Default)]
struct KindTokens([u128; 5]);

impl From<&PriceTokens> for KindTokens {
    fn from(tokens: &PriceTokens) -> Self {
        let write = tokens.cache_write.min(tokens.tokens_in);
        let thought = tokens.reasoning.min(tokens.tokens_out);
        Self([
            write.into(),
            (tokens.tokens_in - write).into(),
            thought.into(),
            (tokens.tokens_out - thought).into(),
            tokens.cache_read.into(),
        ])
    }
}

impl KindTokens {
    fn prompt(self) -> u128 {
        self.0[0] + self.0[1] + self.0[4]
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rates {
    input: u64,
    cache_write: u64,
    cache_read: u64,
    output: u64,
}

impl Rates {
    fn rate(&self, kind: TokenKind) -> u64 {
        match kind {
            TokenKind::CacheWrite => self.cache_write,
            TokenKind::Input => self.input,
            TokenKind::CacheRead => self.cache_read,
            TokenKind::Output | TokenKind::Reasoning => self.output,
        }
    }

    fn amount(&self, kinds: KindTokens) -> Option<i128> {
        let mut total = 0_u128;
        for (kind, tokens) in KINDS.into_iter().zip(kinds.0) {
            total = total.checked_add(tokens.checked_mul(self.rate(kind).into())?)?;
        }
        i128::try_from(total).ok()
    }
}

#[derive(Debug, Deserialize)]
struct Tier {
    from: u64,
    rates: Rates,
}

#[derive(Debug, Deserialize)]
struct Price {
    rates: Rates,
    tiers: Vec<Tier>,
}

impl Price {
    fn tier(&self, kinds: KindTokens) -> Option<usize> {
        self.tiers
            .iter()
            .rposition(|tier| kinds.prompt() >= tier.from.into())
    }

    fn rates(&self, tier: Option<usize>) -> &Rates {
        tier.map(|index| &self.tiers[index].rates)
            .unwrap_or(&self.rates)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub vendor: String,
    prices: HashMap<Region, Price>,
    pub global: bool,
    pub peak_hours: bool,
    /// The context window the vendor publishes, in tokens.
    #[serde(default)]
    pub context_window: Option<u64>,
}

impl Model {
    /// The list a call on `region` is priced by: that platform's, or the one
    /// list of a global vendor. A regional model a platform does not sell has
    /// no price there (agent-hud-native-open `ModelCatalog.Model.price(in:)`).
    fn price(&self, region: Region) -> Option<(&Price, Region)> {
        self.prices
            .get(&region)
            .map(|price| (price, region))
            .or_else(|| {
                self.global
                    .then(|| self.prices.get(&Region::International))
                    .flatten()
                    .map(|price| (price, Region::International))
            })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    checked_on: String,
    rate_unit: String,
    models: HashMap<String, Model>,
    aliases: HashMap<String, String>,
    vendor_services: HashMap<String, String>,
}

/// The prices that come from outside the catalog: the relays' lists and
/// what the owner set.
#[derive(Default)]
struct Overrides {
    routes: HashMap<String, RoutePrices>,
    external: ExternalPrices,
    user: UserPrices,
}

/// How a call is priced: flat per-token rates in a currency, or a catalog
/// listing (tiers, peak hours) times a multiplier.
enum Quote {
    Flat(String, [i128; 4]),
    Listed(Listing, f64),
}

#[derive(Clone, Copy)]
struct Listing {
    model: &'static Model,
    price: &'static Price,
    region: Region,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let catalog: Catalog = serde_json::from_str(include_str!("catalog.json"))
            .expect("the checked-in source price catalog must be valid");
        assert_eq!(catalog.rate_unit, "pico-currency-per-token");
        catalog
    })
}

pub fn checked_on() -> &'static str {
    &catalog().checked_on
}

/// Exact source normalization, not a display-name or family-name heuristic.
/// In particular, `openai/gpt-5.5` stays gateway-qualified and is unpriced.
pub fn normalized_model_name(model: &str) -> Option<String> {
    let mut name = model.split('#').next()?.to_lowercase();
    if name.ends_with("[1m]") {
        name.truncate(name.len() - 4);
    }
    let bytes = name.as_bytes();
    let compact_date = bytes
        .len()
        .checked_sub(9)
        .filter(|at| bytes[*at] == b'-' && bytes[*at + 1..].iter().all(u8::is_ascii_digit));
    let dashed_date = bytes.len().checked_sub(11).filter(|at| {
        bytes[*at] == b'-'
            && bytes[*at + 5] == b'-'
            && bytes[*at + 8] == b'-'
            && bytes[*at + 1..*at + 5].iter().all(u8::is_ascii_digit)
            && bytes[*at + 6..*at + 8].iter().all(u8::is_ascii_digit)
            && bytes[*at + 9..].iter().all(u8::is_ascii_digit)
    });
    if let Some(at) = compact_date.or(dashed_date) {
        name.truncate(at);
    }
    (!name.is_empty()).then_some(name)
}

pub fn name_of(consumer: &str) -> Option<String> {
    let (_, model) = consumer.split_once("-model:")?;
    normalized_model_name(model)
}

fn listed(consumer: &str) -> Option<&'static Model> {
    let name = name_of(consumer)?;
    let catalog = catalog();
    let name = catalog.aliases.get(&name).unwrap_or(&name);
    catalog.models.get(name)
}

/// A route is accepted only when the original catalog identifies it as that
/// vendor's own API/plan. Azure, Bedrock, Vertex, OpenRouter and proxies are not
/// priced with a vendor's list merely because their model has a familiar name.
pub fn model_for(consumer: &str) -> Option<&'static Model> {
    let model = listed(consumer)?;
    if let Some((_, route)) = consumer.split_once('#') {
        if catalog().vendor_services.get(route) != Some(&model.vendor)
            && !(route == "deepseek-official" && model.vendor == "DeepSeek")
        {
            return None;
        }
    }
    Some(model)
}

#[cfg(test)]
mod tests;
