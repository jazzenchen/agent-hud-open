//! Cached account facts supplied by the platform account reader. The domain
//! engine never performs network or engine I/O while projecting local sessions.
use super::*;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Default, PartialEq)]
pub struct AccountProjection {
    pub quotas: Vec<Value>,
    pub billing: Vec<Value>,
    pub accounts: Vec<Value>,
    pub providers: HashMap<String, Value>,
    pub session_records: Vec<Value>,
    /// Which clients reach which vendors' plans and APIs, and on which
    /// platform (`{client, provider, product, accountId, region}`).
    pub services: Vec<Value>,
    pub external_prices: pricing::ExternalPrices,
}

impl crate::AgentHudCore {
    pub fn set_account_projection(&mut self, accounts: AccountProjection) {
        if let Some(database) = &self.database {
            let mut changed = false;
            for row in &accounts.quotas {
                let Some(id) = row["id"].as_str() else {
                    continue;
                };
                let Some(observed) = row["observedAt"].as_str() else {
                    continue;
                };
                if self
                    .account_samples
                    .get(id)
                    .is_some_and(|previous| previous == observed)
                {
                    continue;
                }
                if row["status"] == "stale" {
                    continue;
                }
                let Some(at) = metering::date_ms(observed) else {
                    continue;
                };
                let Some(used) = row["usedPercent"].as_f64() else {
                    continue;
                };
                // The plan the reading was taken under, so a share is only
                // ever compared with others measured against the same cap.
                let plan = accounts
                    .accounts
                    .iter()
                    .find(|account| account["id"] == row["accountId"])
                    .and_then(|account| {
                        crate::plan_value::plan_key(
                            account["provider"].as_str()?,
                            account["plan"].as_str()?,
                        )
                    });
                match metering::insert_quota_sample(
                    database,
                    id,
                    at,
                    (100.0 - used).clamp(0.0, 100.0),
                    plan.as_deref(),
                    row["resetAt"].as_str().and_then(metering::date_ms),
                ) {
                    Ok(count) => {
                        changed |= count > 0;
                        self.account_samples.insert(id.into(), observed.into());
                    }
                    Err(_) => {
                        self.storage_issue = Some(l10n::text(
                            "额度读数暂时无法保存",
                            "Quota sample persistence is unavailable",
                        ))
                    }
                }
            }
            if let Some(path) = &self.legacy_quota_database {
                let pairs = accounts
                    .quotas
                    .iter()
                    .filter_map(|row| {
                        // The Swift app kept Grok Bot's quota under its `grok` provider.
                        let scope = match row["vendor"].as_str()? {
                            "grok-bot" => "grok",
                            vendor => vendor,
                        };
                        Some((
                            row["id"].as_str()?.into(),
                            scope.into(),
                            row["legacyWindowId"].as_str()?.into(),
                        ))
                    })
                    .collect::<Vec<_>>();
                match legacy_quota::import(database, path, &pairs) {
                    Ok(count) => changed |= count > 0,
                    Err(_) => {
                        self.storage_issue = Some(l10n::text(
                            "无法读取之前的额度记录",
                            "Previous quota history could not be read",
                        ))
                    }
                }
            }
            if changed {
                let _ = metering::prune_quota_samples(
                    database,
                    Utc::now().timestamp_millis() - crate::quota_history::KEPT_MS,
                );
                self.meters.samples = None;
            }
        }
        if !accounts.session_records.is_empty()
            && self.accounts.session_records != accounts.session_records
        {
            let _ = self.ingest_account_sessions(&accounts.session_records);
        }
        let regions = pricing::PriceRegions::new(&accounts.services, &accounts.billing);
        if regions != self.price_regions {
            // Costs already shown were priced on the old platforms.
            self.price_regions = regions;
            self.meters.view = None;
            self.statistics.revision = self.statistics.revision.wrapping_add(1);
        }
        let routes_changed = pricing::set_route_prices(route_prices(&accounts.services));
        let external_changed = pricing::set_external_prices(accounts.external_prices.clone());
        if routes_changed || external_changed {
            // Calls through a relay are priced on its new list: what was left
            // unpriced is priced again, and the rates and statistics follow.
            for head in self.logs.values_mut() {
                if head.unpriced_tokens.unwrap_or(0) > 0 {
                    head.list_cost_priced = false;
                }
            }
            self.meters.view = None;
            self.statistics.revision = self.statistics.revision.wrapping_add(1);
        }
        if self.accounts != accounts {
            self.render.accounts = self.render.accounts.wrapping_add(1);
            self.accounts = accounts;
        }
    }
}

/// The relays' price lists the relay reader brought (`services` rows of
/// product `relay` with `prices`: per million tokens), per token in pico
/// units, by every route calls through the relay are recorded under: its
/// host, and the Codex providers that point at it.
fn route_prices(services: &[Value]) -> HashMap<String, pricing::RoutePrices> {
    services
        .iter()
        .filter(|row| row["product"] == "relay")
        .filter_map(|row| {
            let host = row["provider"].as_str()?;
            let mut routes = row["routes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if !routes.iter().any(|route| route == host) {
                routes.push(host.to_owned());
            }
            let prices = &row["prices"];
            let currency = prices["currency"].as_str()?;
            let models = prices["models"]
                .as_object()?
                .iter()
                .filter_map(|(model, rates)| {
                    let rates = rates.as_array()?;
                    let pico = |index: usize| -> Option<i128> {
                        let per_million = rates.get(index)?.as_f64()?;
                        (per_million >= 0.0).then(|| (per_million * 1e6).round() as i128)
                    };
                    Some((model.clone(), [pico(0)?, pico(1)?, pico(2)?, pico(3)?]))
                })
                .collect::<HashMap<_, _>>();
            (!models.is_empty()).then(|| {
                let prices = pricing::RoutePrices {
                    currency: currency.to_owned(),
                    models,
                };
                routes.into_iter().map(move |route| (route, prices.clone()))
            })
        })
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_relays_price_list_becomes_per_token_rates() {
        let services = vec![
            json!({"product": "relay", "provider": "relay.example.com", "routes": ["relay.example.com", "myrelay"],
                   "prices": {"currency": "CNY", "models": {"claude-sonnet-4-5": [21.0, 105.0, 2.1, 26.25], "bad": [1.0]}}}),
            json!({"product": "relay", "provider": "quiet.example.com", "routes": ["quiet.example.com"]}),
            json!({"product": "coding-plan", "provider": "zai", "client": "claude", "prices": {"currency": "USD", "models": {"x": [1, 1, 1, 1]}}}),
        ];
        let prices = route_prices(&services);
        assert_eq!(prices.len(), 2, "only a relay with a list, by each route");
        assert_eq!(
            prices["myrelay"], prices["relay.example.com"],
            "a Codex provider that points at it"
        );
        let relay = &prices["relay.example.com"];
        assert_eq!(relay.currency, "CNY");
        assert_eq!(
            relay.models["claude-sonnet-4-5"],
            [21_000_000, 105_000_000, 2_100_000, 26_250_000]
        );
        assert!(!relay.models.contains_key("bad"));
    }
}
