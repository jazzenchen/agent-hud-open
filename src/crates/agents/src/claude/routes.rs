//! Where Claude Code is told to send its model calls: `ANTHROPIC_BASE_URL`
//! and the key it sends there, in the `env` of its settings files or in the
//! environment. Relays, coding plans and vendor balances all start from
//! these; each decides for itself which addresses are its own.
use crate::account::{read_json, resolve, Host};
use crate::{Location, Look};
use serde_json::Value;

/// Claude Code's own settings files, in the order they are read:
/// `settings.json`, then `settings.local.json` beside it, both in
/// `CLAUDE_CONFIG_DIR` when it is set, else in `~/.claude`.
pub const SETTINGS: [Location; 2] = [
    Location::home(&["CLAUDE_CONFIG_DIR"], &[".claude"], "settings.json"),
    Location::home(&["CLAUDE_CONFIG_DIR"], &[".claude"], "settings.local.json"),
];

/// One address Claude Code is configured to call, and the key it sends with
/// it (`ANTHROPIC_AUTH_TOKEN`, else `ANTHROPIC_API_KEY`), as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub base_url: String,
    pub key: Option<String>,
}

/// The route a set of variables names, when it names an address.
fn route(variable: &dyn Fn(&str) -> Option<String>) -> Option<Route> {
    Some(Route {
        base_url: variable("ANTHROPIC_BASE_URL")?,
        key: variable("ANTHROPIC_AUTH_TOKEN").or_else(|| variable("ANTHROPIC_API_KEY")),
    })
}

/// The route a settings file's `env` names.
fn from_settings(settings: &Value) -> Option<Route> {
    let env = &settings["env"];
    route(&|name| env[name].as_str().map(str::to_owned))
}

/// Every route Claude Code is configured with on the host: each settings
/// file's, then the environment's.
pub fn configured(host: &dyn Host) -> Vec<Route> {
    let mut routes = SETTINGS
        .iter()
        .flat_map(|location| resolve(host, location))
        .filter_map(|path| read_json(host, &path))
        .filter_map(|settings| from_settings(&settings))
        .collect::<Vec<_>>();
    routes.extend(route(&|name| host.env(name).map(str::to_owned)));
    routes
}

/// The same routes, as discovery looks at the machine.
pub fn seen(look: &dyn Look) -> Vec<Route> {
    let mut routes = SETTINGS
        .iter()
        .filter_map(|location| from_settings(&look.settings(location)))
        .collect::<Vec<_>>();
    routes.extend(route(&|name| look.env(name).map(str::to_owned)));
    routes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    const GLM: &str = r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.z.ai/api/anthropic","ANTHROPIC_AUTH_TOKEN":"sk-glm"}}"#;
    const KIMI: &str = r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.kimi.com/coding","ANTHROPIC_API_KEY":"sk-kimi"}}"#;

    fn route(base: &str, key: Option<&str>) -> Route {
        Route {
            base_url: base.into(),
            key: key.map(str::to_owned),
        }
    }

    #[test]
    fn both_settings_files_then_the_environment_name_routes() {
        let host = FakeHost::new("/u")
            .file("/u/.claude/settings.json", GLM)
            .file("/u/.claude/settings.local.json", KIMI)
            .env("ANTHROPIC_BASE_URL", "https://relay.example.com")
            .env("ANTHROPIC_API_KEY", "sk-env");
        assert_eq!(
            configured(&host),
            [
                route("https://api.z.ai/api/anthropic", Some("sk-glm")),
                route("https://api.kimi.com/coding", Some("sk-kimi")),
                route("https://relay.example.com", Some("sk-env")),
            ]
        );
    }

    #[test]
    fn claude_config_dir_moves_both_settings_files() {
        let home = "/u";
        let config = format!("{home}/work-claude");
        let host = FakeHost::new(home)
            .env("CLAUDE_CONFIG_DIR", &config)
            .file("/u/.claude/settings.json", GLM)
            .file(&format!("{config}/settings.local.json"), KIMI);
        assert_eq!(
            configured(&host),
            [route("https://api.kimi.com/coding", Some("sk-kimi"))],
            "the default folder is not read while the variable names another"
        );
    }

    #[test]
    fn settings_without_an_address_name_no_route() {
        let host = FakeHost::new("/u").file(
            "/u/.claude/settings.json",
            r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"sk"}}"#,
        );
        assert!(configured(&host).is_empty());
    }
}
