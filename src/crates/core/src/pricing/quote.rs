//! A model's price as the Statistics window quotes it: from a relay, the
//! owner, or the catalog, and which list it came from.
use super::overrides::{listed_rates, overrides};
use super::*;

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
    /// The public LiteLLM catalog for an otherwise unlisted direct model.
    External,
}

impl Origin {
    pub fn name(self) -> &'static str {
        match self {
            Self::Custom => "custom",
            Self::Local => "local",
            Self::Relay => "relay",
            Self::Multiplier => "multiplier",
            Self::Catalog => "catalog",
            Self::External => "external",
        }
    }
}

/// How `consumer` (`<client>-model:<model>[#<route>]`) is priced on `region`,
/// and where the price comes from: the owner's price for the model on that
/// route, then for the model on any route; free on a local route (a named
/// model server, or a local host the owner calls a model server, or by
/// default one whose model has no list price); the relay's own list; the
/// owner's multiplier on the vendor's list; the vendor's list through its own
/// service. None when nothing prices it.
pub(super) fn quote(consumer: &str, region: Region) -> Option<(Origin, Quote)> {
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
    if let Some(listing) = listing(consumer, region) {
        return Some((Origin::Catalog, Quote::Listed(listing, 1.0)));
    }
    if route.is_some() || region != Region::International || listed(consumer).is_some() {
        return None;
    }
    let rates = overrides(|held| held.external.get(bare).copied())?;
    Some((Origin::External, Quote::Flat("USD".into(), rates)))
}

/// `amount` times the owner's multiplier.
pub(super) fn scaled(amount: i128, times: f64) -> i128 {
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

fn listing(consumer: &str, region: Region) -> Option<Listing> {
    let model = model_for(consumer)?;
    let (price, region) = model.price(region)?;
    Some(Listing {
        model,
        price,
        region,
    })
}
