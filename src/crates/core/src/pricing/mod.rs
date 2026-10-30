//! Model pricing catalog and token cost estimation.
//!
//! `catalog.json` is a snapshot of the vendors' own price pages, taken on its
//! `checkedOn` date. Every rate is an integer number of pico currency units
//! per token, preserving exact calculations. Unknown models, ids no vendor
//! publishes a price for, and gateways deliberately have no estimate.

use chrono::{Datelike, FixedOffset, TimeZone, Timelike};
use serde::Deserialize;
use std::collections::HashMap;
#[cfg(not(test))]
use std::sync::Mutex;
use std::sync::OnceLock;

mod regions;
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

/// Whether a route reaches a model server on this computer or its network
/// (a local model, `Ollama` and the like), whose calls are free: no list
/// price applies, and they cost nothing.
pub fn is_local_route(route: &str) -> bool {
    matches!(
        route,
        "local" | "oss" | "ollama" | "lmstudio" | "llama.cpp" | "llamacpp" | "vllm"
    )
}

/// The route a Claude Code session's calls are recorded under, from where its
/// session's hook said its calls go (`database::endpoints`): None for
/// Anthropic's or another vendor's own API, whose models are priced by name;
/// the cloud platform; the relay's host; a local model server's kind, which is
/// free; and for a local address no server was found at, its host, which
/// pricing tells a gateway from a model server by (`quote`).
pub(crate) fn claude_route(endpoint: &serde_json::Value) -> Option<String> {
    use agent_hud_agents::endpoints::{host_kind, HostKind};
    if let Some(platform) = endpoint["platform"].as_str() {
        return Some(platform.to_owned());
    }
    let host = endpoint["host"].as_str()?;
    match host_kind(host) {
        HostKind::Official | HostKind::Vendor => None,
        HostKind::Relay => Some(host.to_owned()),
        HostKind::Local => match endpoint["server"].as_str() {
            Some(server) if is_local_route(server) => Some(server.to_owned()),
            _ => Some(host.to_owned()),
        },
    }
}

/// A relay's own prices (agents `relays`), per token in pico units of its
/// currency, by model: input, output, cache read, cache write.
#[derive(Clone, Debug, PartialEq)]
pub struct RoutePrices {
    pub currency: String,
    pub models: HashMap<String, [i128; 4]>,
}

/// The prices that come from outside the catalog: the relays' lists and
/// what the owner set.
#[derive(Default)]
struct Overrides {
    routes: HashMap<String, RoutePrices>,
    user: UserPrices,
}

/// The overrides, shared by the whole engine.
#[cfg(not(test))]
fn overrides<R>(read: impl FnOnce(&mut Overrides) -> R) -> R {
    static STATE: OnceLock<Mutex<Overrides>> = OnceLock::new();
    let mut state = STATE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    read(&mut state)
}

/// The overrides of one test, so tests running side by side keep their own.
#[cfg(test)]
fn overrides<R>(read: impl FnOnce(&mut Overrides) -> R) -> R {
    thread_local! {
        static STATE: std::cell::RefCell<Overrides> = Default::default();
    }
    STATE.with(|state| read(&mut state.borrow_mut()))
}

/// Replaces the relays' prices, by route (the relay's host); true when they
/// changed.
pub(crate) fn set_route_prices(prices: HashMap<String, RoutePrices>) -> bool {
    overrides(|held| {
        let changed = held.routes != prices;
        held.routes = prices;
        changed
    })
}

/// What a relay's list `models` says for `model`: by its exact name, else by
/// the same name without a date, else loosely, without a `vendor/` prefix and
/// with dots read as dashes (`anthropic/claude-sonnet-4.5` is
/// `claude-sonnet-4-5`), when every loose match has the same rates.
fn listed_rates<'a, T: PartialEq>(models: &'a HashMap<String, T>, model: &str) -> Option<&'a T> {
    if let Some(rates) = models.get(model) {
        return Some(rates);
    }
    let name = normalized_model_name(model)?;
    if let Some((_, rates)) = models
        .iter()
        .find(|(listed, _)| normalized_model_name(listed).as_deref() == Some(name.as_str()))
    {
        return Some(rates);
    }
    let loose = loose_model_name(&name);
    let mut matches = models
        .iter()
        .filter(|(listed, _)| {
            normalized_model_name(listed).is_some_and(|listed| loose_model_name(&listed) == loose)
        })
        .map(|(_, rates)| rates);
    let first = matches.next()?;
    matches.all(|rates| rates == first).then_some(first)
}

/// A normalized model name without a `vendor/` prefix, dots read as dashes.
fn loose_model_name(name: &str) -> String {
    name.rsplit('/').next().unwrap_or(name).replace('.', "-")
}

/// What `tokens` cost at a relay's `rates`.
fn route_cost(currency: &str, rates: [i128; 4], tokens: &PriceTokens) -> ModelCost {
    let fresh = tokens.tokens_in.saturating_sub(tokens.cache_write) as i128;
    ModelCost::new(
        currency,
        fresh * rates[0]
            + tokens.tokens_out as i128 * rates[1]
            + tokens.cache_read as i128 * rates[2]
            + tokens.cache_write as i128 * rates[3],
    )
}

/// What the owner set on the price page (docs/proposals/no-plan-usage.md §3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UserPrices {
    /// By model as recorded (`<model>` for every route, `<model>#<route>` for
    /// one): its currency and per-token pico rates (input, output, cache
    /// read, cache write).
    pub custom: HashMap<String, (String, [i128; 4])>,
    /// By relay host: how many times the vendor's list price a call costs.
    pub multipliers: HashMap<String, f64>,
    /// By host on this computer or its network: true for a model server,
    /// whose calls are free; false for a gateway to a paid API.
    pub local_kinds: HashMap<String, bool>,
}

/// Replaces what the owner set; true when it changed.
pub(crate) fn set_user_prices(prices: UserPrices) -> bool {
    overrides(|held| {
        let changed = held.user != prices;
        held.user = prices;
        changed
    })
}

/// The owner's settings as prices: `customPrices` (per million tokens, an
/// empty cache rate taking the input's), `relayMultipliers`, `localKinds`.
pub(crate) fn user_prices_from(settings: &serde_json::Value) -> UserPrices {
    let mut prices = UserPrices::default();
    for (model, price) in settings["customPrices"].as_object().into_iter().flatten() {
        let currency = match price["currency"].as_str() {
            Some(currency @ ("USD" | "CNY")) => currency,
            _ => continue,
        };
        let rate = |key: &str| price[key].as_f64().filter(|v| v.is_finite() && *v >= 0.0);
        let (Some(input), Some(output)) = (rate("input"), rate("output")) else {
            continue;
        };
        let pico = |per_million: f64| (per_million * 1e6).round() as i128;
        let rates = [
            pico(input),
            pico(output),
            pico(rate("cacheRead").unwrap_or(input)),
            pico(rate("cacheWrite").unwrap_or(input)),
        ];
        prices
            .custom
            .insert(model.clone(), (currency.to_owned(), rates));
    }
    for (host, times) in settings["relayMultipliers"]
        .as_object()
        .into_iter()
        .flatten()
    {
        if let Some(times) = times.as_f64().filter(|v| v.is_finite() && *v > 0.0) {
            prices.multipliers.insert(host.clone(), times);
        }
    }
    for (host, kind) in settings["localKinds"].as_object().into_iter().flatten() {
        match kind.as_str() {
            Some("model") => prices.local_kinds.insert(host.clone(), true),
            Some("gateway") => prices.local_kinds.insert(host.clone(), false),
            _ => None,
        };
    }
    prices
}

/// Where a call's price comes from, in the order it is looked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The owner's own price for the model.
    Custom,
    /// A model on this computer or its network: free.
    Local,
    /// The relay's own price list.
    Relay,
    /// The vendor's list price times the owner's multiplier for the relay.
    Multiplier,
    /// The vendor's list price, through its own API or plan.
    Catalog,
}

impl Origin {
    pub fn name(self) -> &'static str {
        match self {
            Self::Custom => "custom",
            Self::Local => "local",
            Self::Relay => "relay",
            Self::Multiplier => "multiplier",
            Self::Catalog => "catalog",
        }
    }
}

/// How a call is priced: flat per-token rates in a currency, or a catalog
/// listing (tiers, peak hours) times a multiplier.
enum Quote {
    Flat(String, [i128; 4]),
    Listed(Listing, f64),
}

/// How `consumer` (`<client>-model:<model>[#<route>]`) is priced on `region`,
/// and where the price comes from: the owner's price for the model on that
/// route, then for the model on any route; free on a local route (a named
/// model server, or a local host the owner calls a model server, or by
/// default one whose model has no list price); the relay's own list; the
/// owner's multiplier on the vendor's list; the vendor's list through its own
/// service. None when nothing prices it.
fn quote(consumer: &str, region: Region) -> Option<(Origin, Quote)> {
    use agent_hud_agents::endpoints::{host_kind, HostKind};
    let (base, route) = consumer
        .split_once('#')
        .map_or((consumer, None), |(base, route)| (base, Some(route)));
    let model = consumer
        .split_once("-model:")
        .map_or(consumer, |(_, model)| model);
    let bare = base.split_once("-model:").map_or(base, |(_, model)| model);
    // What the overrides say, read at once.
    let (custom, local_kind, relay, multiplier) = overrides(|held| {
        let user = &held.user;
        let custom = |name: &str| {
            user.custom.get(name).cloned().or_else(|| {
                let normalized = normalized_model_name(name)?;
                let route = name.split_once('#').map(|(_, route)| route);
                user.custom
                    .iter()
                    .find(|(listed, _)| {
                        listed.split_once('#').map(|(_, route)| route) == route
                            && normalized_model_name(listed).as_deref() == Some(normalized.as_str())
                    })
                    .map(|(_, price)| price.clone())
            })
        };
        let custom = custom(model).or_else(|| route.and_then(|_| custom(bare)));
        let route = route.unwrap_or("");
        let relay = held.routes.get(route).and_then(|prices| {
            listed_rates(&prices.models, bare).map(|rates| (prices.currency.clone(), *rates))
        });
        (
            custom,
            user.local_kinds.get(route).copied(),
            relay,
            user.multipliers.get(route).copied(),
        )
    });
    if let Some((currency, rates)) = custom {
        return Some((Origin::Custom, Quote::Flat(currency, rates)));
    }
    if let Some(route) = route {
        let local = is_local_route(route)
            || (host_kind(route) == HostKind::Local
                && local_kind.unwrap_or_else(|| listed(base).is_none()));
        if local {
            return Some((
                Origin::Local,
                Quote::Flat(Region::International.currency().into(), [0; 4]),
            ));
        }
        if let Some((currency, rates)) = relay {
            return Some((Origin::Relay, Quote::Flat(currency, rates)));
        }
        if let Some(times) = multiplier {
            return Some((
                Origin::Multiplier,
                Quote::Listed(listing(base, region)?, times),
            ));
        }
    }
    Some((
        Origin::Catalog,
        Quote::Listed(listing(consumer, region)?, 1.0),
    ))
}

/// `amount` times the owner's multiplier.
fn scaled(amount: i128, times: f64) -> i128 {
    if times == 1.0 {
        amount
    } else {
        (amount as f64 * times).round() as i128
    }
}

/// What a model costs per million tokens and where the price comes from, as
/// the price page shows it: input, output, cache read and cache write, at the
/// base rates (no prompt tier, off peak).
pub(crate) fn price_quote(consumer: &str) -> Option<(Origin, String, [f64; 4])> {
    let per_million = |pico: i128| pico as f64 / 1e6;
    let (origin, quote) = quote(consumer, Region::International)?;
    Some(match quote {
        Quote::Flat(currency, rates) => (origin, currency, rates.map(per_million)),
        Quote::Listed(listing, times) => {
            let rates = &listing.price.rates;
            let rate = |value: u64| per_million(scaled(value as i128, times));
            (
                origin,
                listing.region.currency().to_owned(),
                [
                    rate(rates.input),
                    rate(rates.output),
                    rate(rates.cache_read),
                    rate(rates.cache_write),
                ],
            )
        }
    })
}

#[derive(Clone, Copy)]
struct Listing {
    model: &'static Model,
    price: &'static Price,
    region: Region,
}

fn listing(consumer: &str, region: Region) -> Option<Listing> {
    let model = model_for(consumer)?;
    let (price, region) = model.price(region)?;
    Some(Listing {
        model,
        price,
        region,
    })
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn cost(source: &str, model: &str, tokens: &PriceTokens) -> Option<i128> {
        cost_at(source, model, tokens, true, Region::International, None)
            .map(|cost| cost.amount_pico)
    }

    #[test]
    fn a_local_route_is_free_and_a_relay_route_unpriced() {
        let tokens = every_kind();
        assert_eq!(cost("codex", "gpt-oss:20b#ollama", &tokens), Some(0));
        assert_eq!(cost("claude", "qwen3-coder:30b#local", &tokens), Some(0));
        assert_eq!(
            cost("codex", "gpt-6-sol#ollama", &tokens),
            Some(0),
            "a catalog model served locally is still free"
        );
        assert_eq!(
            tier_starts("codex", "gpt-oss:20b#lmstudio"),
            Some(Vec::new())
        );
        assert_eq!(
            cost_in_tier("codex", "gpt-oss:20b#oss", &tokens, None).map(|c| c.amount_pico),
            Some(0)
        );
        assert_eq!(cost("codex", "gpt-6-sol#myrelay", &tokens), None, "a relay");
        assert!(cost("codex", "gpt-6-sol", &tokens).is_some_and(|cost| cost > 0));
    }

    #[test]
    fn a_relay_route_takes_the_relays_own_prices() {
        let tokens = PriceTokens {
            tokens_in: 1_200,
            tokens_out: 100,
            cache_read: 5_000,
            cache_write: 200,
            reasoning: 0,
        };
        set_route_prices(HashMap::from([(
            "relay-test.example.com".to_string(),
            RoutePrices {
                currency: "CNY".into(),
                models: HashMap::from([("claude-sonnet-4-5".to_string(), [10, 50, 1, 12])]),
            },
        )]));
        let expected = 1_000 * 10 + 100 * 50 + 5_000 + 200 * 12;
        let charged = cost_at(
            "claude",
            "claude-sonnet-4-5-20250929#relay-test.example.com",
            &tokens,
            true,
            Region::International,
            None,
        )
        .unwrap();
        assert_eq!(
            (charged.currency.as_str(), charged.amount_pico),
            ("CNY", expected),
            "dated name, the relay's list"
        );
        assert_eq!(
            tier_starts("claude", "claude-sonnet-4-5#relay-test.example.com"),
            Some(Vec::new())
        );
        assert_eq!(
            cost("claude", "claude-opus-4-5#relay-test.example.com", &tokens),
            None,
            "a model the relay does not list"
        );
        assert_eq!(
            cost(
                "claude",
                "claude-sonnet-4-5#other-relay.example.com",
                &tokens
            ),
            None
        );

        // A relay that names models its own way.
        set_route_prices(HashMap::from([(
            "gate-test.example.com".to_string(),
            RoutePrices {
                currency: "USD".into(),
                models: HashMap::from([
                    ("anthropic/claude-sonnet-4.5".to_string(), [3, 15, 0, 0]),
                    ("openai/gpt-6".to_string(), [2, 8, 0, 0]),
                    ("azure/gpt-6".to_string(), [4, 9, 0, 0]),
                ]),
            },
        )]));
        let plain = PriceTokens {
            tokens_in: 1_000,
            tokens_out: 10,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        };
        assert_eq!(
            cost(
                "claude",
                "claude-sonnet-4-5-20250929#gate-test.example.com",
                &plain
            ),
            Some(1_000 * 3 + 10 * 15),
            "vendor prefix and dots"
        );
        assert_eq!(
            cost("codex", "gpt-6#gate-test.example.com", &plain),
            None,
            "two listings disagree"
        );
        set_route_prices(HashMap::new());
        assert_eq!(
            cost(
                "claude",
                "claude-sonnet-4-5#relay-test.example.com",
                &tokens
            ),
            None
        );
    }

    #[test]
    fn the_owners_prices_come_first_then_local_relay_multiplier_and_catalog() {
        let tokens = PriceTokens {
            tokens_in: 1_000_000,
            tokens_out: 0,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        };
        let catalog = cost("codex", "gpt-6-sol", &tokens).unwrap();
        let origin = |consumer: &str| price_quote(consumer).map(|(origin, _, _)| origin);
        set_user_prices(user_prices_from(&serde_json::json!({
            "customPrices": {
                "qwen3-coder:30b": {"currency": "CNY", "input": 1.0, "output": 4.0, "cacheRead": null, "cacheWrite": null},
                "gpt-6-sol#relay.example.com": {"currency": "USD", "input": 0.5, "output": 2.0},
                "broken": {"currency": "EUR", "input": 1.0, "output": 1.0}
            },
            "relayMultipliers": {"cheap.example.com": 0.3},
            "localKinds": {"localhost:4000": "model", "localhost:5000": "gateway"}
        })));
        // The owner's price, for one route and for every route.
        assert_eq!(
            cost("codex", "gpt-6-sol#relay.example.com", &tokens),
            Some(500_000_000_000)
        );
        assert_eq!(
            origin("codex-model:gpt-6-sol#relay.example.com"),
            Some(Origin::Custom)
        );
        let custom = cost_at(
            "claude",
            "qwen3-coder:30b#relay.example.com",
            &tokens,
            true,
            Region::International,
            None,
        )
        .unwrap();
        assert_eq!(
            (custom.currency.as_str(), custom.amount_pico),
            ("CNY", 1_000_000_000_000)
        );
        assert_eq!(
            price_quote("claude-model:qwen3-coder:30b").map(|(_, _, rates)| rates),
            Some([1.0, 4.0, 1.0, 1.0]),
            "an empty cache rate takes the input's"
        );
        // A multiplier on the vendor's list.
        assert_eq!(
            cost("codex", "gpt-6-sol#cheap.example.com", &tokens),
            Some((catalog as f64 * 0.3).round() as i128)
        );
        assert_eq!(
            origin("codex-model:gpt-6-sol#cheap.example.com"),
            Some(Origin::Multiplier)
        );
        // A local host: as the owner says, else a listed model goes through a gateway.
        assert_eq!(
            cost("claude", "claude-sonnet-4-5#localhost:4000", &tokens),
            Some(0)
        );
        assert_eq!(
            cost("claude", "claude-sonnet-4-5#localhost:5000", &tokens),
            None
        );
        assert_eq!(
            cost("claude", "claude-sonnet-4-5#localhost:6000", &tokens),
            None,
            "a gateway by default"
        );
        assert_eq!(
            cost("claude", "mystery-model#localhost:6000", &tokens),
            Some(0),
            "a model server by default"
        );
        assert_eq!(
            origin("claude-model:mystery-model#localhost:6000"),
            Some(Origin::Local)
        );
        assert_eq!(origin("codex-model:gpt-6-sol"), Some(Origin::Catalog));
        assert_eq!(
            cost("codex", "gpt-6-sol#other.example.com", &tokens),
            None,
            "a relay with nothing set"
        );
        set_user_prices(UserPrices::default());
        assert_eq!(cost("codex", "gpt-6-sol#relay.example.com", &tokens), None);
    }

    #[test]
    fn only_a_claude_model_gets_claudes_window_when_none_is_published() {
        assert_eq!(
            context_window("claude", "claude-future-9", None, None),
            Some(200_000)
        );
        assert_eq!(
            context_window("claude", "qwen3-coder:30b#local", None, None),
            None
        );
        assert_eq!(
            context_window("claude", "qwen3-coder:30b", None, None),
            None
        );
        assert_eq!(
            context_window("claude", "qwen3-coder:30b", Some(32_768), None),
            Some(32_768),
            "a reported window still counts"
        );
    }

    fn every_kind() -> PriceTokens {
        PriceTokens {
            tokens_in: 10_000,
            tokens_out: 3_000,
            cache_read: 50_000,
            cache_write: 2_000,
            reasoning: 1_000,
        }
    }

    #[test]
    fn antigravity_effort_variants_cost_what_their_model_costs() {
        let tokens = every_kind();
        for (variant, model) in [
            ("claude-opus-5-5-low", "claude-opus-5-5"),
            ("claude-opus-5-5-medium", "claude-opus-5-5"),
            ("claude-opus-5-5-high", "claude-opus-5-5"),
            ("claude-opus-5-5-max", "claude-opus-5-5"),
            ("claude-sonnet-5-5-low", "claude-sonnet-5-5"),
            ("claude-sonnet-5-5-medium", "claude-sonnet-5-5"),
            ("claude-sonnet-5-5-high", "claude-sonnet-5-5"),
            ("claude-sonnet-5-5-max", "claude-sonnet-5-5"),
            ("gemini-4-argon-low", "gemini-4-argon"),
            ("gemini-4-argon-medium", "gemini-4-argon"),
            ("gemini-4-argon-high", "gemini-4-argon"),
            ("gemini-3.8-flash-low", "gemini-3.8-flash"),
            ("gemini-3.8-flash-medium", "gemini-3.8-flash"),
            ("gemini-3.8-flash-high", "gemini-3.8-flash"),
        ] {
            let priced = cost("antigravity", variant, &tokens);
            assert!(
                priced.is_some_and(|pico| pico > 0),
                "{variant} has no price"
            );
            assert_eq!(priced, cost("antigravity", model, &tokens), "{variant}");
            assert_eq!(
                context_window("antigravity", variant, None, None),
                context_window("antigravity", model, None, None),
                "{variant}"
            );
        }
    }

    #[test]
    fn codex_gpt_6_1_terra_prices_both_context_tiers() {
        assert_eq!(tier_starts("codex", "gpt-6.1-terra"), Some(vec![272_001]));
        let at = |cache_read| PriceTokens {
            tokens_in: 10_000,
            cache_write: 2_000,
            cache_read,
            tokens_out: 1_000,
            reasoning: 0,
        };
        // Official Standard list price: input $1, 30m write $1.25,
        // cached input $0.05, output $5 per million tokens.
        let short = 8_000 * 1_000_000 + 2_000 * 1_250_000 + 262_000 * 50_000 + 1_000 * 5_000_000;
        assert_eq!(cost("codex", "gpt-6.1-terra", &at(262_000)), Some(short));
        // Over 272K prompt tokens, input and cache double, output is 1.5x.
        let long = 8_000 * 2_000_000 + 2_000 * 2_500_000 + 262_001 * 100_000 + 1_000 * 7_500_000;
        assert_eq!(cost("codex", "gpt-6.1-terra", &at(262_001)), Some(long));
        assert_eq!(
            context_window("codex", "gpt-6.1-terra", None, None),
            Some(1_050_000)
        );
    }

    #[test]
    fn haiku_5_5_pays_the_higher_list_for_a_prompt_over_100k() {
        assert_eq!(
            tier_starts("claude", "claude-haiku-5-5"),
            Some(vec![100_001])
        );
        // A prompt counts fresh input, cache writes and cache reads.
        let at = |cache_read| PriceTokens {
            tokens_in: 10_000,
            cache_write: 2_000,
            cache_read,
            tokens_out: 1_000,
            reasoning: 0,
        };
        // $0.10 in, $0.20 1-hour write, $0.01 read, $0.50 out.
        let short = 2_000 * 200_000 + 8_000 * 100_000 + 90_000 * 10_000 + 1_000 * 500_000;
        assert_eq!(cost("claude", "claude-haiku-5-5", &at(90_000)), Some(short));
        // $0.50 in, $1 1-hour write, $0.05 read, $2.50 out.
        let long = 2_000 * 1_000_000 + 8_000 * 500_000 + 90_001 * 50_000 + 1_000 * 2_500_000;
        assert_eq!(cost("claude", "claude-haiku-5-5", &at(90_001)), Some(long));
        // A pre-summed bucket's prompt is unknown: base rates.
        let summed = cost_at(
            "claude",
            "claude-haiku-5-5",
            &at(90_001),
            false,
            Region::International,
            None,
        )
        .map(|cost| cost.amount_pico);
        assert_eq!(summed, Some(short + 10_000));
        assert_eq!(
            context_window("claude", "claude-haiku-5-5", None, None),
            Some(1_000_000)
        );
    }

    #[test]
    fn gemini_4_argon_and_sonnet_5_5_cache_reads_follow_their_pages() {
        let million = PriceTokens {
            tokens_in: 1_000_000,
            tokens_out: 1_000_000,
            cache_read: 1_000_000,
            cache_write: 0,
            reasoning: 400_000,
        };
        // $2 in, $0.10 cached, $10 out with thinking.
        let argon = cost_at(
            "antigravity",
            "gemini-4-argon",
            &million,
            true,
            Region::International,
            None,
        )
        .unwrap();
        assert_eq!(argon.currency, "USD");
        assert_eq!(argon.amount_pico, 12_100_000_000_000);
        let read = PriceTokens {
            cache_read: 1_000_000,
            ..Default::default()
        };
        assert_eq!(
            cost("claude", "claude-sonnet-5-5", &read),
            Some(100_000_000_000)
        );
    }

    #[test]
    fn ids_no_vendor_prices_and_gateway_routes_stay_unpriced() {
        let tokens = every_kind();
        for (source, model) in [
            ("antigravity", "gemini-3.8-flash-tiered"),
            ("antigravity", "gemini-3.8-flash-n"),
            ("codex", "codex-auto-review"),
            ("cursor", "grok-bot"),
            ("grok-bot", "grok-bot-default"),
        ] {
            assert_eq!(cost(source, model, &tokens), None, "{model}");
        }
        assert!(model_for("pi-model:claude-haiku-5-5#anthropic").is_some());
        assert!(model_for("pi-model:claude-haiku-5-5#openrouter").is_none());
        assert!(model_for("opencode-model:gemini-4-argon#google").is_some());
        assert!(model_for("opencode-model:gemini-4-argon#google-vertex").is_none());
    }
}
