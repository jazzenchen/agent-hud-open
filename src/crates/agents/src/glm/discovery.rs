//! How a GLM Coding Plan is found configured: its keys in the environment,
//! or another client (Claude Code, OpenCode, Pi) routed to the plan. A plan
//! configured only in WorkBuddy belongs to WorkBuddy, as the original groups
//! it, so that file alone never makes a standalone GLM item.
use crate::discovery::Look;
use crate::Location;
use serde_json::Value;

const OPENCODE_CONFIGS: [Location; 2] = [
    Location::config_home(&["opencode"], "opencode.json"),
    Location::config_home(&["opencode"], "opencode.jsonc"),
];
const OPENCODE_AUTH: Location = Location::data_home(&["opencode"], "auth.json");
const PI_MODELS: Location = Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "models.json");
const PI_AUTH: Location = Location::home(&["PI_CODING_AGENT_DIR"], &[".pi/agent"], "auth.json");

/// A secret that is there, not a reference to look one up (`!command`).
fn present(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|value| !value.is_empty() && !value.starts_with('!'))
}

/// Whether a provider id, or a base URL, is a GLM Coding Plan's.
fn plan(provider: &str, base: Option<&str>, pi: bool) -> bool {
    if let Some(base) = base {
        return ["https://api.z.ai", "https://open.bigmodel.cn"]
            .iter()
            .any(|host| {
                ["/api/coding/paas/v4", "/api/anthropic"]
                    .iter()
                    .any(|path| {
                        base == format!("{host}{path}")
                            || base == format!("{host}{path}/")
                            || base == format!("{host}:443{path}")
                    })
            });
    }
    matches!(provider, "zai-coding-plan" | "zhipuai-coding-plan")
        || pi && matches!(provider, "zai" | "zai-coding-cn")
}

pub fn configured(look: &dyn Look) -> Vec<&'static str> {
    let mut found = Vec::new();
    let has = |key: &str| look.env(key).is_some_and(|value| !value.is_empty());
    let region = look.env("Z_AI_REGION").unwrap_or("global");
    let scope = look.env("Z_AI_USAGE_SCOPE").unwrap_or("personal");
    if has("Z_AI_API_KEY")
        && matches!(region, "global" | "bigmodel-cn")
        && (scope == "personal"
            || scope == "team" && has("Z_AI_ORGANIZATION") && has("Z_AI_PROJECT"))
    {
        found.push("provider-environment");
    }
    // Claude Code's settings files (in CLAUDE_CONFIG_DIR when set), then
    // the environment.
    if crate::claude::routes::seen(look).iter().any(|route| {
        plan("", Some(&route.base_url), false)
            && route
                .key
                .as_deref()
                .is_some_and(|key| !key.is_empty() && !key.starts_with('!'))
    }) {
        found.push("claude-provider-route");
    }
    let configs = OPENCODE_CONFIGS
        .iter()
        .map(|location| look.settings(location))
        .collect::<Vec<_>>();
    let valid_configs = OPENCODE_CONFIGS
        .iter()
        .zip(&configs)
        .all(|(location, config)| !look.exists(location) || config.is_object());
    if let Some(auth) = look.settings(&OPENCODE_AUTH).as_object() {
        for (provider, auth) in auth {
            let base = configs
                .iter()
                .rev()
                .find_map(|config| config["provider"][provider]["options"]["baseURL"].as_str());
            if valid_configs
                && auth["type"] == "api"
                && present(&auth["key"])
                && plan(provider, base, false)
            {
                found.push("opencode-provider-route");
            }
        }
    }
    let models = look.settings(&PI_MODELS);
    if let Some(auth) = look.settings(&PI_AUTH).as_object() {
        for (provider, auth) in auth {
            if auth["type"] == "api_key"
                && present(&auth["key"])
                && plan(
                    provider,
                    models["providers"][provider]["baseUrl"].as_str(),
                    true,
                )
            {
                found.push("pi-provider-route");
            }
        }
    }
    found
}
