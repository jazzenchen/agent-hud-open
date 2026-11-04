//! MiniMax plan keys other clients already hold: the environment, Claude
//! Code's `ANTHROPIC_BASE_URL`, Codex's model providers, OpenCode's and Pi's
//! sign-ins, wherever they send a key to MiniMax's own API. Discovery only
//! reads: no key resolver runs, no token is refreshed, nobody signs in.
use crate::account::{resolve, Host};
use crate::codex::providers::ModelProvider;
use crate::endpoints::base_url_host;
use crate::{claude, codex, Location, Look};
use serde_json::Value;

/// The variables a MiniMax key is kept in: MiniMax's own names for a plan
/// key and an API key, which OpenCode and Pi read too, and Pi's name for its
/// China provider (`minimax-cn`).
pub(super) const VARIABLES: &[&str] = &[
    "MINIMAX_CODING_API_KEY",
    "MINIMAX_API_KEY",
    "MINIMAX_CN_API_KEY",
];

const OPENCODE_CONFIGS: [Location; 2] = [
    Location::config_home(&["opencode"], "opencode.json"),
    Location::config_home(&["opencode"], "opencode.jsonc"),
];
const OPENCODE_AUTH: Location = Location::data_home(&["opencode"], "auth.json");
const PI_MODELS: Location = Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "models.json");
const PI_AUTH: Location = Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "auth.json");
/// A settings file's size limit.
const LIMIT: u64 = 1024 * 1024;
/// At most this many keys are read, one after another.
const MOST: usize = 8;

/// Which of MiniMax's two platforms a key belongs to: keys of one are
/// refused by the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Region {
    /// minimax.io.
    Global,
    /// minimax.cn, formerly minimaxi.com.
    China,
}
impl Region {
    /// The platform a MiniMax host belongs to; None for any other host.
    fn of_host(host: &str) -> Option<Region> {
        match host {
            "api.minimax.io" | "www.minimax.io" => Some(Region::Global),
            "api.minimax.cn" | "www.minimax.cn" | "api.minimaxi.com" | "www.minimaxi.com" => {
                Some(Region::China)
            }
            _ => None,
        }
    }
    /// The platform a base URL is on, when it is MiniMax's.
    fn of_url(url: &str) -> Option<Region> {
        Region::of_host(&base_url_host(url)?)
    }
    /// As price lists and readings name it.
    pub(super) fn name(self) -> &'static str {
        match self {
            Region::Global => "international",
            Region::China => "china",
        }
    }
}

/// One key, the platform the clients send it to (None when only the
/// environment holds it, or two clients disagree), the clients that send
/// it, by their names, and the routes their calls with it are logged under
/// (`client#provider id`; Claude Code's calls have none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Key {
    pub key: String,
    pub region: Option<Region>,
    pub clients: Vec<&'static str>,
    pub routes: Vec<String>,
}

/// A key as written, when it can read a plan: not empty, not a command that
/// looks one up (`!…`), on one line, and not a pay-as-you-go API key
/// (`sk-api-`), which MiniMax keeps apart from plan keys (`sk-cp-`).
fn usable(key: Option<&str>) -> Option<&str> {
    key.map(str::trim).filter(|key| {
        !key.is_empty()
            && !key.starts_with('!')
            && !key.starts_with("sk-api-")
            && !key.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n'))
    })
}

/// The files and variables the clients keep, however they are read.
struct Machine<'a> {
    env: &'a dyn Fn(&str) -> Option<String>,
    /// A JSON (or JSONC) settings file: None when nothing is there, null
    /// when it is not that format.
    settings: &'a dyn Fn(&Location) -> Option<Value>,
    claude: Vec<claude::routes::Route>,
    codex: Vec<ModelProvider>,
}

/// The platform an OpenCode or Pi provider id names; a `baseURL` of its own
/// decides instead, and one off MiniMax's API is not a MiniMax route.
fn provider_region(id: &str, base: Option<&str>, pi: bool) -> Option<Region> {
    if let Some(base) = base {
        return Region::of_url(base);
    }
    match id {
        "minimax" => Some(Region::Global),
        "minimax-cn" => Some(Region::China),
        "minimax-coding-plan" if !pi => Some(Region::Global),
        "minimax-cn-coding-plan" if !pi => Some(Region::China),
        _ => None,
    }
}

/// Every key found, with the client that holds it, its platform where that
/// is known and the route the client logs its calls with it under, in the
/// order they are read.
type Found = (&'static str, String, Option<Region>, Option<String>);
fn found(machine: &Machine) -> Vec<Found> {
    let mut found = Vec::new();
    let mut add =
        |client: &'static str, key: Option<&str>, region: Option<Region>, route: Option<String>| {
            if let Some(key) = usable(key) {
                found.push((client, key.to_owned(), region, route));
            }
        };
    let env = machine.env;
    add(
        "MiniMax",
        env("MINIMAX_CODING_API_KEY").as_deref(),
        None,
        None,
    );
    add("MiniMax", env("MINIMAX_API_KEY").as_deref(), None, None);
    add(
        "MiniMax",
        env("MINIMAX_CN_API_KEY").as_deref(),
        Some(Region::China),
        None,
    );
    for route in &machine.claude {
        if let Some(region) = Region::of_url(&route.base_url) {
            add("Claude", route.key.as_deref(), Some(region), None);
        }
    }
    for provider in &machine.codex {
        if let Some(region) = Region::of_url(&provider.base_url) {
            add(
                "Codex",
                provider.key(env).as_deref(),
                Some(region),
                Some(format!("codex#{}", provider.id)),
            );
        }
    }
    // OpenCode: its sign-ins, once every config it has is readable, since a
    // config can send a provider elsewhere.
    let configs = OPENCODE_CONFIGS
        .iter()
        .filter_map(|location| (machine.settings)(location))
        .collect::<Vec<_>>();
    if configs.iter().all(Value::is_object) {
        let auth = (machine.settings)(&OPENCODE_AUTH).unwrap_or(Value::Null);
        for (id, value) in auth.as_object().into_iter().flatten() {
            let base = configs
                .iter()
                .rev()
                .find_map(|config| config["provider"][id]["options"]["baseURL"].as_str());
            if value["type"] == "api" {
                if let Some(region) = provider_region(id, base, false) {
                    add(
                        "OpenCode",
                        value["key"].as_str(),
                        Some(region),
                        Some(format!("opencode#{id}")),
                    );
                }
            }
        }
    }
    // Pi: an API key, or the variable that holds it.
    let auth = (machine.settings)(&PI_AUTH).unwrap_or(Value::Null);
    let models = (machine.settings)(&PI_MODELS).unwrap_or(Value::Null);
    for (id, value) in auth.as_object().into_iter().flatten() {
        let base = models["providers"][id]["baseUrl"].as_str();
        if value["type"] == "api_key" {
            if let Some(region) = provider_region(id, base, true) {
                let raw = value["key"].as_str();
                let key = raw.map(|raw| env(raw).unwrap_or_else(|| raw.to_owned()));
                add("Pi", key.as_deref(), Some(region), Some(format!("pi#{id}")));
            }
        }
    }
    found
}

/// Every MiniMax key on the host, each once with the clients that send it.
/// A key two clients send to different platforms is tried on both.
pub(super) fn keys(host: &dyn Host) -> Vec<Key> {
    let env = |name: &str| host.env(name).map(str::to_owned);
    let settings = |location: &Location| -> Option<Value> {
        let path = resolve(host, location).into_iter().next()?;
        let bytes = match host.read_file(&path, LIMIT) {
            crate::Answer::Missing => return None,
            answer => answer.found(),
        };
        let text = bytes
            .filter(|bytes| bytes.len() as u64 <= LIMIT)
            .and_then(|bytes| String::from_utf8(bytes).ok());
        Some(
            text.and_then(|text| json5::from_str(&text).ok())
                .unwrap_or(Value::Null),
        )
    };
    let machine = Machine {
        env: &env,
        settings: &settings,
        claude: claude::routes::configured(host),
        codex: codex::providers::configured(host),
    };
    // Each key once, with every platform a client named for it.
    let mut found_keys = Vec::<(Key, Vec<Region>)>::new();
    for (client, key, region, route) in found(&machine) {
        let index = match found_keys.iter().position(|(known, _)| known.key == key) {
            Some(index) => index,
            None => {
                found_keys.push((
                    Key {
                        key,
                        region: None,
                        clients: Vec::new(),
                        routes: Vec::new(),
                    },
                    Vec::new(),
                ));
                found_keys.len() - 1
            }
        };
        let (known, regions) = &mut found_keys[index];
        if !known.clients.contains(&client) {
            known.clients.push(client);
        }
        if let Some(route) = route.filter(|route| !known.routes.contains(route)) {
            known.routes.push(route);
        }
        if let Some(region) = region.filter(|region| !regions.contains(region)) {
            regions.push(region);
        }
    }
    found_keys
        .into_iter()
        .take(MOST)
        .map(|(mut key, regions)| {
            key.region = match regions[..] {
                [only] => Some(only),
                _ => None,
            };
            key
        })
        .collect()
}

/// What says MiniMax is configured through another client: Claude Code,
/// Codex, OpenCode or Pi sending a plan key to its API. Only whether a key
/// is there is looked at; the variables are `VARIABLES`.
pub(super) fn configured(look: &dyn Look) -> Vec<&'static str> {
    let env = |name: &str| look.env(name).map(str::to_owned);
    let settings = |location: &Location| -> Option<Value> {
        look.exists(location).then(|| look.settings(location))
    };
    let machine = Machine {
        env: &env,
        settings: &settings,
        claude: claude::routes::seen(look),
        codex: look
            .text(&codex::providers::CONFIG)
            .map(|toml| codex::providers::parse(&toml))
            .unwrap_or_default(),
    };
    let mut evidence = Vec::new();
    for (client, ..) in found(&machine) {
        let name = match client {
            "Claude" => "claude-provider-route",
            "Codex" => "codex-provider-route",
            "OpenCode" => "opencode-provider-route",
            "Pi" => "pi-provider-route",
            _ => continue,
        };
        if !evidence.contains(&name) {
            evidence.push(name);
        }
    }
    evidence
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    const CLAUDE: &str = r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.minimax.io/anthropic","ANTHROPIC_AUTH_TOKEN":"sk-cp-claude"}}"#;
    const CODEX: &str = "[model_providers.minimax]\nname = \"MiniMax\"\nbase_url = \"https://api.minimax.cn/v1\"\nexperimental_bearer_token = \"sk-cp-codex\"\n\n[model_providers.relay]\nbase_url = \"https://relay.example.com/v1\"\nexperimental_bearer_token = \"sk-relay\"\n";

    #[test]
    fn keys_sent_to_minimaxs_own_api_are_found_with_their_platform() {
        let host = FakeHost::new("/u")
            .file("/u/.claude/settings.json", CLAUDE)
            .file("/u/.codex/config.toml", CODEX)
            .file(
                "/u/.local/share/opencode/auth.json",
                r#"{"minimax-cn-coding-plan": {"type": "api", "key": "sk-cp-open"},
                    "anthropic": {"type": "api", "key": "sk-ant"}}"#,
            )
            .file(
                "/u/.pi/agent/auth.json",
                r#"{"minimax": {"type": "api_key", "key": "PI_MM"}}"#,
            )
            .env("PI_MM", "sk-cp-claude")
            .env("MINIMAX_API_KEY", "sk-cp-env");
        let keys = keys(&host);
        let key = |text: &str| keys.iter().find(|key| key.key == text).unwrap();
        assert_eq!(keys.len(), 4);
        assert_eq!(
            key("sk-cp-env").region,
            None,
            "the environment says no platform"
        );
        assert_eq!(key("sk-cp-env").clients, ["MiniMax"]);
        assert_eq!(key("sk-cp-claude").region, Some(Region::Global));
        assert_eq!(key("sk-cp-claude").clients, ["Claude", "Pi"]);
        assert_eq!(key("sk-cp-codex").region, Some(Region::China));
        assert_eq!(key("sk-cp-open").region, Some(Region::China));
        assert_eq!(key("sk-cp-open").clients, ["OpenCode"]);
        // The provider ids each client logs its calls with the key under.
        assert_eq!(key("sk-cp-claude").routes, ["pi#minimax"]);
        assert_eq!(key("sk-cp-codex").routes, ["codex#minimax"]);
        assert_eq!(
            key("sk-cp-open").routes,
            ["opencode#minimax-cn-coding-plan"]
        );
        assert!(key("sk-cp-env").routes.is_empty());
    }

    #[test]
    fn pay_as_you_go_keys_commands_and_other_hosts_are_not_plan_keys() {
        let host = FakeHost::new("/u")
            .env("MINIMAX_API_KEY", "sk-api-paygo")
            .env("MINIMAX_CODING_API_KEY", "!op read minimax")
            .file(
                "/u/.claude/settings.json",
                r#"{"env":{"ANTHROPIC_BASE_URL":"https://relay.example.com/minimax","ANTHROPIC_AUTH_TOKEN":"sk-cp-x"}}"#,
            );
        assert!(keys(&host).is_empty());
    }

    #[test]
    fn an_opencode_base_url_of_its_own_decides_the_platform() {
        let host = FakeHost::new("/u")
            .file(
                "/u/.config/opencode/opencode.json",
                r#"{"provider": {"minimax": {"options": {"baseURL": "https://api.minimaxi.com/anthropic/v1"}},
                    "minimax-coding-plan": {"options": {"baseURL": "https://proxy.example.com/v1"}}}}"#,
            )
            .file(
                "/u/.local/share/opencode/auth.json",
                r#"{"minimax": {"type": "api", "key": "sk-cp-a"},
                    "minimax-coding-plan": {"type": "api", "key": "sk-cp-b"}}"#,
            );
        let keys = keys(&host);
        assert_eq!(keys.len(), 1, "a proxy is not MiniMax's API");
        assert_eq!(keys[0].region, Some(Region::China));
    }

    #[test]
    fn a_key_sent_to_both_platforms_is_tried_on_both() {
        let host = FakeHost::new("/u")
            .file(
                "/u/.claude/settings.json",
                r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.minimax.cn/anthropic","ANTHROPIC_AUTH_TOKEN":"sk-cp-k"}}"#,
            )
            .file(
                "/u/.codex/config.toml",
                "[model_providers.mm]\nbase_url = \"https://api.minimax.io/v1\"\nexperimental_bearer_token = \"sk-cp-k\"\n",
            );
        let keys = keys(&host);
        assert_eq!(keys[0].region, None);
        assert_eq!(keys[0].clients, ["Claude", "Codex"]);
    }
}
