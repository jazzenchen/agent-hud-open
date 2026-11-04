//! A relay's balance: the templates tried in turn, the one the owner describes,
//! and the answers' units and failures.
use super::new_api::{new_api, new_api_prices};
use super::{Balance, Found, Target};
use crate::account::{Host, HttpFailure, HttpRequest};
use crate::endpoints::base_url_host;
use crate::l10n;
use serde_json::{json, Value};
use std::time::Duration;

/// What the relay reports of the key's balance, and a New API site's price
/// list (`new_api_prices`), which it shows whatever the key's limit. Every
/// request goes to the relay's own address.
pub(super) fn balance(host: &dyn Host, target: &Target) -> Found {
    let ask = |url: &str, key: bool| {
        let authorization = format!("Bearer {}", target.key);
        let secrets: &[(&str, &str)] = if key {
            &[("Authorization", &authorization)]
        } else {
            &[]
        };
        host.http_json(&HttpRequest {
            url,
            secrets,
            headers: &[],
            body: None,
            timeout: Duration::from_secs(12),
        })
    };
    let kind = target.choice.kind.as_str();
    let known = |balance: Result<Balance, String>| Found {
        balance,
        prices: None,
        recognized: true,
    };
    if kind == "custom" {
        return known(custom_balance(host, target, &target.choice.custom));
    }
    if kind == "openrouter" || (kind == "auto" && target.host == "openrouter.ai") {
        return known(openrouter(target, &ask));
    }
    // A New API site says so on its public status page; one the owner named
    // as New API is read with the default units when the page is closed.
    let status = matches!(kind, "auto" | "newapi")
        .then(|| ask(&format!("{}/api/status", target.origin), false).ok())
        .flatten()
        .and_then(|status| {
            let per_unit = status["data"]["quota_per_unit"]
                .as_f64()
                .filter(|per| *per > 0.0)?;
            Some((status, per_unit))
        })
        .or_else(|| {
            (kind == "newapi").then(|| (json!({"data": {"quota_display_type": "USD"}}), 500_000.0))
        });
    if let Some((status, per_unit)) = status {
        let balance = new_api(&status["data"], per_unit, &target.host, &|url| {
            ask(&format!("{}{url}", target.origin), true)
        });
        let prices = ask(&format!("{}/api/pricing", target.origin), false)
            .ok()
            .and_then(|body| new_api_prices(&body, &status["data"]));
        return Found {
            balance,
            prices,
            recognized: true,
        };
    }
    let usage = ask(&format!("{}/v1/usage", target.origin), true);
    if let (Err(failed), "sub2api") = (&usage, kind) {
        return known(Err(failure(*failed)));
    }
    if let Ok(usage) = usage {
        let remaining = usage["remaining"]
            .as_f64()
            .or_else(|| usage["quota"]["remaining"].as_f64())
            .or_else(|| usage["balance"].as_f64());
        if let Some(amount) = remaining {
            let unit = usage["unit"]
                .as_str()
                .or_else(|| usage["quota"]["unit"].as_str())
                .unwrap_or("USD");
            let balance = currency(unit)
                .ok_or_else(unsupported_unit)
                .map(|currency| Balance {
                    kind: "sub2api",
                    label: target.host.clone(),
                    amount,
                    currency,
                });
            return known(balance);
        }
    }
    if kind == "sub2api" {
        return known(Err(l10n::text(
            "这个地址的 /v1/usage 没有返回余额",
            "This address's /v1/usage reports no balance.",
        )));
    }
    Found {
        balance: Err(l10n::text(
            "没认出这个中转站，可以手选类型",
            "Agent HUD could not tell what kind of relay this is; choose its type.",
        )),
        prices: None,
        recognized: false,
    }
}

/// A relay with its own balance endpoint: a GET of `url` (a path on the
/// relay, or an HTTPS address on its host) with the key as a bearer token or
/// in the named `header`, whose JSON holds what is left at `remaining` (a
/// path like `data.balance` or `data.items[0].left`), divided by `divisor`,
/// in `currency`.
fn custom_balance(host: &dyn Host, target: &Target, custom: &Value) -> Result<Balance, String> {
    let endpoint = custom["url"].as_str().unwrap_or("");
    let url = if endpoint.starts_with('/') {
        format!("{}{endpoint}", target.origin)
    } else if endpoint.starts_with("https://")
        && base_url_host(endpoint).as_deref() == Some(target.host.as_str())
    {
        endpoint.to_owned()
    } else {
        return Err(l10n::text(
            "余额接口必须在中转站自己的地址上",
            "The balance endpoint must be on the relay's own address.",
        ));
    };
    let bearer = format!("Bearer {}", target.key);
    let secret = match custom["header"].as_str().filter(|h| !h.is_empty()) {
        Some(header) if !header.eq_ignore_ascii_case("authorization") => {
            (header, target.key.as_str())
        }
        _ => ("Authorization", bearer.as_str()),
    };
    let body = host
        .http_json(&HttpRequest {
            url: &url,
            secrets: &[secret],
            headers: &[],
            body: None,
            timeout: Duration::from_secs(12),
        })
        .map_err(failure)?;
    let found = json_path(&body, custom["remaining"].as_str().unwrap_or("")).ok_or_else(|| {
        l10n::text(
            "余额接口的返回里没有找到这个字段",
            "The balance endpoint's answer has nothing at that path.",
        )
    })?;
    let amount = found
        .as_f64()
        .or_else(|| found.as_str().and_then(|text| text.trim().parse().ok()))
        .ok_or_else(|| {
            l10n::text(
                "余额接口返回的余额不是数字",
                "The balance endpoint's balance is not a number.",
            )
        })?;
    let divisor = custom["divisor"]
        .as_f64()
        .filter(|d| *d > 0.0)
        .unwrap_or(1.0);
    let currency =
        currency(custom["currency"].as_str().unwrap_or("USD")).ok_or_else(unsupported_unit)?;
    Ok(Balance {
        kind: "custom",
        label: target.host.clone(),
        amount: amount / divisor,
        currency,
    })
}

/// The value at a dotted path with optional indices (`data.items[0].left`).
pub(super) fn json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for part in path.split('.').filter(|part| !part.is_empty()) {
        let (name, indices) = part
            .split_once('[')
            .map_or((part, ""), |(name, rest)| (name, rest));
        if !name.is_empty() {
            current = current.get(name)?;
        }
        for index in indices.split('[').filter(|index| !index.is_empty()) {
            current = current.get(index.trim_end_matches(']').parse::<usize>().ok()?)?;
        }
    }
    (!current.is_null()).then_some(current)
}

/// OpenRouter, or a relay that answers like it at its own address: the
/// account's credits left, else the key's remaining limit.
fn openrouter(
    target: &Target,
    ask: &dyn Fn(&str, bool) -> Result<Value, HttpFailure>,
) -> Result<Balance, String> {
    let credits = ask(&format!("{}/api/v1/credits", target.origin), true).ok();
    let left = credits.as_ref().and_then(|body| {
        Some(body["data"]["total_credits"].as_f64()? - body["data"]["total_usage"].as_f64()?)
    });
    let amount = match left {
        Some(left) => left,
        None => {
            let key = ask(&format!("{}/api/v1/key", target.origin), true).map_err(failure)?;
            key["data"]["limit_remaining"].as_f64().ok_or_else(|| {
                l10n::text(
                    "这个 OpenRouter key 不限额度，读不到账户余额",
                    "This OpenRouter key has no limit, and the account's credits could not be read.",
                )
            })?
        }
    };
    Ok(Balance {
        kind: "openrouter",
        label: if target.host == "openrouter.ai" {
            "OpenRouter".into()
        } else {
            target.host.clone()
        },
        amount,
        currency: "USD",
    })
}

fn currency(unit: &str) -> Option<&'static str> {
    match unit.trim().to_ascii_uppercase().as_str() {
        "USD" | "$" => Some("USD"),
        "CNY" | "RMB" | "¥" => Some("CNY"),
        _ => None,
    }
}

fn unsupported_unit() -> String {
    l10n::text(
        "这个中转站的余额单位无法识别",
        "This relay reports its balance in a unit Agent HUD does not know.",
    )
}

pub(super) fn failure(failure: HttpFailure) -> String {
    match failure {
        HttpFailure::Unreachable | HttpFailure::BadHeader => l10n::text(
            "中转站余额读取失败，请检查网络后重试",
            "The relay's balance could not be reached; check the connection.",
        ),
        other => other.message(),
    }
}
