//! DeepSeek's API balance, or the Harness Platform account's separate wallets,
//! and the balance of every key Claude Code, Codex or Pi sends to DeepSeek's own
//! API (`routes.rs`). Credentials stay in the engine and are sent only to
//! their fixed official endpoints; no client or helper is launched.
use super::balance::{api_balance, Balances};
use super::platform::platform_balance;
use super::routes::{self, Routed};
use super::settings::{configured, Configured};
use crate::account::{account_id, legacy_unresolved, Host, Provider, Reading, Unread};
use crate::l10n;
use chrono::Utc;
use serde_json::{json, Value};

pub(super) fn decimal(value: &Value) -> Option<f64> {
    let raw = value.as_str()?;
    let magnitude = raw.strip_prefix('-').unwrap_or(raw);
    let mut parts = magnitude.split('.');
    let integer = parts.next()?;
    if integer.is_empty() || !integer.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if let Some(fraction) = parts.next() {
        if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
    }
    if parts.next().is_some() {
        return None;
    }
    raw.parse::<f64>().ok().filter(|value| value.is_finite())
}

/// DeepSeek's balance, with the key DeepSeek Harness or the user keeps, and
/// with the keys other clients send to DeepSeek's API.
pub static PROVIDER: Provider = Provider {
    ..Provider::new("deepseek", read)
};

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let mut routed = routes::keys(host);
    let own = read_text(host, &mut routed);
    if routed.is_empty() {
        return own;
    }
    let mut failures = Vec::new();
    let mut reading = match own {
        // Without a key of Harness's own, its notice gives way to the
        // clients' keys.
        Ok(mut reading) if reading.account.is_none() => {
            reading.notice = None;
            reading
        }
        Ok(reading) => reading,
        Err(unread) => {
            failures.push(unread);
            Reading::empty("deepseek")
        }
    };
    let checked = Utc::now().to_rfc3339();
    for route in routed {
        if host.cancelled() {
            return Err(l10n::text("账户读取已停止", "Account reading was stopped").into());
        }
        let names = client_names(&route.clients);
        let id = account_id("deepseek", &["api-key", &route.key]);
        let balances = match api_balance(host, &route.key) {
            Ok(balances) => balances,
            Err(unread) => {
                failures.push(unread.prefixed(&format!("{names}: ")));
                continue;
            }
        };
        let label = if l10n::chinese() {
            format!("{names} 的 API 余额")
        } else {
            format!("API balance ({names})")
        };
        for (currency, amount) in &balances.amounts {
            reading.billing.push(json!({"id":format!("{id}:{currency}"),"vendor":"deepseek","accountId":id,"clients":route.clients,"label":label,"amount":amount,"currency":currency,"available":balances.available,"notice":balances.notice()}));
        }
        reading
            .services
            .extend(services(&route.clients, &id, &balances));
        let account = json!({"id":id,"provider":"deepseek","label":"DeepSeek API","plan":null,"identity":"credential-scope","clients":route.clients,"checkedAt":checked});
        if reading.account.is_none() {
            reading.account = Some(account);
        } else {
            reading.additional_accounts.push(account);
        }
    }
    let failed = Unread::combined(failures);
    if reading.account.is_none() {
        return match failed {
            Some(unread) => Err(unread),
            None => Ok(reading),
        };
    }
    if let Some(unread) = failed {
        reading.notice = Some(match reading.notice.take() {
            Some(notice) => format!("{notice}\n{}", unread.reason()),
            None => unread.reason().to_owned(),
        });
    }
    Ok(reading)
}

/// The clients' names, as they call themselves.
fn client_names(clients: &[&str]) -> String {
    clients
        .iter()
        .map(|id| crate::by_id(id).map_or(*id, |agent| agent.name))
        .collect::<Vec<_>>()
        .join(if l10n::chinese() { "、" } else { ", " })
}

/// Which clients reach DeepSeek's API with the account, on the platform its
/// balance's money names (CNY for China's, USD for the international one).
fn services(clients: &[&str], id: &str, balances: &Balances) -> Vec<Value> {
    let region = match balances.amounts.as_slice() {
        [("CNY", _)] => Some("china"),
        [("USD", _)] => Some("international"),
        _ => None,
    };
    clients
        .iter()
        .map(|client| json!({"client":client,"provider":"DeepSeek","product":"api","accountId":id,"region":region}))
        .collect()
}

/// The balance of the key DeepSeek Harness or the user keeps, or the Harness
/// Platform account's wallets. A client in `routed` that sends the same key
/// is taken out and named on these rows.
fn read_text(host: &dyn Host, routed: &mut Vec<Routed>) -> Result<Reading, Unread> {
    let Configured {
        directory,
        endpoint,
        credential,
        credentials,
    } = configured(host)?;
    let Some(credential) = credential else {
        if let Some(reading) = platform_balance(host, &credentials, &directory)? {
            return Ok(reading);
        }
        let mut reading = Reading::empty("deepseek");
        reading.notice = Some(l10n::text(
            "请在 Harness 中配置 DeepSeek API Key 以读取余额",
            "Configure a DeepSeek API key in Harness to read its balance.",
        ));
        return Ok(reading);
    };
    let endpoint = url::Url::parse(&endpoint)
        .map_err(|_| l10n::text("DeepSeek API 地址无效", "DeepSeek API address is invalid."))?;
    if endpoint.scheme() != "https"
        || endpoint.host_str() != Some("api.deepseek.com")
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.port_or_known_default() != Some(443)
    {
        return Err(l10n::text(
            "自定义 API 地址无法读取 DeepSeek 官方余额",
            "Official DeepSeek balance is unavailable for a custom API endpoint.",
        )
        .into());
    }
    // The clients that send Harness's own key share its rows.
    let mut shared = Vec::new();
    routed.retain(|route| {
        let same = route.key == credential;
        if same {
            shared.extend(route.clients.iter().copied());
        }
        !same
    });
    let balances = api_balance(host, &credential)?;
    // The balance endpoint returns no account identity. The original retains it
    // under the Harness billing source; rotating a key must not create an account.
    let scope = host.canonical(&directory).unwrap_or(directory);
    let id = account_id("deepseek", &[scope.to_string_lossy().as_ref()]);
    let account_identity = legacy_unresolved("DeepSeek", &scope.to_string_lossy());
    let mut billing = Vec::new();
    for (currency, amount) in &balances.amounts {
        let mut row = json!({"id":format!("{id}:{currency}"),"vendor":"deepseek","accountId":account_identity,"consumerRoutes":["deepseek-official"],"consumerHome":scope,"label":l10n::text("API 余额", "API balance"),"amount":amount,"currency":currency,"available":balances.available,
            "notice":balances.notice()});
        if !shared.is_empty() {
            row["clients"] = json!(["deepseek"].iter().chain(&shared).collect::<Vec<_>>());
        }
        billing.push(row);
    }
    Ok(Reading {
        provider: "deepseek",
        failure: None,
        quotas: Vec::new(),
        billing,
        notice: None,
        additional_accounts: Vec::new(),
        session_records: Vec::new(),
        services: services(&shared, &account_identity, &balances),
        account: Some(
            json!({"id":account_identity,"provider":"deepseek","label":"DeepSeek API","plan":null,"identity":"local-home","checkedAt":Utc::now().to_rfc3339()}),
        ),
        unreviewed_hooks: vec![],
    })
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
