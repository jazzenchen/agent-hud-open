//! A New API site's balance (one-api / new-api), told by its public `/api/status`.
use super::balance::failure;
use super::Balance;
use crate::account::HttpFailure;
use crate::l10n::tr;
use serde_json::{json, Value};

/// A New API site's price list, in what calls through it cost per million
/// tokens in the money the site shows: `{currency, models: {name: [input,
/// output, cache read, cache write]}}`. A ratio of 1 is $2 per million input
/// tokens; output is input times the completion ratio, cache reads and writes
/// input times their ratios, all times the group's ratio. Models priced per
/// call or by a billing script are left out, and so is everything when the
/// group cannot be told (several groups, none called `default`).
pub(super) fn new_api_prices(body: &Value, status: &Value) -> Option<Value> {
    let shown = status["quota_display_type"]
        .as_str()
        .map(str::to_ascii_uppercase);
    let (rate, currency) = match shown.as_deref() {
        Some("CNY") => (status["usd_exchange_rate"].as_f64()?, "CNY"),
        Some("USD") => (1.0, "USD"),
        None if status["display_in_currency"] != false => (1.0, "USD"),
        _ => return None,
    };
    let group = match body["group_ratio"].as_object() {
        None => 1.0,
        Some(groups) if groups.is_empty() => 1.0,
        Some(groups) if groups.len() == 1 => groups.values().next()?.as_f64()?,
        Some(groups) => groups.get("default")?.as_f64()?,
    };
    let mut models = serde_json::Map::new();
    for item in body["data"].as_array()? {
        let (Some(name), Some(ratio)) = (item["model_name"].as_str(), item["model_ratio"].as_f64())
        else {
            continue;
        };
        let scripted = item["billing_expr"]
            .as_str()
            .is_some_and(|expr| !expr.is_empty());
        if item["quota_type"].as_i64() != Some(0) || scripted || ratio <= 0.0 {
            continue;
        }
        let input = ratio * 2.0 * group * rate;
        let times = |key: &str| item[key].as_f64().unwrap_or(1.0);
        models.insert(
            name.to_owned(),
            json!([
                input,
                input * times("completion_ratio"),
                input * times("cache_ratio"),
                input * times("create_cache_ratio")
            ]),
        );
    }
    (!models.is_empty()).then(|| json!({"currency": currency, "models": models}))
}

/// A New API site: the key's account balance when the site lets the key read
/// it, else what the key may still spend, in the money the site shows.
pub(super) fn new_api(
    status: &Value,
    per_unit: f64,
    relay: &str,
    ask: &dyn Fn(&str) -> Result<Value, HttpFailure>,
) -> Result<Balance, String> {
    let usage = ask("/api/usage/token/").map_err(failure)?;
    let data = &usage["data"];
    let quota = data["user_quota"]["total_available"].as_f64().or_else(|| {
        (data["unlimited_quota"] != true)
            .then(|| data["total_available"].as_f64())
            .flatten()
    });
    let Some(quota) = quota else {
        return Err(tr!(
            "This key has no spending limit and the site does not share the account balance."
        ));
    };
    let shown = status["quota_display_type"]
        .as_str()
        .map(str::to_ascii_uppercase)
        .unwrap_or_else(|| {
            if status["display_in_currency"] == false {
                "TOKENS".into()
            } else {
                "USD".into()
            }
        });
    let (amount, currency) = match shown.as_str() {
        "CNY" => (
            quota / per_unit * status["usd_exchange_rate"].as_f64().unwrap_or(1.0),
            "CNY",
        ),
        "USD" => (quota / per_unit, "USD"),
        _ => return Err(tr!("This relay counts its quota in tokens, not money.")),
    };
    let name = status["system_name"]
        .as_str()
        .map(str::trim)
        .filter(|name| !name.is_empty() && name.chars().count() <= 40);
    Ok(Balance {
        kind: "newapi",
        label: name.unwrap_or(relay).to_owned(),
        amount,
        currency,
    })
}
