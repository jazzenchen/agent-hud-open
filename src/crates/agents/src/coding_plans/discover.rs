//! Coding-plan credentials other clients already hold: environment
//! variables, Claude Code's and OpenCode's settings, Pi's and Kimi Code's
//! sign-ins, WorkBuddy's models and keys saved in Agent HUD. Discovery only
//! reads: no key resolver runs, no token is refreshed, nobody signs in.
use super::{nonempty, numeric, Credential};
use crate::account::{account_id, jwt_claims, read_bounded, variable, Host};
use crate::l10n;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

/// A settings file's size limit.
const LIMIT: u64 = 1024 * 1024;

/// A JSON (or JSON5) settings file; null when it is missing, too large or
/// not that format.
fn file(host: &dyn Host, path: &Path, json5: bool) -> Value {
    present(host, path, json5).unwrap_or(Value::Null)
}
/// `file`, or None when nothing is at `path`.
fn present(host: &dyn Host, path: &Path, json5: bool) -> Option<Value> {
    let bytes = match read_bounded(host, path, LIMIT) {
        crate::Answer::Missing => return None,
        answer => answer.found(),
    };
    let text = bytes.and_then(|bytes| String::from_utf8(bytes).ok());
    Some(match text {
        Some(text) if json5 => json5::from_str(&text).unwrap_or(Value::Null),
        Some(text) => serde_json::from_str(&text).unwrap_or(Value::Null),
        None => Value::Null,
    })
}
fn usable(key: &str, now: i64) -> bool {
    jwt_claims(key)
        .and_then(|p| p["exp"].as_f64())
        .is_none_or(|expiry| expiry > now as f64 / 1000.0 + 60.0)
}
fn service(id: &str, base: Option<&str>, pi: bool) -> Option<(&'static str, String)> {
    if let Some(base) = base {
        for provider in ["kimi", "glm", "opencode"] {
            if let Ok(base) = crate::account::endpoint(provider, Some(base)) {
                return Some((provider, base));
            }
        }
        return None;
    }
    let (provider, base) = match id {
        "kimi-for-coding" | "kimi-coding" | "kimi-code" => {
            ("kimi", "https://api.kimi.com/coding/v1")
        }
        "zai" if pi => ("glm", "https://api.z.ai/api/coding/paas/v4"),
        "zai-coding-cn" if pi => ("glm", "https://open.bigmodel.cn/api/coding/paas/v4"),
        "zai-coding-plan" => ("glm", "https://api.z.ai/api/coding/paas/v4"),
        "zhipuai-coding-plan" => ("glm", "https://open.bigmodel.cn/api/coding/paas/v4"),
        "opencode-go" => ("opencode", "https://opencode.ai/zen/go/v1"),
        _ => return None,
    };
    Some((provider, base.into()))
}
pub(super) fn discover(host: &dyn Host, wanted: &str) -> Result<Vec<Credential>, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut entries = Vec::new();
    let mut add = |service: Option<(&'static str, String)>,
                   key: Option<&str>,
                   client: &str,
                   organization: Option<&str>,
                   project: Option<&str>,
                   device: Option<String>| {
        if let (Some((provider, base)), Some(key)) = (service, nonempty(key)) {
            if usable(key, now) {
                entries.push(Credential {
                    provider,
                    base,
                    key: key.into(),
                    organization: organization.map(str::to_owned),
                    project: project.map(str::to_owned),
                    clients: HashSet::from([client.into()]),
                    device,
                });
            }
        }
    };
    let env = |key: &str| host.env(key);
    add(
        service("kimi-code", env("KIMI_CODE_BASE_URL"), false),
        env("KIMI_CODE_API_KEY"),
        "Kimi",
        None,
        None,
        None,
    );
    for (key, id, client) in [
        ("OPENCODE_GO_API_KEY", "opencode-go", "OpenCode"),
        ("ZAI_API_KEY", "zai-coding-plan", "Pi"),
        ("ZAI_CODING_CN_API_KEY", "zhipuai-coding-plan", "Pi"),
        ("KIMI_API_KEY", "kimi-code", "Pi"),
        ("BIGMODEL_API_KEY", "zhipuai-coding-plan", "GLM"),
        ("ZHIPU_API_KEY", "zhipuai-coding-plan", "GLM"),
        ("ZHIPUAI_API_KEY", "zhipuai-coding-plan", "GLM"),
        ("GLM_API_KEY", "zhipuai-coding-plan", "GLM"),
    ] {
        add(service(id, None, true), env(key), client, None, None, None);
    }
    let region = env("Z_AI_REGION").unwrap_or("global");
    let scope = env("Z_AI_USAGE_SCOPE").unwrap_or("personal");
    if matches!(region, "global" | "bigmodel-cn")
        && matches!(scope, "personal" | "team")
        && (scope == "personal"
            || (nonempty(env("Z_AI_ORGANIZATION")).is_some()
                && nonempty(env("Z_AI_PROJECT")).is_some()))
    {
        add(
            service(
                if region == "global" {
                    "zai-coding-plan"
                } else {
                    "zhipuai-coding-plan"
                },
                None,
                false,
            ),
            env("Z_AI_API_KEY"),
            "GLM",
            (scope == "team")
                .then(|| env("Z_AI_ORGANIZATION"))
                .flatten(),
            (scope == "team").then(|| env("Z_AI_PROJECT")).flatten(),
            None,
        );
    }
    // Claude Code: both its settings files (in CLAUDE_CONFIG_DIR when set),
    // then the environment.
    for route in crate::claude::routes::configured(host) {
        add(
            service("", Some(&route.base_url), false),
            route.key.as_deref(),
            "Claude",
            None,
            None,
            None,
        );
    }
    let data = variable(host, "XDG_DATA_HOME").unwrap_or_else(|| host.home().join(".local/share"));
    let config = variable(host, "XDG_CONFIG_HOME").unwrap_or_else(|| host.home().join(".config"));
    let configs = [
        config.join("opencode/opencode.json"),
        config.join("opencode/opencode.jsonc"),
    ]
    .into_iter()
    .filter_map(|p| present(host, &p, true))
    .collect::<Vec<_>>();
    if configs.iter().all(Value::is_object) {
        let auth = file(host, &data.join("opencode/auth.json"), false);
        for (id, value) in auth.as_object().into_iter().flatten() {
            if value["type"] == "api" {
                let base = configs
                    .iter()
                    .rev()
                    .find_map(|c| c["provider"][id]["options"]["baseURL"].as_str());
                add(
                    service(id, base, false),
                    value["key"].as_str(),
                    "OpenCode",
                    None,
                    None,
                    None,
                );
            }
        }
    }
    let pi = variable(host, "PI_CODING_AGENT_DIR").unwrap_or_else(|| host.home().join(".pi/agent"));
    let auth = file(host, &pi.join("auth.json"), false);
    let models = file(host, &pi.join("models.json"), false);
    for (id, value) in auth.as_object().into_iter().flatten() {
        let target = service(id, models["providers"][id]["baseUrl"].as_str(), true);
        if value["type"] == "api_key" {
            let raw = value["key"].as_str();
            add(
                target,
                raw.and_then(|key| host.env(key).or(Some(key))),
                "Pi",
                None,
                None,
                None,
            );
        } else if id == "kimi-coding"
            && value["type"] == "oauth"
            && env("KIMI_CODE_OAUTH_HOST").is_none()
            && env("KIMI_OAUTH_HOST").is_none()
            && numeric(&value["expires"]).is_some_and(|n| n > now as f64 + 60_000.0)
        {
            add(target, value["access"].as_str(), "Pi", None, None, None);
        }
    }
    let kimi = variable(host, "KIMI_CODE_HOME").unwrap_or_else(|| host.home().join(".kimi-code"));
    let oauth = env("KIMI_CODE_OAUTH_HOST").or_else(|| env("KIMI_OAUTH_HOST"));
    for (api, slot, oauth_host) in [
        (
            "https://api.kimi.com/coding/v1",
            "kimi-code".to_owned(),
            "https://auth.kimi.com",
        ),
        (
            "https://api.kimi.ai/coding/v1",
            {
                let mut h = Sha256::new();
                h.update(br#"{"oauthHost":"https://auth.kimi.ai","baseUrl":"https://api.kimi.ai/coding/v1"}"#);
                format!("kimi-code-env-{}", &format!("{:x}", h.finalize())[..16])
            },
            "https://auth.kimi.ai",
        ),
    ] {
        if oauth.is_some_and(|v| v != oauth_host)
            || env("KIMI_CODE_BASE_URL").is_some_and(|v| {
                crate::account::endpoint("kimi", Some(v)).ok().as_deref() != Some(api)
            })
        {
            continue;
        }
        let auth = file(host, &kimi.join(format!("credentials/{slot}.json")), false);
        if numeric(&auth["expires_at"]).is_some_and(|n| n > now as f64 / 1000.0 + 60.0) {
            let device = read_bounded(host, &kimi.join("device_id"), 4096)
                .found()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .map(|s| s.trim().to_owned())
                .filter(|s| nonempty(Some(s)).is_some());
            add(
                service("", Some(api), false),
                auth["access_token"].as_str(),
                "Kimi",
                None,
                None,
                device,
            );
        }
    }
    let wb = file(host, &host.home().join(".workbuddy/models.json"), false);
    for value in wb.as_array().into_iter().flatten() {
        if value["vendor"] == "GLM Coding Plan" {
            let target = service("", value["url"].as_str(), false).filter(|(p, _)| *p == "glm");
            add(
                target,
                value["apiKey"].as_str(),
                "WorkBuddy",
                None,
                None,
                None,
            );
        }
    }
    for provider in ["kimi", "glm", "opencode"]
        .into_iter()
        .filter(|p| *p == wanted)
    {
        if let Some(saved) = host.saved_credential(provider)? {
            add(
                Some((
                    provider,
                    crate::account::endpoint(provider, saved["baseUrl"].as_str())?,
                )),
                saved["apiKey"].as_str(),
                "Agent HUD",
                saved["organization"].as_str(),
                saved["project"].as_str(),
                None,
            );
        }
    }
    let mut unique = BTreeMap::<String, Credential>::new();
    for entry in entries {
        let id = account_id(
            entry.provider,
            &[
                &entry.base,
                &entry.key,
                entry.organization.as_deref().unwrap_or(""),
                entry.project.as_deref().unwrap_or(""),
            ],
        );
        unique
            .entry(id)
            .and_modify(|old| old.clients.extend(entry.clients.clone()))
            .or_insert(entry);
    }
    if unique.len() > 32 {
        return Err(l10n::text(
            "单独配置的 Coding Plan 凭据过多",
            "Too many independently configured coding-plan credentials",
        ));
    }
    Ok(unique.into_values().collect())
}
