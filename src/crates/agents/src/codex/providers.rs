//! The model providers Codex's `config.toml` defines (`[model_providers.<id>]`):
//! where Codex sends its calls besides OpenAI, and the key it sends there.
//! Relays and vendor balances both start from these; each decides for
//! itself which addresses are its own.
use crate::account::{read_bounded, resolve, Host};
use crate::Location;

/// Codex's configuration, in `CODEX_HOME` when set, else in `~/.codex`.
pub const CONFIG: Location = Location::home(&["CODEX_HOME"], &[".codex"], "config.toml");

/// The configuration's size limit.
const LIMIT: u64 = 1024 * 1024;

/// One `[model_providers.<id>]` table of a Codex `config.toml`.
#[derive(Debug, Default, PartialEq)]
pub struct ModelProvider {
    /// The table's name, which its calls are recorded under.
    pub id: String,
    pub base_url: String,
    /// The variable that holds the key it sends.
    pub env_key: Option<String>,
    /// A key written in the file itself (`experimental_bearer_token`).
    pub bearer: Option<String>,
}

impl ModelProvider {
    /// The key Codex sends this provider: the one in the file, else the
    /// value of its variable.
    pub fn key(&self, env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
        self.bearer
            .clone()
            .or_else(|| self.env_key.as_deref().and_then(env))
    }
}

/// The model providers a Codex `config.toml` defines: each
/// `[model_providers.<id>]` table's `base_url`, `env_key` and
/// `experimental_bearer_token`, read as plain string values.
pub fn parse(toml: &str) -> Vec<ModelProvider> {
    let mut providers = Vec::new();
    let mut current: Option<ModelProvider> = None;
    let mut flush = |current: &mut Option<ModelProvider>| {
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
                    current = Some(ModelProvider {
                        id: id.to_owned(),
                        ..ModelProvider::default()
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

/// The model providers Codex is configured with on the host.
pub fn configured(host: &dyn Host) -> Vec<ModelProvider> {
    resolve(host, &CONFIG)
        .first()
        .and_then(|path| read_bounded(host, path, LIMIT).found())
        .map(|bytes| parse(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    const TOML: &str = r#"
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

    #[test]
    fn providers_come_from_the_config_tables() {
        assert_eq!(
            parse(TOML),
            [
                ModelProvider {
                    id: "relay".into(),
                    base_url: "https://relay.example.com/v1".into(),
                    env_key: Some("RELAY_KEY".into()),
                    bearer: None
                },
                ModelProvider {
                    id: "inline".into(),
                    base_url: "https://other.example.com/v1".into(),
                    env_key: None,
                    bearer: Some("sk-inline".into())
                },
            ]
        );
    }

    #[test]
    fn the_key_is_the_files_else_its_variables() {
        let env = |name: &str| (name == "RELAY_KEY").then(|| "sk-env".to_owned());
        let keys = parse(TOML)
            .iter()
            .map(|provider| provider.key(&env))
            .collect::<Vec<_>>();
        assert_eq!(keys, [Some("sk-env".into()), Some("sk-inline".into())]);
    }

    #[test]
    fn codex_home_moves_the_config() {
        let home = "/u";
        let codex = format!("{home}/codex-alt");
        let host = FakeHost::new(home)
            .env("CODEX_HOME", &codex)
            .file("/u/.codex/config.toml", TOML)
            .file(
                &format!("{codex}/config.toml"),
                "[model_providers.deepseek]\nbase_url = \"https://api.deepseek.com\"\n",
            );
        let providers = configured(&host);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, "deepseek");
    }
}
