//! Prices from outside the catalog: the routes a call can take, the relays'
//! own lists, and what the owner set.
use super::*;
#[cfg(not(test))]
use std::sync::{Mutex, PoisonError};

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

/// The overrides, shared by the whole engine.
#[cfg(not(test))]
pub(super) fn overrides<R>(read: impl FnOnce(&mut Overrides) -> R) -> R {
    static STATE: OnceLock<Mutex<Overrides>> = OnceLock::new();
    let mut state = STATE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    read(&mut state)
}

/// The overrides of one test, so tests running side by side keep their own.
#[cfg(test)]
pub(super) fn overrides<R>(read: impl FnOnce(&mut Overrides) -> R) -> R {
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
pub(super) fn listed_rates<'a, T: PartialEq>(
    models: &'a HashMap<String, T>,
    model: &str,
) -> Option<&'a T> {
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
pub(super) fn route_cost(currency: &str, rates: [i128; 4], tokens: &PriceTokens) -> ModelCost {
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
        let rate = |key: &str| price[key].as_f64().filter(|v| *v >= 0.0);
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
        if let Some(times) = times.as_f64().filter(|v| *v > 0.0) {
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
