//! DeepSeek's API balance, or the Harness Platform account's separate wallets.
//! Credentials stay in the engine and are sent only to their fixed official
//! endpoints; no client or helper is launched.
use crate::account::{
    account_id, legacy_unresolved, Answer, Host, HttpFailure, HttpRequest, Reading,
};
use crate::l10n;
use chrono::Utc;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const BALANCE: &str = "https://api.deepseek.com/user/balance";
const PLATFORM_ORIGIN: &str = "https://platform.deepseek.com";
const PLATFORM_BALANCE: &str = "https://platform.deepseek.com/api/v0/users/get_user_summary";
const PLATFORM_RECORD: &str = "deepseek-account-platform/default";

fn unreadable() -> String {
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

fn decimal(value: &Value) -> Option<f64> {
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

/// The Harness account plugin accepts decimal or scientific notation for its
/// wallet balances. Keep the API-key reader's stricter contract unchanged.
fn platform_amount(value: &Value) -> Option<f64> {
    let raw = value.as_str()?;
    if raw.is_empty() || raw.starts_with('+') || raw.bytes().any(|c| c.is_ascii_whitespace()) {
        return None;
    }
    let amount = raw.parse::<f64>().ok()?;
    amount.is_finite().then_some(amount)
}

fn platform_balance(
    host: &dyn Host,
    credentials: &Value,
    directory: &Path,
) -> Result<Option<Reading>, String> {
    let record = &credentials["records"][PLATFORM_RECORD];
    if record.is_null() {
        return Ok(None);
    }
    let payload = &record["payload"];
    let token = payload["token"].as_str().filter(|token| {
        !token.is_empty() && token.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    });
    if record["kind"] != "grant"
        || payload["version"] != 1
        || payload["issuer"] != PLATFORM_ORIGIN
        || token.is_none()
    {
        return Err(l10n::text(
            "DeepSeek 平台登录凭据格式或来源无法识别",
            "DeepSeek Platform sign-in has an unsupported format or issuer.",
        ));
    }
    let platform = match host.os() {
        crate::Os::Windows => "desktop-win",
        crate::Os::MacOs => "desktop-mac",
        crate::Os::Linux => "web",
    };
    let body = host
        .http_json(&HttpRequest {
            url: PLATFORM_BALANCE,
            secrets: &[("x-dsh-auth-token", token.unwrap())],
            body: None,
            headers: &[("x-client-platform", platform)],
            timeout: Duration::from_secs(15),
        })
        .map_err(|failure| match failure {
            HttpFailure::Unreachable | HttpFailure::BadHeader => l10n::text(
                "DeepSeek 余额读取失败，请检查网络后重试",
                "DeepSeek balance could not be reached; check the connection.",
            ),
            other => other.message(),
        })?;
    if body["code"] == 40003 {
        return Err(HttpFailure::Status(401).message());
    }
    let summary = &body["data"]["biz_data"];
    if body["code"] != 0 || body["data"]["biz_code"] != 0 {
        return Err(l10n::text(
            "DeepSeek 平台余额读取失败",
            "DeepSeek Platform balance could not be read.",
        ));
    }
    let scope = host
        .canonical(directory)
        .unwrap_or_else(|| directory.to_path_buf());
    let id = account_id("deepseek", &[scope.to_string_lossy().as_ref()]);
    let identity = legacy_unresolved("DeepSeek", &scope.to_string_lossy());
    let mut billing = Vec::new();
    for (kind, field) in [("normal", "normal_wallets"), ("bonus", "bonus_wallets")] {
        let wallets = summary[field].as_array().ok_or_else(|| {
            l10n::text(
                "DeepSeek 平台余额格式无法识别",
                "DeepSeek Platform balance has an unsupported format.",
            )
        })?;
        let mut currencies = std::collections::HashSet::new();
        for wallet in wallets {
            let currency = wallet["currency"]
                .as_str()
                .filter(|c| matches!(*c, "CNY" | "USD"))
                .ok_or_else(|| {
                    l10n::text(
                        "DeepSeek 余额的币种无法识别",
                        "DeepSeek balance response has an unsupported currency.",
                    )
                })?;
            let amount = platform_amount(&wallet["balance"]).ok_or_else(|| {
                l10n::text(
                    "DeepSeek 余额金额无效",
                    "DeepSeek balance response has an invalid amount.",
                )
            })?;
            if !currencies.insert(currency) {
                return Err(l10n::text(
                    "DeepSeek 平台余额格式无法识别",
                    "DeepSeek Platform balance has an unsupported format.",
                ));
            }
            // Harness displays granted wallets only while positive.
            if kind == "bonus" && amount <= 0.0 {
                continue;
            }
            let label = if kind == "normal" {
                l10n::text("充值余额", "Topped-up balance")
            } else {
                l10n::text("赠送余额", "Granted balance")
            };
            // Platform login wallets are not proven to be an API-key billing
            // route, so do not attach a consumer route or estimate its costs.
            billing.push(json!({"id":format!("{id}:platform:{kind}:{currency}"),"vendor":"deepseek","accountId":identity,"label":label,"amount":amount,"currency":currency}));
        }
    }
    Ok(Some(Reading {
        provider: "deepseek",
        quotas: Vec::new(),
        billing,
        account: Some(
            json!({"id":identity,"provider":"deepseek","label":"DeepSeek Platform","plan":null,"identity":"local-home","checkedAt":Utc::now().to_rfc3339()}),
        ),
        additional_accounts: Vec::new(),
        notice: None,
        session_records: Vec::new(),
        services: Vec::new(),
    }))
}

pub fn read(host: &dyn Host) -> Result<Reading, String> {
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
        .unwrap_or("https://api.deepseek.com");
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
                ));
            }
            credential = credentials["refs"][reference].as_str().map(str::to_owned);
        }
    }
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
    let endpoint = url::Url::parse(endpoint)
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
        ));
    }
    if credential
        .bytes()
        .any(|byte| matches!(byte, 0 | b'\r' | b'\n'))
    {
        return Err(l10n::text(
            "DeepSeek 凭据格式无法识别",
            "DeepSeek credentials have an unsupported format.",
        ));
    }
    let body = host
        .http_json(&HttpRequest {
            url: BALANCE,
            secrets: &[("Authorization", &format!("Bearer {credential}"))],
            body: None,
            headers: &[],
            timeout: Duration::from_secs(15),
        })
        .map_err(|failure| match failure {
            HttpFailure::Unreachable | HttpFailure::BadHeader => l10n::text(
                "DeepSeek 余额读取失败，请检查网络后重试",
                "DeepSeek balance could not be reached; check the connection.",
            ),
            other => other.message(),
        })?;
    let available = body["is_available"].as_bool().ok_or_else(|| {
        l10n::text(
            "DeepSeek 余额返回的数据格式无法识别",
            "DeepSeek balance response has an unsupported format.",
        )
    })?;
    let balances = body["balance_infos"].as_array().ok_or_else(|| {
        l10n::text(
            "DeepSeek 余额返回的数据格式无法识别",
            "DeepSeek balance response has an unsupported format.",
        )
    })?;
    // The balance endpoint returns no account identity. The original retains it
    // under the Harness billing source; rotating a key must not create an account.
    let scope = host.canonical(&directory).unwrap_or(directory);
    let id = account_id("deepseek", &[scope.to_string_lossy().as_ref()]);
    let account_identity = legacy_unresolved("DeepSeek", &scope.to_string_lossy());
    let mut billing = Vec::new();
    for row in balances {
        let currency = row["currency"]
            .as_str()
            .filter(|currency| matches!(*currency, "CNY" | "USD"))
            .ok_or_else(|| {
                l10n::text(
                    "DeepSeek 余额的币种无法识别",
                    "DeepSeek balance response has an unsupported currency.",
                )
            })?;
        let amount = decimal(&row["total_balance"]).ok_or_else(|| {
            l10n::text(
                "DeepSeek 余额金额无效",
                "DeepSeek balance response has an invalid amount.",
            )
        })?;
        if decimal(&row["granted_balance"]).is_none()
            || decimal(&row["topped_up_balance"]).is_none()
        {
            return Err(l10n::text(
                "DeepSeek 余额金额无效",
                "DeepSeek balance response has an invalid amount.",
            ));
        }
        billing.push(json!({"id":format!("{id}:{currency}"),"vendor":"deepseek","accountId":account_identity,"consumerRoutes":["deepseek-official"],"consumerHome":scope,"label":l10n::text("API 余额", "API balance"),"amount":amount,"currency":currency,"available":available,
            "notice":(!available).then(|| l10n::text("DeepSeek 表示此余额当前不能用于 API 调用", "DeepSeek reports that this balance is unavailable for API calls."))}));
    }
    Ok(Reading {
        provider: "deepseek",
        quotas: Vec::new(),
        billing,
        notice: None,
        additional_accounts: Vec::new(),
        session_records: Vec::new(),
        services: Vec::new(),
        account: Some(
            json!({"id":account_identity,"provider":"deepseek","label":"DeepSeek API","plan":null,"identity":"local-home","checkedAt":Utc::now().to_rfc3339()}),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    const GRANT: &str = "version: 1\nrecords:\n  deepseek-account-platform/default:\n    kind: grant\n    payload:\n      version: 1\n      token: sample-grant\n      issuer: https://platform.deepseek.com\n";

    fn platform_wallets() -> Value {
        json!({"code": 0, "data": {"biz_code": 0, "biz_data": {
            "normal_wallets": [{"currency":"CNY","balance":"9.92"}, {"currency":"USD","balance":"0.00"}],
            "bonus_wallets": [{"currency":"CNY","balance":"1.25"}, {"currency":"USD","balance":"0"}]
        }}})
    }

    #[test]
    fn harness_platform_grant_reads_independent_wallets_on_each_os() {
        for os in [crate::Os::MacOs, crate::Os::Windows, crate::Os::Linux] {
            let mut host = FakeHost::new("/u")
                .file("/u/.dsh/.credentials.yaml", GRANT)
                .answer(PLATFORM_BALANCE, Ok(platform_wallets()));
            host.os = os;
            let reading = read(&host).unwrap();
            assert_eq!(host.asked.borrow()[0].0, PLATFORM_BALANCE);
            assert_eq!(
                host.sent.borrow()[0].0,
                [("x-dsh-auth-token".into(), "sample-grant".into())]
            );
            let platform = match os {
                crate::Os::Windows => "desktop-win",
                crate::Os::MacOs => "desktop-mac",
                crate::Os::Linux => "web",
            };
            assert_eq!(
                host.headers.borrow()[0],
                [("x-client-platform".into(), platform.into())]
            );
            assert_eq!(reading.billing.len(), 3);
            assert_eq!(reading.billing[0]["currency"], "CNY");
            assert_eq!(reading.billing[0]["amount"], 9.92);
            assert_eq!(reading.billing[0]["label"], "Topped-up balance");
            assert_eq!(reading.billing[1]["currency"], "USD");
            assert_eq!(reading.billing[1]["amount"], 0.0);
            assert_eq!(reading.billing[2]["label"], "Granted balance");
            assert_eq!(reading.billing[2]["amount"], 1.25);
            assert_ne!(reading.billing[0]["id"], reading.billing[2]["id"]);
            assert!(reading
                .billing
                .iter()
                .all(|row| row["consumerRoutes"].is_null() && row["available"].is_null()));
        }
    }

    #[test]
    fn custom_api_endpoint_does_not_block_the_separate_platform_grant() {
        let host = FakeHost::new("/u")
            .file("/u/.dsh/.credentials.yaml", GRANT)
            .env("DEEPSEEK_BASE_URL", "https://proxy.example.com")
            .answer(PLATFORM_BALANCE, Ok(platform_wallets()));
        assert_eq!(read(&host).unwrap().billing.len(), 3);
        assert_eq!(host.asked.borrow()[0].0, PLATFORM_BALANCE);
    }

    #[test]
    fn an_api_key_takes_precedence_over_platform_grant() {
        let host = FakeHost::new("/u")
            .file("/u/.dsh/.credentials.yaml", GRANT)
            .env("DEEPSEEK_API_KEY", "sample-key")
            .answer(BALANCE, Ok(balance("3")));
        let reading = read(&host).unwrap();
        assert_eq!(
            host.asked.borrow()[0],
            (BALANCE.into(), Some("Bearer sample-key".into()))
        );
        assert_eq!(reading.billing[0]["label"], "API balance");
        assert_eq!(host.asked.borrow().len(), 1);
    }

    #[test]
    fn platform_grant_requires_exact_issuer_and_valid_format_without_sending() {
        for invalid in [
            GRANT.replace(
                "https://platform.deepseek.com",
                "https://platform.deepseek.com.evil.example",
            ),
            GRANT.replace(
                "https://platform.deepseek.com",
                "https://platform.deepseek.com/path",
            ),
            GRANT.replace(
                "https://platform.deepseek.com",
                "http://platform.deepseek.com",
            ),
            GRANT.replace("sample-grant", "\"\""),
            GRANT.replace("kind: grant", "kind: reference"),
            GRANT.replace("      version: 1", "      version: 2"),
        ] {
            let host = FakeHost::new("/u").file("/u/.dsh/.credentials.yaml", &invalid);
            let error = read(&host).err().unwrap();
            assert!(!error.contains("sample-grant"));
            assert!(host.asked.borrow().is_empty());
        }
    }

    #[test]
    fn platform_failures_do_not_become_zero_balances_or_leak_credentials() {
        for body in [
            json!({"code": 40003}),
            json!({"code": 0, "data": {"biz_code": 9}}),
            json!({"code": 0, "data": {"biz_code": 0, "biz_data": {"normal_wallets": [], "bonus_wallets": [{"currency":"CNY", "balance":"NaN"}]}}}),
            json!({"code": 0, "data": {"biz_code": 0, "biz_data": {"normal_wallets": [{"currency":"EUR", "balance":"1"}], "bonus_wallets": []}}}),
        ] {
            let host = FakeHost::new("/u")
                .file("/u/.dsh/.credentials.yaml", GRANT)
                .answer(PLATFORM_BALANCE, Ok(body));
            let error = read(&host).err().unwrap();
            assert!(!error.contains("sample-grant"));
        }
    }

    #[test]
    fn platform_amounts_allow_documented_decimal_notation_only() {
        for value in [".5", "1.", "1e3", "-1.2e-2"] {
            assert!(platform_amount(&json!(value)).is_some());
        }
        for value in ["", "+1", "NaN", "inf", "1e999", " 1"] {
            assert!(platform_amount(&json!(value)).is_none());
        }
    }

    fn balance(total: &str) -> Value {
        json!({"is_available": true, "balance_infos": [
            {"currency": "CNY", "total_balance": total, "granted_balance": "0.00", "topped_up_balance": total}
        ]})
    }

    #[test]
    fn harness_refs_name_the_key() {
        let host = FakeHost::new("/u")
            .file(
                "/u/.dsh/settings.yaml",
                "llm-deepseek:\n  apiKeyEnv: MY_KEY\n",
            )
            .file(
                "/u/.dsh/.credentials.yaml",
                "version: 1\nrefs:\n  MY_KEY: sk-1\n",
            )
            .answer(BALANCE, Ok(balance("12.50")));
        let reading = read(&host).unwrap();
        assert_eq!(
            host.asked.borrow()[0],
            (BALANCE.into(), Some("Bearer sk-1".into()))
        );
        let row = &reading.billing[0];
        assert_eq!(row["amount"], 12.5);
        assert_eq!(row["currency"], "CNY");
        let home = host.home().join(".dsh").to_string_lossy().into_owned();
        assert_eq!(row["consumerHome"], home);
        let identity = legacy_unresolved("DeepSeek", &home);
        assert_eq!(row["accountId"], identity.as_str());
        assert_eq!(
            row["id"],
            format!("{}:CNY", account_id("deepseek", &[&home]))
        );
        assert_eq!(reading.account.unwrap()["id"], identity.as_str());
    }

    #[test]
    fn a_saved_key_wins_and_skips_harness_settings() {
        let mut host = FakeHost::new("/u")
            .env("DSH_HOME", "~/h")
            .env("DEEPSEEK_API_KEY", "sk-env")
            .file("/u/h/settings.yaml", "not: [valid")
            .answer(BALANCE, Ok(balance("1")));
        host.saved = Ok(Some(json!({"apiKey": "sk-saved"})));
        read(&host).unwrap();
        assert_eq!(host.asked.borrow()[0].1.as_deref(), Some("Bearer sk-saved"));
    }

    #[test]
    fn without_a_key_it_says_where_to_set_one() {
        let host = FakeHost::new("/u");
        let reading = read(&host).unwrap();
        assert!(reading.notice.is_some());
        assert!(host.asked.borrow().is_empty());
    }

    #[test]
    fn a_custom_endpoint_has_no_official_balance() {
        let host = FakeHost::new("/u")
            .env("DEEPSEEK_API_KEY", "sk")
            .env("DEEPSEEK_BASE_URL", "https://proxy.example.com");
        assert_eq!(
            read(&host).err().unwrap(),
            "Official DeepSeek balance is unavailable for a custom API endpoint."
        );
    }

    #[test]
    fn unreadable_and_oversized_settings_are_errors() {
        let mut host = FakeHost::new("/u");
        host.files
            .insert("/u/.dsh/settings.yaml".into(), Answer::Denied);
        assert_eq!(read(&host).err().unwrap(), unreadable());
        let big = format!("a: b\n#{}", "x".repeat(1024 * 1024));
        let host = FakeHost::new("/u").file("/u/.dsh/settings.yaml", &big);
        assert_eq!(
            read(&host).err().unwrap(),
            "DeepSeek settings or credentials exceeded the size limit."
        );
    }

    #[test]
    fn amounts_must_be_plain_decimals() {
        assert_eq!(decimal(&json!("12.50")), Some(12.5));
        assert_eq!(decimal(&json!("-3")), Some(-3.0));
        for bad in ["", "1.", ".5", "1e3", "1.2.3", "NaN"] {
            assert_eq!(decimal(&json!(bad)), None, "{bad}");
        }
        assert_eq!(decimal(&json!(12.5)), None);
    }

    #[test]
    fn network_failures_are_worded_for_deepseek() {
        let host = FakeHost::new("/u").env("DEEPSEEK_API_KEY", "sk");
        assert_eq!(
            read(&host).err().unwrap(),
            "DeepSeek balance could not be reached; check the connection."
        );
        let host = FakeHost::new("/u")
            .env("DEEPSEEK_API_KEY", "sk")
            .answer(BALANCE, Err(HttpFailure::Status(429)));
        assert_eq!(
            read(&host).err().unwrap(),
            HttpFailure::Status(429).message()
        );
    }
}
