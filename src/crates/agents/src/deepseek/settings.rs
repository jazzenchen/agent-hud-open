//! What DeepSeek Harness calls, as its settings say: the endpoint, and the key
//! it sends there. The balance reader (`account.rs`) asks DeepSeek's own API
//! with it; the relays reader asks a relay with it when the endpoint is one.
use crate::account::{Answer, Host, Unread};
use crate::l10n;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub(super) fn unreadable() -> String {
    l10n::text(
        "无法读取 DeepSeek 设置或凭据",
        "DeepSeek settings or credentials could not be read.",
    )
}
fn yaml(host: &dyn Host, path: &Path) -> Result<Value, String> {
    const LIMIT: u64 = 1024 * 1024;
    let bytes = match host.read_file(path, LIMIT) {
        Answer::Found(bytes) => bytes,
        Answer::Missing => return Ok(Value::Null),
        _ => return Err(unreadable()),
    };
    if bytes.len() as u64 > LIMIT {
        return Err(l10n::text(
            "DeepSeek 设置或凭据文件过大",
            "DeepSeek settings or credentials exceeded the size limit.",
        ));
    }
    let text = String::from_utf8(bytes).map_err(|_| unreadable())?;
    // A YAML error can quote the offending secret line. Only return fixed diagnostics.
    serde_saphyr::from_str(&text).map_err(|_| {
        l10n::text(
            "DeepSeek 设置或凭据的 YAML 格式无法识别",
            "DeepSeek settings or credentials have an unsupported YAML format.",
        )
    })
}

/// Harness's home: `DSH_HOME`, or `~/.dsh`.
fn directory(host: &dyn Host) -> PathBuf {
    let Some(path) = host.env("DSH_HOME").filter(|path| !path.trim().is_empty()) else {
        return host.home().join(".dsh");
    };
    if path == "~" {
        host.home().to_path_buf()
    } else if let Some(tail) = path.strip_prefix("~/") {
        host.home().join(tail)
    } else {
        PathBuf::from(path)
    }
}

/// Harness's endpoint and key: the ones saved in Agent HUD, or else its
/// `settings.yaml` and the environment, with the credentials file that may
/// hold a Platform grant instead of a key.
pub(super) struct Configured {
    pub directory: PathBuf,
    pub endpoint: String,
    pub credential: Option<String>,
    pub credentials: Value,
}

pub(super) fn configured(host: &dyn Host) -> Result<Configured, Unread> {
    let directory = directory(host);
    let saved = host.saved_credential("deepseek")?;
    let settings = if saved.is_none() {
        yaml(host, &directory.join("settings.yaml"))?
    } else {
        Value::Null
    };
    let config = &settings["llm-deepseek"];
    let reference = config["apiKeyEnv"].as_str().unwrap_or("DEEPSEEK_API_KEY");
    let endpoint = saved
        .as_ref()
        .map(|value| {
            value["baseUrl"]
                .as_str()
                .unwrap_or("https://api.deepseek.com")
        })
        .or_else(|| config["baseURL"].as_str())
        .or_else(|| host.env("DEEPSEEK_BASE_URL"))
        .unwrap_or("https://api.deepseek.com")
        .to_owned();
    let mut credential = saved
        .as_ref()
        .and_then(|value| value["apiKey"].as_str())
        .map(str::to_owned)
        .or_else(|| {
            host.env(reference)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        });
    let mut credentials = Value::Null;
    if credential.is_none() {
        credentials = yaml(host, &directory.join(".credentials.yaml"))?;
        if !credentials.is_null() {
            if credentials["version"] != 1
                || !credentials.as_object().is_some_and(|fields| {
                    fields
                        .keys()
                        .all(|key| matches!(key.as_str(), "version" | "refs" | "records"))
                })
                || !(credentials["refs"].is_null()
                    || credentials["refs"].as_object().is_some_and(|refs| {
                        refs.values()
                            .all(|value| value.as_str().is_some_and(|value| !value.is_empty()))
                    }))
            {
                return Err(l10n::text(
                    "DeepSeek 凭据文件格式无法识别",
                    "DeepSeek credentials have an unsupported document format.",
                )
                .into());
            }
            credential = credentials["refs"][reference].as_str().map(str::to_owned);
        }
    }
    Ok(Configured {
        directory,
        endpoint,
        credential,
        credentials,
    })
}

/// Harness's endpoint and the key it sends there, when it has a key. The
/// relays reader takes them for a relay when the endpoint is not DeepSeek's
/// own API; settings it cannot read give no relay.
pub fn route(host: &dyn Host) -> Option<(String, String)> {
    let configured = configured(host).ok()?;
    Some((configured.endpoint, configured.credential?))
}
