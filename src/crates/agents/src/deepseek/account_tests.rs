use super::*;
use crate::account::fake::FakeHost;
use crate::account::{Answer, HttpFailure};
use crate::deepseek::balance::BALANCE;
use crate::deepseek::platform::{platform_amount, PLATFORM_BALANCE};
use crate::deepseek::settings::unreadable;

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
        let error = read(&host).err().unwrap().reason().to_owned();
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
        let error = read(&host).err().unwrap().reason().to_owned();
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

const CLAUDE_ROUTE: &str = r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.deepseek.com/anthropic","ANTHROPIC_AUTH_TOKEN":"sk-claude"}}"#;
const CODEX_ROUTE: &str = "[model_providers.deepseek]\nbase_url = \"https://api.deepseek.com\"\nexperimental_bearer_token = \"sk-codex\"\n";

/// Everything a reading hands on, as text.
fn shown(reading: &Reading) -> String {
    serde_json::to_string(&json!({
        "billing": reading.billing, "account": reading.account,
        "accounts": reading.additional_accounts, "services": reading.services,
        "notice": reading.notice,
    }))
    .unwrap()
}

#[test]
fn a_key_claude_code_sends_to_deepseek_reads_its_balance() {
    let host = FakeHost::new("/u")
        .file("/u/.claude/settings.json", CLAUDE_ROUTE)
        .answer(BALANCE, Ok(balance("7.25")));
    let reading = read(&host).unwrap();
    assert_eq!(
        *host.asked.borrow(),
        [(BALANCE.into(), Some("Bearer sk-claude".into()))],
        "the key goes to DeepSeek's balance endpoint and nowhere else"
    );
    assert!(reading.notice.is_none(), "Harness's own notice gives way");
    assert_eq!(reading.billing.len(), 1);
    let id = account_id("deepseek", &["api-key", "sk-claude"]);
    let row = &reading.billing[0];
    assert_eq!(row["id"], format!("{id}:CNY"));
    assert_eq!(row["vendor"], "deepseek");
    assert_eq!(row["accountId"], id.as_str());
    assert_eq!(row["clients"], json!(["claude"]));
    assert_eq!(row["label"], "API balance (Claude Code)");
    assert_eq!(row["amount"], 7.25);
    assert!(row["consumerHome"].is_null(), "not Harness's home");
    let account = reading.account.as_ref().unwrap();
    assert_eq!(account["id"], id.as_str());
    assert_eq!(account["identity"], "credential-scope");
    assert_eq!(
        reading.services,
        [
            json!({"client":"claude","provider":"DeepSeek","product":"api","accountId":id,"region":"china"})
        ]
    );
    assert!(!shown(&reading).contains("sk-claude"));
}

#[test]
fn harness_and_the_clients_share_one_key_once() {
    let host = FakeHost::new("/u")
        .env("DEEPSEEK_API_KEY", "sk-codex")
        .file("/u/.codex/config.toml", CODEX_ROUTE)
        .answer(BALANCE, Ok(balance("3")));
    let reading = read(&host).unwrap();
    assert_eq!(host.asked.borrow().len(), 1);
    assert_eq!(reading.billing.len(), 1);
    let row = &reading.billing[0];
    assert_eq!(row["label"], "API balance");
    assert_eq!(row["clients"], json!(["deepseek", "codex"]));
    assert_eq!(row["consumerRoutes"], json!(["deepseek-official"]));
    assert_eq!(reading.services[0]["client"], "codex");
    assert_eq!(reading.services[0]["accountId"], row["accountId"]);
    assert!(reading.additional_accounts.is_empty());
}

#[test]
fn each_clients_own_key_is_its_own_account() {
    let usd = json!({"is_available": false, "balance_infos": [
        {"currency": "USD", "total_balance": "0.40", "granted_balance": "0", "topped_up_balance": "0.40"}
    ]});
    let host = FakeHost::new("/u")
        .env("DEEPSEEK_API_KEY", "sk-harness")
        .file("/u/.claude/settings.json", CLAUDE_ROUTE)
        .file("/u/.codex/config.toml", CODEX_ROUTE)
        .answer_for(BALANCE, "Bearer sk-harness", Ok(balance("1")))
        .answer_for(BALANCE, "Bearer sk-claude", Ok(balance("2")))
        .answer_for(BALANCE, "Bearer sk-codex", Ok(usd));
    let reading = read(&host).unwrap();
    let sent = host
        .asked
        .borrow()
        .iter()
        .map(|(url, key)| (url.clone(), key.clone().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        sent,
        [
            (BALANCE.to_owned(), "Bearer sk-harness".to_owned()),
            (BALANCE.to_owned(), "Bearer sk-claude".to_owned()),
            (BALANCE.to_owned(), "Bearer sk-codex".to_owned()),
        ]
    );
    assert_eq!(reading.billing.len(), 3);
    assert_eq!(reading.billing[0]["label"], "API balance");
    assert_eq!(reading.billing[2]["label"], "API balance (Codex)");
    assert_eq!(reading.billing[2]["available"], false);
    assert!(reading.billing[2]["notice"].is_string());
    assert_eq!(reading.additional_accounts.len(), 2);
    assert_eq!(reading.services[1]["region"], "international");
    assert!(!shown(&reading).contains("sk-"));
}

#[test]
fn a_clients_key_that_fails_is_a_notice_or_the_reading_s_failure() {
    let host = FakeHost::new("/u")
        .env("DEEPSEEK_API_KEY", "sk-harness")
        .file("/u/.claude/settings.json", CLAUDE_ROUTE)
        .answer_for(BALANCE, "Bearer sk-harness", Ok(balance("1")))
        .answer_for(BALANCE, "Bearer sk-claude", Err(HttpFailure::Status(401)));
    let reading = read(&host).unwrap();
    assert_eq!(reading.billing.len(), 1);
    assert_eq!(
        reading.notice.as_deref(),
        Some(format!("Claude Code: {}", HttpFailure::Status(401).message()).as_str())
    );

    let host = FakeHost::new("/u").file("/u/.claude/settings.json", CLAUDE_ROUTE);
    let failed = read(&host).err().unwrap();
    assert!(matches!(failed, Unread::Transient { .. }), "{failed:?}");
    assert_eq!(
        failed,
        "Claude Code: DeepSeek balance could not be reached; check the connection."
    );
    assert!(!failed.reason().contains("sk-claude"));
}

#[test]
fn a_broken_harness_setup_does_not_hide_a_clients_balance() {
    let host = FakeHost::new("/u")
        .env("DEEPSEEK_API_KEY", "sk-harness")
        .env("DEEPSEEK_BASE_URL", "https://proxy.example.com")
        .file("/u/.claude/settings.json", CLAUDE_ROUTE)
        .answer(BALANCE, Ok(balance("5")));
    let reading = read(&host).unwrap();
    assert_eq!(reading.billing.len(), 1);
    assert_eq!(reading.billing[0]["clients"], json!(["claude"]));
    assert_eq!(
        reading.notice.as_deref(),
        Some("Official DeepSeek balance is unavailable for a custom API endpoint.")
    );
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
