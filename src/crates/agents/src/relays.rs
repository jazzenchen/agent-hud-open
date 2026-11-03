//! Relays (中转站) and their balances (docs/proposals/no-plan-usage.md §5),
//! one per address. A relay is found where a client keeps its address and
//! key: Claude Code's settings.json `env` or this process's environment,
//! Codex's `config.toml` providers; or the owner adds it by hand, for any
//! agent. Which agents use it is the clients configured with it here, and
//! the core adds the agents whose calls go through it. Nothing is asked of a
//! relay found in a client's configuration until the owner agrees to it,
//! address by address (`relayReading` in Agent HUD's settings); then a key
//! a client already sends it is sent to that same address, and nowhere
//! else, to read its
//! balance. A few common templates are tried in turn: OpenRouter's credits,
//! a New API site (one-api / new-api, told by its public `/api/status`), a
//! Sub2API site's `/v1/usage`. When none fits, the owner may choose the
//! template for that address, or describe its balance endpoint
//! (`custom`); every request still goes only to the relay's own host.
use crate::account::{
    read_json, variable, Host, HttpFailure, HttpRequest, Provider, Reading, Unread,
};
use crate::endpoints::{base_url_host, host_kind, HostKind};
use crate::l10n;
use serde_json::{json, Value};
use std::time::Duration;

/// The setting that lists the relay addresses the owner agreed to read.
pub const CONSENT: &str = "relayReading";

/// The saved credential that holds the relays the owner added by hand and
/// the templates chosen by address (`Host::saved_credential`): `{sources:
/// [{id, client, name, baseUrl, key}], kinds: {host: {kind, custom}}}`. A
/// relay added by hand is read without asking again.
pub const MANUAL: &str = "relay";

/// The templates a relay can be read with; `auto` tries the others in turn.
pub const KINDS: [&str; 5] = ["auto", "newapi", "sub2api", "openrouter", "custom"];

/// The consent key `Host::consented` answers for one relay address.
pub fn consent_key(host: &str) -> String {
    format!("{CONSENT}:{host}")
}

/// One relay, by its address.
#[derive(Clone, Debug, PartialEq)]
struct Target {
    /// The relay's host, as calls are recorded under it.
    host: String,
    /// `https://host[:port]`, where its own endpoints are.
    origin: String,
    /// The key it is read with: the one added by hand, else the first
    /// client's.
    key: String,
    /// The clients configured with it (`claude`, `codex`).
    clients: Vec<String>,
    /// The routes its calls are recorded under: its host, the Codex
    /// providers that point at it, and the provider name other agents give
    /// a known one (`openrouter`).
    routes: Vec<String>,
    /// What the owner said of a relay added by hand.
    manual: Option<Manual>,
    /// The template it is read with.
    choice: Choice,
}

impl Target {
    fn new(host: String, key: String, kinds: &Value, legacy: Option<&Value>) -> Target {
        let mut routes = vec![host.clone()];
        if host == "openrouter.ai" {
            routes.push("openrouter".into());
        }
        Target {
            origin: format!("https://{host}"),
            choice: Choice::of(kinds, &host, legacy),
            host,
            key,
            clients: Vec::new(),
            routes,
            manual: None,
        }
    }
}

/// A relay the owner added by hand.
#[derive(Clone, Debug, PartialEq)]
struct Manual {
    id: String,
    /// Empty when the relay's own name is shown.
    name: String,
}

/// The template a relay is read with: one of `KINDS`, `auto` unless the
/// owner chose one for its address.
#[derive(Clone, Debug, PartialEq)]
struct Choice {
    kind: String,
    /// A custom relay's balance endpoint: `{url, remaining, divisor,
    /// currency, header}`.
    custom: Value,
}

impl Choice {
    /// The owner's choice for `relay` in `kinds`, else what a relay added by
    /// hand said before choices were kept by address, else `auto`.
    fn of(kinds: &Value, relay: &str, legacy: Option<&Value>) -> Choice {
        [Some(&kinds[relay]), legacy]
            .into_iter()
            .flatten()
            .find_map(|said| {
                let kind = said["kind"].as_str()?;
                (KINDS.contains(&kind) && kind != "auto").then(|| Choice {
                    kind: kind.to_owned(),
                    custom: said["custom"].clone(),
                })
            })
            .unwrap_or(Choice {
                kind: "auto".into(),
                custom: Value::Null,
            })
    }
}

/// A relay's balance as it reports it.
struct Balance {
    kind: &'static str,
    label: String,
    amount: f64,
    currency: &'static str,
}

/// What reading a relay found: its balance, a New API site's price list,
/// and whether any template fit it.
struct Found {
    balance: Result<Balance, String>,
    prices: Option<Value>,
    recognized: bool,
}

/// The relays the clients send their calls through. A relay is read sooner
/// while the clients it is found in work.
pub static PROVIDER: Provider = Provider {
    work_sources: &["claude", "codex"],
    ..Provider::new("relay", read)
};

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    read_text(host).map_err(Unread::from)
}

fn read_text(host: &dyn Host) -> Result<Reading, String> {
    let mut reading = Reading::empty("relay");
    for target in targets(host) {
        let consented = target.manual.is_some() || host.consented(&consent_key(&target.host));
        let mut service = json!({"provider": target.host, "product": "relay", "clients": target.clients, "routes": target.routes, "consented": consented});
        if let Some(manual) = &target.manual {
            service["manual"] = json!(true);
            service["id"] = json!(manual.id);
            if !manual.name.is_empty() {
                service["name"] = json!(manual.name);
            }
        }
        if target.choice.kind != "auto" {
            service["chosen"] = json!(target.choice.kind);
            if target.choice.kind == "custom" {
                service["custom"] = target.choice.custom.clone();
            }
        }
        if consented && !host.cancelled() {
            let read = balance(host, &target);
            if let Some(prices) = read.prices {
                service["kind"] = json!("newapi");
                service["prices"] = prices;
            }
            if !read.recognized {
                service["unrecognized"] = json!(true);
            }
            match read.balance {
                Ok(mut found) => {
                    service["kind"] = json!(found.kind);
                    if let Some(manual) = target.manual.as_ref().filter(|m| !m.name.is_empty()) {
                        found.label = manual.name.clone();
                    }
                    let amount = (found.amount * 100.0).round() / 100.0;
                    service["balance"] =
                        json!({"label": found.label, "amount": amount, "currency": found.currency});
                    // One balance per relay; the core shows it under every
                    // agent that uses it.
                    reading.billing.push(json!({
                        "id": format!("relay:{}", target.host),
                        "vendor": "relay",
                        "relay": target.host,
                        "clients": target.clients,
                        "consumerRoutes": target.routes,
                        "label": found.label,
                        "amount": amount,
                        "currency": found.currency,
                        "available": true,
                    }));
                }
                Err(notice) => service["notice"] = json!(notice),
            }
        }
        reading.services.push(service);
    }
    Ok(reading)
}

/// Every relay the owner added by hand, then every one the clients are
/// configured with, once per address with the clients that use it.
fn targets(host: &dyn Host) -> Vec<Target> {
    let mut found = Vec::<Target>::new();
    let saved = host
        .saved_credential(MANUAL)
        .ok()
        .flatten()
        .unwrap_or(Value::Null);
    let kinds = &saved["kinds"];
    for source in saved["sources"].as_array().into_iter().flatten() {
        let text = |key: &str| source[key].as_str().map(str::to_owned);
        let (Some(base), Some(key), Some(id)) = (text("baseUrl"), text("key"), text("id")) else {
            continue;
        };
        let Some(relay) = base_url_host(&base) else {
            continue;
        };
        if found.iter().any(|t| t.host == relay) {
            continue;
        }
        let mut target = Target::new(relay, key, kinds, Some(source));
        target.manual = Some(Manual {
            id,
            name: text("name").unwrap_or_default(),
        });
        found.push(target);
    }
    let mut add = |client: &'static str,
                   base: Option<&str>,
                   key: Option<&str>,
                   route: Option<&str>| {
        let (Some(base), Some(key)) = (base, key.map(str::trim).filter(|key| !key.is_empty()))
        else {
            return;
        };
        let Some(relay) = base_url_host(base).filter(|relay| host_kind(relay) == HostKind::Relay)
        else {
            return;
        };
        if key.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n')) {
            return;
        }
        let at = match found.iter().position(|t| t.host == relay) {
            Some(at) => at,
            None => {
                found.push(Target::new(relay, key.to_owned(), kinds, None));
                found.len() - 1
            }
        };
        let target = &mut found[at];
        if !target.clients.iter().any(|known| known == client) {
            target.clients.push(client.to_owned());
        }
        if let Some(route) = route.filter(|route| !target.routes.iter().any(|known| known == route))
        {
            target.routes.push(route.to_owned());
        }
    };
    // Claude Code: its settings' env, then this process's own environment.
    let claude = variable(host, "CLAUDE_CONFIG_DIR").unwrap_or_else(|| host.home().join(".claude"));
    for name in ["settings.json", "settings.local.json"] {
        if let Some(settings) = read_json(host, &claude.join(name)) {
            let env = &settings["env"];
            let key = env["ANTHROPIC_AUTH_TOKEN"]
                .as_str()
                .or_else(|| env["ANTHROPIC_API_KEY"].as_str());
            add("claude", env["ANTHROPIC_BASE_URL"].as_str(), key, None);
        }
    }
    let key = host
        .env("ANTHROPIC_AUTH_TOKEN")
        .or_else(|| host.env("ANTHROPIC_API_KEY"));
    add("claude", host.env("ANTHROPIC_BASE_URL"), key, None);
    // Codex: the providers its config.toml defines; its calls are recorded
    // under the provider's id.
    let codex = variable(host, "CODEX_HOME").unwrap_or_else(|| host.home().join(".codex"));
    if let Some(bytes) = host
        .read_file(&codex.join("config.toml"), 1024 * 1024)
        .found()
    {
        for provider in codex_providers(&String::from_utf8_lossy(&bytes)) {
            let key = provider.bearer.clone().or_else(|| {
                provider
                    .env_key
                    .as_deref()
                    .and_then(|name| host.env(name))
                    .map(str::to_owned)
            });
            add(
                "codex",
                Some(&provider.base_url),
                key.as_deref(),
                Some(&provider.id),
            );
        }
    }
    found
}

/// One `[model_providers.<id>]` table of a Codex `config.toml`.
#[derive(Debug, Default, PartialEq)]
struct CodexProvider {
    /// The table's name, which its calls are recorded under.
    id: String,
    base_url: String,
    env_key: Option<String>,
    bearer: Option<String>,
}

/// The model providers a Codex `config.toml` defines: each
/// `[model_providers.<id>]` table's `base_url`, `env_key` and
/// `experimental_bearer_token`, read as plain string values.
fn codex_providers(toml: &str) -> Vec<CodexProvider> {
    let mut providers = Vec::new();
    let mut current: Option<CodexProvider> = None;
    let mut flush = |current: &mut Option<CodexProvider>| {
        if let Some(provider) = current.take().filter(|p| !p.base_url.is_empty()) {
            providers.push(provider);
        }
    };
    for line in toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            flush(&mut current);
            let table = line.trim_start_matches('[').trim_end_matches(']').trim();
            if let Some(id) = table.strip_prefix("model_providers.") {
                let id = id.trim().trim_matches('"');
                if !id.contains('.') && !id.is_empty() {
                    current = Some(CodexProvider {
                        id: id.to_owned(),
                        ..CodexProvider::default()
                    });
                }
            }
            continue;
        }
        let Some(provider) = current.as_mut() else {
            continue;
        };
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let Some(value) = value
            .strip_prefix('"')
            .and_then(|rest| rest.split_once('"'))
            .map(|(text, _)| text.to_owned())
        else {
            continue;
        };
        match name.trim() {
            "base_url" => provider.base_url = value,
            "env_key" => provider.env_key = Some(value),
            "experimental_bearer_token" => provider.bearer = Some(value),
            _ => {}
        }
    }
    flush(&mut current);
    providers
}

/// What the relay reports of the key's balance, and a New API site's price
/// list (`new_api_prices`), which it shows whatever the key's limit. Every
/// request goes to the relay's own address.
fn balance(host: &dyn Host, target: &Target) -> Found {
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
        .filter(|status| {
            status["data"]["quota_per_unit"]
                .as_f64()
                .is_some_and(|per| per > 0.0)
        })
        .or_else(|| {
            (kind == "newapi")
                .then(|| json!({"data": {"quota_per_unit": 500000, "quota_display_type": "USD"}}))
        });
    if let Some(status) = status {
        let balance = new_api(&status["data"], &target.host, &|url| {
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
fn json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
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

/// A New API site's price list, in what calls through it cost per million
/// tokens in the money the site shows: `{currency, models: {name: [input,
/// output, cache read, cache write]}}`. A ratio of 1 is $2 per million input
/// tokens; output is input times the completion ratio, cache reads and writes
/// input times their ratios, all times the group's ratio. Models priced per
/// call or by a billing script are left out, and so is everything when the
/// group cannot be told (several groups, none called `default`).
fn new_api_prices(body: &Value, status: &Value) -> Option<Value> {
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
fn new_api(
    status: &Value,
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
        return Err(l10n::text(
            "这个 key 不限额度，站点也没有开放账户余额",
            "This key has no spending limit and the site does not share the account balance.",
        ));
    };
    let per_unit = status["quota_per_unit"].as_f64().unwrap_or(500_000.0);
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
        _ => {
            return Err(l10n::text(
                "这个中转站以 Token 计额度，没有金额",
                "This relay counts its quota in tokens, not money.",
            ))
        }
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

fn failure(failure: HttpFailure) -> String {
    match failure {
        HttpFailure::Unreachable | HttpFailure::BadHeader => l10n::text(
            "中转站余额读取失败，请检查网络后重试",
            "The relay's balance could not be reached; check the connection.",
        ),
        other => other.message(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;
    use std::sync::atomic::Ordering;

    const CLAUDE: &str = "/u/.claude/settings.json";

    fn claude_relay(host: FakeHost, base: &str) -> FakeHost {
        host.file(
            CLAUDE,
            &json!({"env": {"ANTHROPIC_BASE_URL": base, "ANTHROPIC_AUTH_TOKEN": "sk-relay"}})
                .to_string(),
        )
    }

    #[test]
    fn a_relay_is_listed_but_not_asked_before_consent() {
        let host = claude_relay(FakeHost::new("/u"), "https://relay.example.com/api");
        let reading = read(&host).unwrap();
        assert!(reading.billing.is_empty());
        assert!(host.asked.borrow().is_empty(), "nothing is sent");
        assert_eq!(
            reading.services,
            [
                json!({"provider": "relay.example.com", "product": "relay", "clients": ["claude"], "routes": ["relay.example.com"], "consented": false})
            ]
        );
    }

    #[test]
    fn official_and_vendor_addresses_are_no_relays() {
        let host = claude_relay(FakeHost::new("/u"), "https://api.z.ai/api/anthropic")
            .env("ANTHROPIC_BASE_URL", "https://api.anthropic.com")
            .env("ANTHROPIC_API_KEY", "sk-ant");
        assert!(read(&host).unwrap().services.is_empty());
    }

    #[test]
    fn a_new_api_site_reports_the_balance_in_the_money_it_shows() {
        let status = |shown: &str| json!({"success": true, "data": {"system_name": "Relay One", "quota_per_unit": 500000, "quota_display_type": shown, "usd_exchange_rate": 7.0}});
        let usage = json!({"code": true, "data": {"total_available": 2_500_000, "total_used": 100, "unlimited_quota": false}});
        let host = claude_relay(
            FakeHost::new("/u"),
            "https://relay.example.com/api/anthropic",
        )
        .answer("https://relay.example.com/api/status", Ok(status("USD")))
        .answer(
            "https://relay.example.com/api/usage/token/",
            Ok(usage.clone()),
        );
        host.consent.store(true, Ordering::Release);
        let reading = read(&host).unwrap();
        assert_eq!(reading.billing.len(), 1);
        let row = &reading.billing[0];
        assert_eq!(
            (&row["id"], &row["vendor"], &row["relay"], &row["clients"]),
            (
                &json!("relay:relay.example.com"),
                &json!("relay"),
                &json!("relay.example.com"),
                &json!(["claude"])
            ),
            "one balance per relay, shown by the core under its agents"
        );
        assert_eq!(row["consumerRoutes"], json!(["relay.example.com"]));
        assert_eq!(row["label"], "Relay One");
        assert_eq!(
            reading.services[0]["balance"],
            json!({"label": "Relay One", "amount": 5.0, "currency": "USD"})
        );
        assert_eq!(row["amount"], 5.0);
        assert_eq!(row["currency"], "USD");
        assert_eq!(reading.services[0]["kind"], "newapi");
        let asked = host.asked.borrow();
        assert_eq!(
            asked[0],
            ("https://relay.example.com/api/status".into(), None),
            "the status page is public"
        );
        assert_eq!(
            asked[1],
            (
                "https://relay.example.com/api/usage/token/".into(),
                Some("Bearer sk-relay".into())
            )
        );
        drop(asked);

        let cny = claude_relay(FakeHost::new("/u"), "https://relay.example.com")
            .answer("https://relay.example.com/api/status", Ok(status("CNY")))
            .answer(
                "https://relay.example.com/api/usage/token/",
                Ok(json!({"data": {"total_available": 0, "unlimited_quota": true, "user_quota": {"total_available": 1_000_000, "total_used": 5}}})),
            );
        cny.consent.store(true, Ordering::Release);
        let row = &read(&cny).unwrap().billing[0];
        assert_eq!(
            (row["amount"].as_f64(), row["currency"].as_str()),
            (Some(14.0), Some("CNY")),
            "the account's balance, in yuan"
        );
    }

    #[test]
    fn a_new_api_sites_price_list_is_read_in_the_money_it_shows() {
        let pricing = json!({"success": true, "group_ratio": {"default": 0.5, "vip": 0.3}, "data": [
            {"model_name": "claude-sonnet-4-5", "quota_type": 0, "model_ratio": 1.5, "completion_ratio": 5.0, "cache_ratio": 0.1, "create_cache_ratio": 1.25},
            {"model_name": "gpt-6-sol", "quota_type": 0, "model_ratio": 0.625, "completion_ratio": 8.0},
            {"model_name": "image-gen", "quota_type": 1, "model_price": 0.04, "model_ratio": 0}
        ]});
        let host = claude_relay(FakeHost::new("/u"), "https://relay.example.com")
            .answer(
                "https://relay.example.com/api/status",
                Ok(json!({"data": {"quota_per_unit": 500000, "quota_display_type": "USD"}})),
            )
            .answer("https://relay.example.com/api/pricing", Ok(pricing));
        host.consent.store(true, Ordering::Release);
        let reading = read(&host).unwrap();
        let service = &reading.services[0];
        assert_eq!(
            service["kind"], "newapi",
            "known even though the key's usage could not be read"
        );
        assert!(service["notice"].is_string());
        assert_eq!(service["prices"]["currency"], "USD");
        assert_eq!(
            service["prices"]["models"]["claude-sonnet-4-5"],
            json!([1.5, 7.5, 0.15000000000000002, 1.875])
        );
        assert_eq!(
            service["prices"]["models"]["gpt-6-sol"],
            json!([0.625, 5.0, 0.625, 0.625])
        );
        assert!(
            service["prices"]["models"]["image-gen"].is_null(),
            "priced per call"
        );
        let asked = host.asked.borrow();
        assert!(
            asked
                .iter()
                .any(|(url, auth)| url.ends_with("/api/pricing") && auth.is_none()),
            "the price list is public"
        );
    }

    #[test]
    fn a_sub2api_site_and_openrouter_report_what_is_left() {
        let sub2api = claude_relay(FakeHost::new("/u"), "https://pool.example.net").answer(
            "https://pool.example.net/v1/usage",
            Ok(json!({"remaining": 12.34, "unit": "USD", "is_active": true})),
        );
        sub2api.consent.store(true, Ordering::Release);
        let reading = read(&sub2api).unwrap();
        assert_eq!(reading.billing[0]["amount"], 12.34);
        assert_eq!(reading.services[0]["kind"], "sub2api");

        let openrouter = claude_relay(FakeHost::new("/u"), "https://openrouter.ai/api").answer(
            "https://openrouter.ai/api/v1/credits",
            Ok(json!({"data": {"total_credits": 20.0, "total_usage": 7.5}})),
        );
        openrouter.consent.store(true, Ordering::Release);
        let row = &read(&openrouter).unwrap().billing[0];
        assert_eq!(
            (row["amount"].as_f64(), row["label"].as_str()),
            (Some(12.5), Some("OpenRouter"))
        );
    }

    #[test]
    fn a_relay_added_by_hand_is_read_without_asking_again() {
        let mut host = FakeHost::new("/u").answer(
            "https://m.example.com/dashboard/balance",
            Ok(json!({"data": {"items": [{"left": "1234"}]}})),
        );
        host.saved = Ok(Some(json!({"sources": [{
            "id": "relay-1", "client": "claude", "name": "My relay", "baseUrl": "https://m.example.com/v1",
            "key": "sk-m"
        }], "kinds": {"m.example.com": {"kind": "custom",
            "custom": {"url": "/dashboard/balance", "remaining": "data.items[0].left", "divisor": 100.0, "currency": "CNY", "header": "x-api-key"}
        }}})));
        let reading = read(&host).unwrap();
        let row = &reading.billing[0];
        assert_eq!(
            (
                row["label"].as_str(),
                row["amount"].as_f64(),
                row["currency"].as_str(),
                row["vendor"].as_str()
            ),
            (Some("My relay"), Some(12.34), Some("CNY"), Some("relay"))
        );
        let service = &reading.services[0];
        assert_eq!(
            (service["manual"].as_bool(), service["consented"].as_bool()),
            (Some(true), Some(true))
        );
        assert_eq!(
            service["clients"],
            json!([]),
            "added by hand for no agent in particular"
        );
        assert_eq!(
            (
                service["id"].as_str(),
                service["kind"].as_str(),
                service["chosen"].as_str()
            ),
            (Some("relay-1"), Some("custom"), Some("custom"))
        );
        assert_eq!(service["custom"]["remaining"], "data.items[0].left");
        let sent = host.sent.borrow();
        assert_eq!(
            sent[0].0,
            [("x-api-key".to_string(), "sk-m".to_string())],
            "the key in the named header"
        );
    }

    #[test]
    fn a_relay_added_by_hand_without_a_name_shows_its_own() {
        let mut host = FakeHost::new("/u")
            .answer(
                "https://named.example.com/api/status",
                Ok(json!({"data": {"system_name": "Named", "quota_per_unit": 500000, "quota_display_type": "USD"}})),
            )
            .answer(
                "https://named.example.com/api/usage/token/",
                Ok(json!({"data": {"total_available": 500_000, "unlimited_quota": false}})),
            );
        host.saved = Ok(Some(json!({"sources": [{
            "id": "relay-3", "client": "claude", "name": "", "baseUrl": "https://named.example.com", "key": "sk-n"
        }]})));
        let reading = read(&host).unwrap();
        assert_eq!(reading.billing[0]["label"], "Named");
        assert!(reading.services[0]["name"].is_null());
        assert!(reading.services[0]["chosen"].is_null());
    }

    #[test]
    fn a_type_chosen_for_an_address_applies_to_the_relay_a_client_uses() {
        let mut host = claude_relay(FakeHost::new("/u"), "https://pool.example.net/v1").answer(
            "https://pool.example.net/v1/usage",
            Ok(json!({"quota": {"remaining": 3.5, "unit": "CNY"}})),
        );
        host.saved = Ok(Some(
            json!({"sources": [], "kinds": {"pool.example.net": {"kind": "sub2api"}}}),
        ));
        host.consent.store(true, Ordering::Release);
        let reading = read(&host).unwrap();
        assert_eq!(
            (
                reading.billing[0]["amount"].as_f64(),
                reading.billing[0]["currency"].as_str()
            ),
            (Some(3.5), Some("CNY"))
        );
        assert_eq!(reading.services[0]["chosen"], "sub2api");
        let asked = host.asked.borrow();
        assert_eq!(asked.len(), 1, "no sniffing: {asked:?}");
    }

    #[test]
    fn every_template_asks_only_the_relays_own_address() {
        let mut host = claude_relay(FakeHost::new("/u"), "https://gate.example.com/api").answer(
            "https://gate.example.com/api/v1/credits",
            Ok(json!({"data": {"total_credits": 9.0, "total_usage": 4.0}})),
        );
        host.saved = Ok(Some(
            json!({"sources": [], "kinds": {"gate.example.com": {"kind": "openrouter"}}}),
        ));
        host.consent.store(true, Ordering::Release);
        let row = &read(&host).unwrap().billing[0];
        assert_eq!(
            (row["amount"].as_f64(), row["label"].as_str()),
            (Some(5.0), Some("gate.example.com"))
        );
        assert!(host
            .asked
            .borrow()
            .iter()
            .all(|(url, _)| url.starts_with("https://gate.example.com/")));

        let mut elsewhere = claude_relay(FakeHost::new("/u"), "https://gate.example.com");
        elsewhere.saved = Ok(Some(
            json!({"sources": [], "kinds": {"gate.example.com": {"kind": "custom",
            "custom": {"url": "https://collector.example.org/balance", "remaining": "left"}}}}),
        ));
        elsewhere.consent.store(true, Ordering::Release);
        let reading = read(&elsewhere).unwrap();
        assert!(reading.billing.is_empty());
        assert_eq!(
            reading.services[0]["notice"],
            "The balance endpoint must be on the relay's own address."
        );
        assert!(elsewhere.asked.borrow().is_empty(), "the key stays home");
    }

    #[test]
    fn a_new_api_relay_added_by_hand_reads_with_default_units_when_its_status_is_closed() {
        let mut host = FakeHost::new("/u").answer(
            "https://closed.example.com/api/usage/token/",
            Ok(json!({"data": {"total_available": 1_000_000, "unlimited_quota": false}})),
        );
        host.saved = Ok(Some(json!({"sources": [{
            "id": "relay-2", "client": "codex", "name": "Closed", "baseUrl": "https://closed.example.com",
            "key": "sk-c", "kind": "newapi"
        }]})));
        let row = &read(&host).unwrap().billing[0];
        assert_eq!(
            (
                row["amount"].as_f64(),
                row["currency"].as_str(),
                row["vendor"].as_str()
            ),
            (Some(2.0), Some("USD"), Some("relay"))
        );
    }

    #[test]
    fn a_json_path_reaches_into_objects_and_lists() {
        let value = json!({"data": {"items": [{"left": 3}, {"left": 4}]}, "balance": 7});
        assert_eq!(json_path(&value, "balance"), Some(&json!(7)));
        assert_eq!(json_path(&value, "data.items[1].left"), Some(&json!(4)));
        assert_eq!(json_path(&value, "data.items[5].left"), None);
        assert_eq!(json_path(&value, "missing"), None);
    }

    #[test]
    fn an_unknown_relay_says_so_without_a_balance() {
        let host = claude_relay(FakeHost::new("/u"), "https://odd.example.org");
        host.consent.store(true, Ordering::Release);
        let reading = read(&host).unwrap();
        assert!(reading.billing.is_empty());
        assert_eq!(
            reading.services[0]["notice"],
            "Agent HUD could not tell what kind of relay this is; choose its type."
        );
        assert_eq!(reading.services[0]["unrecognized"], true);
    }

    #[test]
    fn codex_providers_come_from_its_config() {
        let toml = r#"
model = "gpt-6-sol"
model_provider = "relay"

[model_providers.relay]
name = "My relay"
base_url = "https://relay.example.com/v1"
env_key = "RELAY_KEY"

[model_providers.inline]
base_url = "https://other.example.com/v1" # comment
experimental_bearer_token = "sk-inline"

[profiles.fast]
base_url = "https://ignored.example.com"
"#;
        assert_eq!(
            codex_providers(toml),
            [
                CodexProvider {
                    id: "relay".into(),
                    base_url: "https://relay.example.com/v1".into(),
                    env_key: Some("RELAY_KEY".into()),
                    bearer: None
                },
                CodexProvider {
                    id: "inline".into(),
                    base_url: "https://other.example.com/v1".into(),
                    env_key: None,
                    bearer: Some("sk-inline".into())
                },
            ]
        );
        let host = FakeHost::new("/u")
            .file("/u/.codex/config.toml", toml)
            .env("RELAY_KEY", "sk-env");
        let listed = read(&host).unwrap().services;
        let hosts: Vec<_> = listed
            .iter()
            .map(|s| {
                (
                    s["provider"].clone(),
                    s["clients"].clone(),
                    s["routes"].clone(),
                )
            })
            .collect();
        assert_eq!(
            hosts,
            [
                (
                    json!("relay.example.com"),
                    json!(["codex"]),
                    json!(["relay.example.com", "relay"])
                ),
                (
                    json!("other.example.com"),
                    json!(["codex"]),
                    json!(["other.example.com", "inline"])
                )
            ]
        );
    }

    #[test]
    fn a_relay_both_clients_use_is_read_once() {
        let host = claude_relay(FakeHost::new("/u"), "https://pool.example.net/api")
            .file(
                "/u/.codex/config.toml",
                "[model_providers.pool]\nbase_url = \"https://pool.example.net/v1\"\nexperimental_bearer_token = \"sk-codex\"\n",
            )
            .answer(
                "https://pool.example.net/v1/usage",
                Ok(json!({"remaining": 8.0, "unit": "USD"})),
            );
        host.consent.store(true, Ordering::Release);
        let reading = read(&host).unwrap();
        assert_eq!(reading.services.len(), 1);
        assert_eq!(reading.services[0]["clients"], json!(["claude", "codex"]));
        assert_eq!(
            reading.billing[0]["consumerRoutes"],
            json!(["pool.example.net", "pool"])
        );
        let asked = host.asked.borrow();
        assert_eq!(
            asked
                .iter()
                .filter(|(url, _)| url.ends_with("/v1/usage"))
                .map(|(_, key)| key.clone())
                .collect::<Vec<_>>(),
            [Some("Bearer sk-relay".to_string())],
            "once, with the first client's key"
        );
    }
}
