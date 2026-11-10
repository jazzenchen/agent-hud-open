//! Codex's quota: its CLI's own `app-server` (account/rateLimits/read and
//! account/read), and Pi's ChatGPT sign-in, which spends the same plan. The
//! same app-server also says which of Agent HUD's hooks wait for the user to
//! allow them (`hook_review`).
use super::hook_review;
use super::limits::parse;
use super::pi_usage::{read_pi, PI_ROUTE};
use crate::account::{
    append_notice, client_home_key, merge, variable, EngineStart, Host, Provider, Reading,
    StatusPage, Unread,
};
use crate::l10n;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::Path,
    time::{Duration, Instant},
};

/// Codex's plan, read through its CLI's app-server and Pi's ChatGPT
/// sign-in. Pi and other clients spend it without Codex rollouts here, so it
/// is read on the account interval whatever this computer does.
pub static PROVIDER: Provider = Provider {
    local_work: false,
    status: Some(&STATUS),
    ..Provider::new("codex", read)
};
/// OpenAI's status page names Codex by its desktop home.
static STATUS: StatusPage = StatusPage {
    url: "https://status.openai.com/api/v2/summary.json",
    components: &["Codex in ChatGPT Desktop", "Responses", "Login"],
};

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let reading = read_plan(host)?;
    Ok(prefer_relay_balance(host, reading))
}

/// Codex's plan, without the windows a relay invents when its calls go through
/// one: the relay answers `account/rateLimits/read` itself, so those windows
/// are not a plan this computer holds. Core shows every relay's balance under
/// the clients that spend it (`relayReading`), so the slot reads what the
/// account has left there instead of a meaningless share. The plan stays when
/// the owner has not agreed to read that relay.
fn prefer_relay_balance(host: &dyn Host, reading: Reading) -> Reading {
    let Some(provider) = crate::codex::providers::selected(host) else {
        return reading;
    };
    let Some(relay) = crate::endpoints::base_url_host(&provider.base_url) else {
        return reading;
    };
    if crate::endpoints::host_kind(&relay) != crate::endpoints::HostKind::Relay {
        return reading;
    }
    if !host.consented(&crate::relays::consent_key(&relay)) {
        return reading;
    }
    let mut reading = reading;
    reading.quotas.clear();
    reading
}

fn read_plan(host: &dyn Host) -> Result<Reading, Unread> {
    let executable = host.executable("codex");
    let native = match executable.as_deref() {
        Some(executable) => read_native(host, executable),
        None => Err(Unread::Failed(l10n::text(
            "安装并登录后即可读取额度",
            "Install and sign in to read quota",
        ))),
    };
    let pi = read_pi(host);
    match (native, pi) {
        (Ok(mut native), Ok(Some(pi))) => {
            // Pi signed in to the same ChatGPT account spends the same windows.
            for row in &mut native.quotas {
                if pi
                    .quotas
                    .iter()
                    .any(|quota| quota["accountId"] == row["accountId"])
                {
                    row["consumerRoutes"] = json!([PI_ROUTE]);
                }
            }
            merge(&mut native, pi);
            Ok(native)
        }
        (Ok(mut native), Err(error)) => {
            append_notice(&mut native, error.reason().to_owned());
            Ok(native)
        }
        (Ok(native), _) => Ok(native),
        (Err(error), Ok(Some(mut pi))) => {
            // Pi can be the only client installed. A configured Codex home
            // still needs its CLI, so a missing executable matters there.
            if executable.is_some() || !host.account_homes("codex").is_empty() {
                append_notice(&mut pi, error.reason().to_owned());
            }
            Ok(pi)
        }
        (Err(native), Err(pi)) => Err(Unread::combined(vec![native, pi]).expect("two failures")),
        (Err(error), _) => Err(error),
    }
}

fn read_native(host: &dyn Host, executable: &Path) -> Result<Reading, Unread> {
    let home = variable(host, "CODEX_HOME").unwrap_or_else(|| host.home().join(".codex"));
    let mut seen = HashSet::new();
    let homes: Vec<_> = std::iter::once(home)
        .chain(host.account_homes("codex"))
        .map(|home| host.canonical(&home).unwrap_or(home))
        .filter(|home| seen.insert(home.clone()))
        .collect();
    let mut reading = Reading::empty("codex");
    let mut succeeded = false;
    let mut failures = Vec::new();
    for home in &homes {
        if host.cancelled() {
            return Err(l10n::text("账户读取已停止", "Account reading was stopped").into());
        }
        let prefix = if homes.len() > 1 {
            format!("{}: ", home.display())
        } else {
            String::new()
        };
        let notice = match read_home(host, executable, home) {
            Ok(mut account) => {
                succeeded = true;
                let notice = account.notice.take();
                merge(&mut reading, account);
                notice
            }
            Err(error) => {
                let error = error.prefixed(&prefix);
                let notice = error.reason().to_owned();
                failures.push(error);
                append_notice(&mut reading, notice);
                None
            }
        };
        if let Some(notice) = notice {
            append_notice(&mut reading, format!("{prefix}{notice}"));
        }
    }
    if succeeded {
        Ok(reading)
    } else {
        Err(Unread::combined(failures).expect("a failure per home"))
    }
}

fn read_home(host: &dyn Host, executable: &Path, home: &Path) -> Result<Reading, Unread> {
    let mut process = host.start_engine(&EngineStart {
        executable,
        args: &["app-server", "--listen", "stdio://"],
        folder: "codex",
        env: &[("CODEX_HOME", home.to_string_lossy().into_owned())],
    })?;
    process.send(&json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"agent_hud","title":"Agent HUD","version":"0.1.0"}}}))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut grace = deadline;
    let mut limits = None;
    let mut refused = None::<String>;
    let mut account = Value::Null;
    let mut requires_openai_auth = None;
    let mut answered = false;
    // An app-server too old for hooks/list answers with an error: none wait.
    let mut hooks = None::<Vec<String>>;
    while let Some(message) = process.next(deadline.min(grace)) {
        let Some(id) = message["id"].as_u64() else {
            continue;
        };
        if message["error"].is_object() {
            // An engine without account/read still reports limits, only
            // without the signed-in email.
            if id == 3 {
                answered = true;
            } else if id == 4 {
                hooks = Some(Vec::new());
            } else if id == 2 {
                // Limits need a ChatGPT sign-in; account/read says why
                // there is none.
                refused = Some(
                    message["error"]["message"]
                        .as_str()
                        .unwrap_or("app-server error")
                        .to_owned(),
                );
                grace = Instant::now() + Duration::from_secs(2);
            } else {
                return Err(format!(
                    "Codex: {}",
                    message["error"]["message"]
                        .as_str()
                        .unwrap_or("app-server error")
                )
                .into());
            }
        } else if let Some(result) = message.get("result") {
            match id {
                1 => {
                    process.send(&json!({"method":"initialized"}))?;
                    process.send(&json!({"id":2,"method":"account/rateLimits/read"}))?;
                    process.send(&json!({"id":3,"method":"account/read","params":{}}))?;
                    process.send(&hook_review::request(4, host.home()))?;
                }
                2 => {
                    limits = Some(result.clone());
                    grace = Instant::now() + Duration::from_secs(2);
                }
                3 => {
                    account = result["account"].clone();
                    requires_openai_auth = result["requiresOpenaiAuth"].as_bool();
                    answered = true;
                }
                4 => hooks = Some(hook_review::unreviewed(result)),
                _ => {}
            }
        }
        if (limits.is_some() || refused.is_some()) && answered && hooks.is_some() {
            break;
        }
    }
    let unreviewed_hooks = hooks.unwrap_or_default();
    if let (None, Some(error)) = (&limits, refused) {
        // No sign-in at all is Codex's own state: the last windows stay.
        if requires_openai_auth == Some(true) && account.is_null() {
            return Err(Unread::SignedOut(l10n::text(
                "Codex 尚未登录，运行 codex login 登录 ChatGPT 后即可读取额度",
                "Codex is not signed in. Run codex login with ChatGPT to read its quota",
            )));
        }
        return match no_plan_notice(&account, requires_openai_auth) {
            Some(notice) => {
                let mut reading = Reading::empty("codex");
                reading.notice = Some(notice);
                reading.unreviewed_hooks = unreviewed_hooks;
                Ok(reading)
            }
            None => Err(format!("Codex: {error}").into()),
        };
    }
    let limits = limits.ok_or_else(|| {
        if process.ended() {
            Unread::Failed(l10n::text(
                "Codex 引擎提前退出，请检查本机登录状态",
                "Codex exited before reporting limits; check local sign-in",
            ))
        } else {
            Unread::transient(l10n::text(
                "Codex 额度查询超时",
                "Codex quota query timed out",
            ))
        }
    })?;
    parse(
        host,
        &limits,
        &account,
        &home.to_string_lossy(),
        &client_home_key(host, home, &host.home().join(".codex")),
        true,
    )
    .map(|mut reading| {
        reading.unreviewed_hooks = unreviewed_hooks;
        reading
    })
    .map_err(Unread::from)
}

/// Why a Codex home has no ChatGPT plan limits, as account/read tells it: an
/// API key, Amazon Bedrock, a model provider that needs no OpenAI sign-in, or
/// no sign-in at all. None when it does not say.
fn no_plan_notice(account: &Value, requires_openai_auth: Option<bool>) -> Option<String> {
    match account["type"].as_str() {
        Some("apiKey") => Some(l10n::text(
            "Codex 用的是 API key 登录，没有 ChatGPT 套餐额度",
            "Codex is signed in with an API key, which has no ChatGPT plan limits",
        )),
        Some("amazonBedrock") => Some(l10n::text(
            "Codex 走的是 Amazon Bedrock，没有 ChatGPT 套餐额度",
            "Codex runs on Amazon Bedrock, which has no ChatGPT plan limits",
        )),
        Some(_) => None,
        None => match requires_openai_auth {
            Some(false) => Some(l10n::text(
                "Codex 用的是自定义模型服务，没有 ChatGPT 套餐额度",
                "Codex uses a custom model provider, which has no ChatGPT plan limits",
            )),
            _ => None,
        },
    }
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
