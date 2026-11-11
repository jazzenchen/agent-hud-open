//! DeepSeek API keys other clients already send to DeepSeek's own API:
//! Claude Code's `ANTHROPIC_BASE_URL` (`https://api.deepseek.com/anthropic`),
//! Codex's model providers whose `base_url` is on `api.deepseek.com`, and
//! Pi's `deepseek` provider or one whose `baseUrl` is on that host.
//! Each key's balance is read at DeepSeek's fixed balance endpoint and
//! nowhere else; the key is never kept or shown.
use crate::account::{resolve, Host};
use crate::endpoints::base_url_host;
use crate::{claude, codex, Location, Look};
use serde_json::Value;

/// DeepSeek's own API host.
const HOST: &str = "api.deepseek.com";
/// At most this many keys are read, one after another.
const MOST: usize = 8;
const PI_MODELS: Location = Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "models.json");
const PI_AUTH: Location = Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "auth.json");
/// A settings file's size limit.
const LIMIT: u64 = 1024 * 1024;

/// One key, and the clients (by id) that send it to DeepSeek.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Routed {
    pub key: String,
    pub clients: Vec<&'static str>,
}

/// Whether a base URL is DeepSeek's own API, on any path.
fn official(base_url: &str) -> bool {
    base_url_host(base_url).as_deref() == Some(HOST)
}

/// A key as written, when it is one: not empty, not a command that looks
/// one up (`!…`), on one line.
fn usable(key: Option<&str>) -> Option<&str> {
    key.map(str::trim).filter(|key| {
        !key.is_empty()
            && !key.starts_with('!')
            && !key.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n'))
    })
}

/// Whether a Pi provider is DeepSeek's own API: by the `baseUrl` its
/// `models.json` entry gives, and without one, by Pi's own `deepseek` id.
fn pi_official(id: &str, base: Option<&str>) -> bool {
    base.map_or(id == "deepseek", official)
}

/// The API keys in Pi's `auth.json` whose providers are DeepSeek's own API,
/// as written: the key, or the name of the variable that holds it.
fn pi_keys(auth: &Value, models: &Value) -> Vec<String> {
    auth.as_object()
        .into_iter()
        .flatten()
        .filter(|(id, entry)| {
            entry["type"] == "api_key"
                && pi_official(id, models["providers"][id.as_str()]["baseUrl"].as_str())
        })
        .filter_map(|(_, entry)| entry["key"].as_str().map(str::to_owned))
        .collect()
}

/// Every key Claude Code, Codex and Pi send to DeepSeek's API here, each
/// once with the clients that send it.
pub(super) fn keys(host: &dyn Host) -> Vec<Routed> {
    let mut found = Vec::<Routed>::new();
    let mut add = |client: &'static str, key: Option<&str>| {
        let Some(key) = usable(key) else {
            return;
        };
        match found.iter_mut().find(|routed| routed.key == key) {
            Some(routed) if !routed.clients.contains(&client) => routed.clients.push(client),
            Some(_) => {}
            None => found.push(Routed {
                key: key.to_owned(),
                clients: vec![client],
            }),
        }
    };
    for route in claude::routes::configured(host) {
        if official(&route.base_url) {
            add("claude", route.key.as_deref());
        }
    }
    let env = |name: &str| host.env(name).map(str::to_owned);
    for provider in codex::providers::configured(host) {
        if official(&provider.base_url) {
            add("codex", provider.key(&env).as_deref());
        }
    }
    let settings = |location: &Location| -> Value {
        resolve(host, location)
            .into_iter()
            .next()
            .and_then(|path| host.read_file(&path, LIMIT).found())
            .filter(|bytes| bytes.len() as u64 <= LIMIT)
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .and_then(|text| json5::from_str(&text).ok())
            .unwrap_or(Value::Null)
    };
    for raw in pi_keys(&settings(&PI_AUTH), &settings(&PI_MODELS)) {
        add("pi", Some(&env(&raw).unwrap_or(raw)));
    }
    found.truncate(MOST);
    found
}

/// What says DeepSeek is configured through another client: Claude Code,
/// Codex or Pi sending a key to its API. Only whether a key is there is looked at.
pub fn configured(look: &dyn Look) -> Vec<&'static str> {
    let mut found = Vec::new();
    if claude::routes::seen(look)
        .iter()
        .any(|route| official(&route.base_url) && usable(route.key.as_deref()).is_some())
    {
        found.push("claude-provider-route");
    }
    let env = |name: &str| look.env(name).map(str::to_owned);
    if look.text(&codex::providers::CONFIG).is_some_and(|toml| {
        codex::providers::parse(&toml).iter().any(|provider| {
            official(&provider.base_url) && usable(provider.key(&env).as_deref()).is_some()
        })
    }) {
        found.push("codex-provider-route");
    }
    if pi_keys(&look.settings(&PI_AUTH), &look.settings(&PI_MODELS))
        .iter()
        .any(|key| usable(Some(key)).is_some())
    {
        found.push("pi-provider-route");
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    const CLAUDE: &str = r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.deepseek.com/anthropic","ANTHROPIC_AUTH_TOKEN":"sk-claude"}}"#;
    const CODEX: &str = "[model_providers.deepseek]\nname = \"DeepSeek\"\nbase_url = \"https://api.deepseek.com/v1\"\nenv_key = \"DEEPSEEK_KEY\"\n\n[model_providers.relay]\nbase_url = \"https://relay.example.com/v1\"\nexperimental_bearer_token = \"sk-relay\"\n";

    #[test]
    fn keys_sent_to_deepseeks_own_api_are_found_once_with_their_clients() {
        let host = FakeHost::new("/u")
            .file("/u/.claude/settings.json", CLAUDE)
            .file("/u/.codex/config.toml", CODEX)
            .env("DEEPSEEK_KEY", "sk-codex");
        assert_eq!(
            keys(&host),
            [
                Routed {
                    key: "sk-claude".into(),
                    clients: vec!["claude"]
                },
                Routed {
                    key: "sk-codex".into(),
                    clients: vec!["codex"]
                },
            ],
            "the relay's key is not DeepSeek's"
        );

        let host = FakeHost::new("/u")
            .file("/u/.claude/settings.local.json", CLAUDE)
            .file("/u/.codex/config.toml", CODEX)
            .env("DEEPSEEK_KEY", "sk-claude");
        assert_eq!(
            keys(&host),
            [Routed {
                key: "sk-claude".into(),
                clients: vec!["claude", "codex"]
            }]
        );
    }

    #[test]
    fn pis_deepseek_key_is_found_unless_its_provider_points_elsewhere() {
        let auth = r#"{"deepseek":{"type":"api_key","key":"sk-pi"},"mine":{"type":"api_key","key":"PI_DS"},"openai":{"type":"api_key","key":"sk-other"},"zai":{"type":"oauth","key":"sk-oauth"}}"#;
        let host = FakeHost::new("/u")
            .file("/u/.pi/agent/auth.json", auth)
            .file(
                "/u/.pi/agent/models.json",
                r#"{"providers":{"mine":{"baseUrl":"https://api.deepseek.com/v1"}}}"#,
            )
            .file("/u/.claude/settings.json", CLAUDE)
            .env("PI_DS", "sk-claude");
        assert_eq!(
            keys(&host),
            [
                Routed {
                    key: "sk-claude".into(),
                    clients: vec!["claude", "pi"]
                },
                Routed {
                    key: "sk-pi".into(),
                    clients: vec!["pi"]
                },
            ],
            "a key named by its variable is the variable's value"
        );

        let host = FakeHost::new("/u")
            .file("/u/.pi/agent/auth.json", auth)
            .file(
                "/u/.pi/agent/models.json",
                r#"{"providers":{"deepseek":{"baseUrl":"https://relay.example.com/v1"}}}"#,
            );
        assert!(
            keys(&host).is_empty(),
            "Pi's deepseek provider sent to a relay is not DeepSeek's API"
        );
    }

    #[test]
    fn other_hosts_and_missing_keys_are_not_deepseeks() {
        let host = FakeHost::new("/u")
            .env(
                "ANTHROPIC_BASE_URL",
                "https://api.deepseek.com.evil.example/anthropic",
            )
            .env("ANTHROPIC_AUTH_TOKEN", "sk")
            .file("/u/.codex/config.toml", CODEX);
        assert!(keys(&host).is_empty(), "no DEEPSEEK_KEY in the environment");
        let host = FakeHost::new("/u")
            .env("ANTHROPIC_BASE_URL", "https://api.deepseek.com/anthropic")
            .env("ANTHROPIC_AUTH_TOKEN", "!op read deepseek");
        assert!(keys(&host).is_empty());
    }
}
