//! Flat USD estimates from the public LiteLLM catalog.
use super::{overrides::overrides, PICOS_PER_UNIT};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};

pub type ExternalPrices = Arc<HashMap<String, [i128; 4]>>;

pub fn parse_external_prices(feed: &Value) -> Option<ExternalPrices> {
    let mut prices = HashMap::new();
    let mut conflicts = std::collections::HashSet::new();
    for (key, row) in feed.as_object()? {
        let Some(provider @ ("openai" | "anthropic" | "gemini" | "xai")) =
            row["litellm_provider"].as_str()
        else {
            continue;
        };
        let name = key.strip_prefix(&format!("{provider}/")).unwrap_or(key);
        if row["mode"] != "chat"
            || name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b".-_".contains(&b))
            || row
                .as_object()?
                .keys()
                .any(|key| key.contains("_cost_per_token_above_"))
        {
            continue;
        }
        let Some(rates) = rates(row, provider) else {
            continue;
        };
        if prices
            .insert(name.to_owned(), rates)
            .is_some_and(|old| old != rates)
        {
            conflicts.insert(name);
        }
    }
    for name in conflicts {
        prices.remove(name);
    }
    (!prices.is_empty()).then(|| Arc::new(prices))
}

fn rates(row: &Value, provider: &str) -> Option<[i128; 4]> {
    let rate = |key: &str| {
        let value = row[key].as_f64()?;
        (0.0..=1.0)
            .contains(&value)
            .then(|| (value * PICOS_PER_UNIT).round() as i128)
    };
    let input = rate("input_cost_per_token")?;
    let output = rate("output_cost_per_token")?;
    let write_key = if provider == "anthropic" {
        "cache_creation_input_token_cost_above_1hr"
    } else {
        "cache_creation_input_token_cost"
    };
    let optional = |key: &str, default| {
        if row.get(key).is_none() {
            Some(default)
        } else {
            rate(key)
        }
    };
    Some([
        input,
        output,
        optional("cache_read_input_token_cost", input)?,
        optional(
            write_key,
            if provider == "anthropic" {
                input * 2
            } else {
                input
            },
        )?,
    ])
}

pub(crate) fn set_external_prices(prices: ExternalPrices) -> bool {
    overrides(|held| {
        if Arc::ptr_eq(&held.external, &prices) {
            return false;
        }
        let changed = held.external != prices;
        held.external = prices;
        changed
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing::{price_quote, Origin};
    use serde_json::json;

    #[test]
    fn external_prices_only_fill_unlisted_direct_models() {
        let chat = json!({"litellm_provider":"openai","mode":"chat",
            "input_cost_per_token":0.000002,"output_cost_per_token":0.00001});
        let mut tiered = chat.clone();
        tiered["input_cost_per_token_above_272k_tokens"] = json!(0.000004);
        let mut conflicting = chat.clone();
        conflicting["input_cost_per_token"] = json!(0.000003);
        let feed = json!({"openai/gpt-test":chat,"gpt-6-sol":chat,
            "gpt-tiered":tiered,"openrouter/gpt-routed":chat,
            "gpt-conflict":chat,"openai/gpt-conflict":conflicting});
        let prices = parse_external_prices(&feed).unwrap();
        assert!(set_external_prices(prices.clone()));
        assert!(!set_external_prices(prices));
        assert_eq!(
            price_quote("codex-model:gpt-test"),
            Some((Origin::External, "USD".into(), [2.0, 10.0, 2.0, 2.0]))
        );
        assert_eq!(
            price_quote("codex-model:gpt-6-sol").unwrap().0,
            Origin::Catalog
        );
        for name in [
            "gpt-tiered",
            "gpt-routed",
            "gpt-conflict",
            "gpt-test#relay.example",
        ] {
            assert!(
                price_quote(&format!("codex-model:{name}")).is_none(),
                "{name}"
            );
        }
    }
}
