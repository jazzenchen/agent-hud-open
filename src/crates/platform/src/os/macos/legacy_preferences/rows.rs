//! The agent rows the original store kept, by Agent HUD's provider ids.
use super::Row;
use serde_json::Value;

pub(super) fn provider(value: &str) -> Option<&'static str> {
    let key = value.to_lowercase();
    let key = match key.as_str() {
        "claude" | "claude code" => "claude",
        "grok" | "grok cli" => "grok-cli",
        "qwen" => "qwen",
        _ => key.as_str(),
    };
    agent_hud_agents::ALL
        .iter()
        .find(|p| p.id == key || p.name.to_lowercase() == key)
        .map(|p| p.id)
}
pub(super) fn block_key(value: &str) -> Option<String> {
    let (api, value) = value
        .strip_prefix("api:")
        .map(|v| (true, v))
        .unwrap_or((false, value));
    provider(value).map(|id| if api { format!("api:{id}") } else { id.into() })
}
pub(super) fn identity(value: &str) -> bool {
    value
        .strip_prefix("account:")
        .or_else(|| value.strip_prefix("pool:"))
        .is_some_and(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}
pub(super) fn rows(agents: &Value) -> Vec<Row> {
    let mut out = vec![];
    for agent in agents.as_array().into_iter().flatten() {
        if agent["connected"] != true {
            continue;
        }
        let (Some(id), Some(vendor), Some(enabled)) = (
            agent["id"].as_str().filter(|id| !id.is_empty()),
            agent["vendor"].as_str(),
            agent["enabled"].as_bool(),
        ) else {
            continue;
        };
        let api = agent["billingPool"]["product"] == "api" || vendor == "DeepSeek";
        let displayed = if agent["billingPool"]["product"] == "api" {
            agent["billingPool"]["provider"].as_str().unwrap_or(vendor)
        } else {
            vendor
        };
        let Some(provider) = provider(displayed) else {
            continue;
        };
        let account = agent["account"]["id"]
            .as_str()
            .filter(|id| identity(id))
            .map(str::to_owned)
            // The computed pool id is already an exact prefix of a pool row.
            // Do not reconstruct identity from a credential, label or path.
            .or_else(|| {
                id.get(..69)
                    .filter(|prefix| {
                        prefix.starts_with("pool:")
                            && identity(prefix)
                            && id.as_bytes().get(69) == Some(&b':')
                    })
                    .map(str::to_owned)
            })
            .or_else(|| (api && vendor == "DeepSeek").then(|| "DeepSeek".into()));
        let window = account
            .as_ref()
            .and_then(|a| {
                id.strip_prefix(&format!("{a}/"))
                    .or_else(|| id.strip_prefix(&format!("{a}:")))
            })
            .unwrap_or(id)
            .to_owned();
        out.push(Row {
            id: id.into(),
            group: if api {
                format!("api:{provider}")
            } else {
                provider.into()
            },
            account,
            window,
            enabled,
            api,
        });
    }
    out
}
