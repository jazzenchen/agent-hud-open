//! Coding plans: Kimi Code, the GLM Coding Plan and OpenCode Go (Open's
//! OpenAgentCredentials and OpenAgentQuota). Their quotas are read with the
//! credentials other clients already hold, from each plan's own usage
//! endpoint; one plan can serve several clients.
mod discover;
mod parse;
#[cfg(test)]
mod tests;

use crate::account::{account_id, Host, HttpFailure, HttpRequest, Provider, Reading, Unread};
use crate::l10n;
use serde_json::{json, Value};
use std::{collections::HashSet, time::Duration};

#[derive(Clone)]
struct Credential {
    provider: &'static str,
    base: String,
    key: String,
    organization: Option<String>,
    project: Option<String>,
    clients: HashSet<String>,
    device: Option<String>,
}
fn nonempty(value: Option<&str>) -> Option<&str> {
    value.filter(|s| {
        !s.trim().is_empty()
            && !s.starts_with('!')
            && !s.bytes().any(|b| matches!(b, 0 | b'\n' | b'\r'))
    })
}
fn numeric(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|v| v.is_finite() && *v >= 0.0)
}
fn read_provider(host: &dyn Host, provider: &'static str) -> Result<Reading, Unread> {
    let entries = discover::discover(host, provider)?
        .into_iter()
        .filter(|e| e.provider == provider)
        .collect::<Vec<_>>();
    let mut result = Reading::empty(provider);
    if entries.is_empty() {
        result.notice = Some(l10n::text(
            "此客户端的现有配置中没有可用的 Coding Plan 凭据",
            "No usable coding-plan credential was found in this client's existing configuration",
        ));
        return Ok(result);
    }
    let mut failures = 0;
    let mut transient = 0;
    for entry in entries {
        if host.cancelled() {
            return Err(l10n::text("账户读取已停止", "Account reading was stopped").into());
        }
        let mut id = account_id(
            provider,
            &[
                &entry.base,
                &entry.key,
                entry.organization.as_deref().unwrap_or(""),
                entry.project.as_deref().unwrap_or(""),
            ],
        );
        let server = url::Url::parse(&entry.base)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .expect("discovery keeps only checked endpoints");
        let endpoint = match provider {
            "kimi" => format!("https://{server}/coding/v1/usages"),
            "opencode" => "https://opencode.ai/zen/go/v1/usage".into(),
            "glm" => format!(
                "https://{server}/api/monitor/usage/quota/limit{}",
                if entry.organization.is_some() {
                    "?type=2"
                } else {
                    ""
                }
            ),
            _ => unreachable!(),
        };
        let authorization = format!("Bearer {}", entry.key);
        let mut headers = Vec::new();
        if let Some(org) = &entry.organization {
            headers.push(("Bigmodel-Organization", org.as_str()));
        }
        if let Some(project) = &entry.project {
            headers.push(("Bigmodel-Project", project.as_str()));
        }
        if let Some(device) = &entry.device {
            headers.push(("X-Msh-Device-Id", device.as_str()));
        }
        let get = |url: &str, seconds: u64| {
            host.http_json(&HttpRequest {
                url,
                secrets: &[("Authorization", &authorization)],
                body: None,
                headers: &headers,
                timeout: Duration::from_secs(seconds),
            })
        };
        if provider == "kimi" {
            if let Ok(profile) = get(&format!("https://{server}/coding/v1/me"), 8) {
                if let (Some(user), Some(region)) = (
                    profile["user_id"].as_str().filter(|s| !s.is_empty()),
                    profile["region"].as_str().filter(|s| !s.is_empty()),
                ) {
                    let domain = numeric(&profile["domain"]).unwrap_or(0.0);
                    id = account_id(provider, &[&server, user, &domain.to_string(), region]);
                }
            }
        }
        let reading = get(&endpoint, 15)
            .map_err(|failure| {
                if matches!(failure.unread(), Unread::Transient { .. }) {
                    transient += 1;
                }
                match failure {
                    HttpFailure::Unreachable | HttpFailure::BadHeader => l10n::text(
                        "无法连接 Coding Plan 额度服务",
                        "Coding plan quota could not be reached",
                    ),
                    other => other.message(),
                }
            })
            .and_then(|body| parse::parse(&body, &entry, &id));
        // Every client configured with this plan reaches it on the plan's
        // platform, read or not; the platform picks its models' price list.
        let (vendor, region) = plan_service(provider, &server);
        let mut clients = entry.clients.iter().collect::<Vec<_>>();
        clients.sort();
        for client in clients {
            result.services.push(json!({"client":client,"provider":vendor,"product":"plan","accountId":id,"region":region}));
        }
        match reading {
            Ok((quotas, plan)) => {
                result.quotas.extend(quotas);
                let account = json!({"id":id,"provider":provider,"label":plan,"plan":plan,"clients":entry.clients,"identityEvidence":if provider=="kimi"&&id!=account_id(provider,&[&entry.base,&entry.key,entry.organization.as_deref().unwrap_or(""),entry.project.as_deref().unwrap_or("")]){"provider-account"}else{"credential-scope"}});
                if result.account.is_none() {
                    result.account = Some(account);
                } else if result
                    .account
                    .as_ref()
                    .is_none_or(|a| a["id"] != account["id"])
                    && !result
                        .additional_accounts
                        .iter()
                        .any(|a| a["id"] == account["id"])
                {
                    result.additional_accounts.push(account);
                }
            }
            Err(_) => failures += 1,
        }
    }
    let mut seen = HashSet::new();
    result.quotas.retain(|q| {
        q["id"]
            .as_str()
            .is_some_and(|id| seen.insert(id.to_owned()))
    });
    if failures > 0 {
        if result.quotas.is_empty() {
            // Only no answer this time, for every plan: it passes by itself.
            if transient == failures {
                return Err(Unread::transient(l10n::text(
                    "无法连接 Coding Plan 额度服务",
                    "Coding plan quota could not be reached",
                )));
            }
            return Err(l10n::text("现有 Coding Plan 凭据未能读到可核实的额度，请检查来源客户端的登录或 API Key", "Existing coding-plan credentials could not read a verifiable quota; check the source client's login or API key").into());
        }
        result.notice = Some(format!(
            "{failures} configured coding-plan account(s) could not be read"
        ));
    }
    Ok(result)
}
/// The open agents' plans are read as one source whose sessions are
/// OpenCode's, Kimi's, GLM's and Pi's.
const OPEN_AGENTS: &[&str] = &["opencode", "kimi", "glm", "pi"];
/// Kimi Code's plan.
pub static KIMI: Provider = Provider {
    work_sources: OPEN_AGENTS,
    ..Provider::new("kimi", kimi)
};
/// GLM's coding plan.
pub static GLM: Provider = Provider {
    work_sources: OPEN_AGENTS,
    ..Provider::new("glm", glm)
};
/// OpenCode Go's plan.
pub static OPENCODE_GO: Provider = Provider {
    work_sources: OPEN_AGENTS,
    ..Provider::new("opencode", opencode_go)
};

/// Kimi Code's quota.
pub fn kimi(host: &dyn Host) -> Result<Reading, Unread> {
    read_provider(host, "kimi")
}
/// The GLM Coding Plan's quota.
pub fn glm(host: &dyn Host) -> Result<Reading, Unread> {
    read_provider(host, "glm")
}
/// OpenCode Go's quota.
pub fn opencode_go(host: &dyn Host) -> Result<Reading, Unread> {
    read_provider(host, "opencode")
}

/// A coding plan's vendor, as the price catalog names it, and its platform:
/// Kimi Code and Zhipu's plans on api.kimi.com and open.bigmodel.cn are
/// China's, those on api.kimi.ai and api.z.ai, and OpenCode Go,
/// international (agent-hud-native-open `OpenAgentCredentials.credential`).
fn plan_service(provider: &str, host: &str) -> (&'static str, &'static str) {
    let vendor = match provider {
        "kimi" => "Moonshot",
        "glm" => "GLM",
        _ => "OpenCode Go",
    };
    let region = if matches!(host, "api.kimi.com" | "open.bigmodel.cn") {
        "china"
    } else {
        "international"
    };
    (vendor, region)
}
