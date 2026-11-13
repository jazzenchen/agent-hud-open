//! Relays (中转站) and their balances (docs/proposals/no-plan-usage.md §5),
//! one per address. A relay is found where a client keeps its address and
//! key: Claude Code's settings.json `env` or this process's environment,
//! Codex's `config.toml` providers, DeepSeek Harness's endpoint
//! (`settings.yaml` `llm-deepseek.baseURL`); or the owner adds it by hand, for any
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
mod balance;
mod new_api;

use crate::account::{Host, Provider, Reading, Unread};
use crate::endpoints::{base_url_host, host_kind, HostKind};
use balance::balance;
use serde_json::{json, Value};

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
    work_sources: &["claude", "codex", "deepseek"],
    ..Provider::new("relay", read)
};

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let mut reading = Reading::empty("relay");
    for target in targets(host)? {
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
fn targets(host: &dyn Host) -> Result<Vec<Target>, Unread> {
    let mut found = Vec::<Target>::new();
    let saved = host.saved_credential(MANUAL)?.unwrap_or(Value::Null);
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
    for route in crate::claude::routes::configured(host) {
        add("claude", Some(&route.base_url), route.key.as_deref(), None);
    }
    // Codex: the providers its config.toml defines; its calls are recorded
    // under the provider's id.
    for provider in crate::codex::providers::configured(host) {
        let key = provider.key(&|name| host.env(name).map(str::to_owned));
        add(
            "codex",
            Some(&provider.base_url),
            key.as_deref(),
            Some(&provider.id),
        );
    }
    // DeepSeek Harness: the endpoint its settings give, and its key.
    if let Some((base, key)) = crate::deepseek::settings::route(host) {
        add("deepseek", Some(&base), Some(&key), None);
    }
    Ok(found)
}

#[cfg(test)]
mod tests;
