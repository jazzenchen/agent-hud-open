use super::*;
use crate::account::fake::FakeHost;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

const KIMI_USAGES: &str = "https://api.kimi.com/coding/v1/usages";
const KIMI_ME: &str = "https://api.kimi.com/coding/v1/me";
const GO_USAGE: &str = "https://opencode.ai/zen/go/v1/usage";
const GLM_GLOBAL: &str = "https://api.z.ai/api/monitor/usage/quota/limit";
const GLM_CHINA: &str = "https://open.bigmodel.cn/api/monitor/usage/quota/limit";

fn kimi_usages() -> Value {
    json!({"membership": {"level": "Andante"}, "usages": {
        "limit_5h": {"used_ratio": 0.25, "reset_time": "2026-10-22T10:00:00Z"},
        "limit_7d": {"used_ratio": "0.5"},
    }})
}

fn glm_limits() -> Value {
    let soon = chrono::Utc::now().timestamp_millis() + 3_600_000;
    json!({"success": true, "code": 200, "data": {"planName": "Pro", "limits": [
        {"type": "TOKENS_LIMIT", "unit": 3, "number": 5, "percentage": 40, "nextResetTime": soon},
        {"type": "TIME_LIMIT", "unit": 5, "number": 1, "percentage": 10,
         "usage": 100, "currentValue": 12, "remaining": 88},
    ]}})
}

#[test]
fn one_key_in_two_clients_is_one_account_serving_both() {
    let host = FakeHost::new("/u")
        .env("KIMI_CODE_API_KEY", "sk-kimi")
        .env("KIMI_API_KEY", "sk-kimi")
        .answer(KIMI_USAGES, Ok(kimi_usages()));
    let reading = kimi(&host).unwrap();
    let id = account_id(
        "kimi",
        &["https://api.kimi.com/coding/v1", "sk-kimi", "", ""],
    );
    let account = reading.account.unwrap();
    assert_eq!(account["id"], id.as_str());
    assert_eq!(account["plan"], "Andante");
    assert_eq!(account["identityEvidence"], "credential-scope");
    assert_eq!(reading.quotas.len(), 2);
    assert_eq!(
        reading.quotas[0]["id"],
        format!("{id}:limit:TIME_UNIT_MINUTE:300.0")
    );
    assert_eq!(reading.quotas[0]["usedPercent"], 25.0);
    assert_eq!(reading.quotas[0]["resetAt"], "2026-10-22T10:00:00+00:00");
    assert_eq!(reading.quotas[1]["usedPercent"], 50.0);
    let clients: Vec<_> = reading
        .services
        .iter()
        .map(|service| service["client"].as_str().unwrap())
        .collect();
    assert_eq!(clients, ["Kimi", "Pi"]);
    assert_eq!(reading.services[0]["provider"], "Moonshot");
    assert_eq!(reading.services[0]["region"], "china");
    let asked = host.asked.borrow();
    assert_eq!(asked[0].0, KIMI_ME);
    assert_eq!(
        asked[1],
        (KIMI_USAGES.into(), Some("Bearer sk-kimi".into()))
    );
}

#[test]
fn kimi_names_the_account_from_its_profile() {
    let host = FakeHost::new("/u")
        .env("KIMI_CODE_API_KEY", "sk")
        .answer(
            KIMI_ME,
            Ok(json!({"user_id": "u1", "region": "cn", "domain": 2})),
        )
        .answer(KIMI_USAGES, Ok(kimi_usages()));
    let account = kimi(&host).unwrap().account.unwrap();
    assert_eq!(
        account["id"],
        account_id("kimi", &["api.kimi.com", "u1", "2", "cn"]).as_str()
    );
    assert_eq!(account["identityEvidence"], "provider-account");
}

#[test]
fn kimi_names_the_person_and_plan_from_its_profile() {
    // Kimi Code 2.5.2's usages answer names no membership; its profile does.
    let usages = json!({"usages": {"limit_5h": {"used_ratio": 0.25}}});
    let reading = |me: Value| {
        let host = FakeHost::new("/u")
            .env("KIMI_CODE_API_KEY", "sk")
            .answer(KIMI_ME, Ok(me))
            .answer(KIMI_USAGES, Ok(usages.clone()));
        kimi(&host).unwrap()
    };
    let read = reading(json!({"user_id": "u1", "region": "cn", "domain": 2,
        "nickname": "Jun", "email": "jun@example.com", "user_level_name": "Allegretto"}));
    let account = read.account.unwrap();
    assert_eq!(account["label"], "jun@example.com");
    assert_eq!(account["plan"], "Allegretto");
    assert_eq!(read.quotas[0]["account"], "jun@example.com");
    // A phone sign-in has no email: the nickname names it.
    let read = reading(json!({"user_id": "u1", "region": "cn", "nickname": "Jun", "email": ""}));
    let account = read.account.unwrap();
    assert_eq!(account["label"], "Jun");
    assert!(account["plan"].is_null());
}

#[test]
fn a_kimi_code_sign_in_sends_its_device() {
    let expires = chrono::Utc::now().timestamp() + 3600;
    let host = FakeHost::new("/u")
        .file(
            "/u/.kimi-code/credentials/kimi-code.json",
            &json!({"access_token": "tok", "expires_at": expires}).to_string(),
        )
        .file("/u/.kimi-code/device_id", " dev-1\n")
        .answer(KIMI_USAGES, Ok(kimi_usages()));
    kimi(&host).unwrap();
    assert_eq!(host.asked.borrow()[1].1.as_deref(), Some("Bearer tok"));
    assert!(host.headers.borrow()[1].contains(&("X-Msh-Device-Id".into(), "dev-1".into())));
}

#[test]
fn glm_through_opencode_uses_the_configured_address() {
    let host = FakeHost::new("/u")
        .env("XDG_CONFIG_HOME", " /cfg ")
        .file(
            "/cfg/opencode/opencode.jsonc",
            r#"{ // OpenCode allows comments
                provider: {"zai-coding-plan": {options: {baseURL: "https://open.bigmodel.cn/api/coding/paas/v4"}}} }"#,
        )
        .file(
            "/u/.local/share/opencode/auth.json",
            r#"{"zai-coding-plan": {"type": "api", "key": "gk"}}"#,
        )
        .answer(GLM_CHINA, Ok(glm_limits()));
    let reading = glm(&host).unwrap();
    assert_eq!(reading.quotas.len(), 2);
    let tokens = &reading.quotas[0];
    assert_eq!(tokens["usedPercent"], 40.0);
    assert_eq!(tokens["windowDurationSeconds"], 18000.0);
    assert_eq!(tokens["allModels"], true);
    let calls = &reading.quotas[1];
    assert_eq!(calls["usedPercent"], 12.0);
    assert_eq!(calls["allModels"], false);
    assert!(calls["windowDurationSeconds"].is_null());
    assert_eq!(calls["nativeUsage"]["capacity"], 100.0);
    assert_eq!(reading.services[0]["client"], "OpenCode");
    assert_eq!(reading.services[0]["region"], "china");
    assert_eq!(reading.account.unwrap()["plan"], "Pro");
}

#[test]
fn claude_codes_plan_keys_come_from_both_settings_files_in_its_config_dir() {
    let home = "/u";
    let config = format!("{home}/claude-work");
    let glm_route = r#"{"env":{"ANTHROPIC_BASE_URL":"https://open.bigmodel.cn/api/anthropic","ANTHROPIC_AUTH_TOKEN":"gk"}}"#;
    let kimi_route = r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.kimi.com/coding/","ANTHROPIC_API_KEY":"sk-local"}}"#;
    let host = FakeHost::new(home)
        .env("CLAUDE_CONFIG_DIR", &config)
        .file(&format!("{config}/settings.json"), glm_route)
        .file(&format!("{config}/settings.local.json"), kimi_route)
        .file("/u/.claude/settings.json", r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.kimi.com/coding","ANTHROPIC_AUTH_TOKEN":"sk-default"}}"#)
        .answer(GLM_CHINA, Ok(glm_limits()))
        .answer(KIMI_USAGES, Ok(kimi_usages()));
    let reading = glm(&host).unwrap();
    assert_eq!(reading.quotas.len(), 2);
    assert_eq!(reading.services[0]["client"], "Claude");
    assert_eq!(
        host.asked.borrow()[0],
        (GLM_CHINA.into(), Some("Bearer gk".into()))
    );

    let reading = kimi(&host).unwrap();
    assert_eq!(reading.services[0]["client"], "Claude");
    let asked = host.asked.borrow();
    assert!(
        asked
            .iter()
            .any(|(url, key)| url == KIMI_USAGES && key.as_deref() == Some("Bearer sk-local")),
        "settings.local.json's key is read"
    );
    assert!(
        asked
            .iter()
            .all(|(_, key)| key.as_deref() != Some("Bearer sk-default")),
        "~/.claude is not read while CLAUDE_CONFIG_DIR names another folder"
    );
}

#[test]
fn claude_codes_local_settings_are_read_in_the_default_folder() {
    let host = FakeHost::new("/u")
        .file(
            "/u/.claude/settings.local.json",
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.z.ai/api/anthropic","ANTHROPIC_AUTH_TOKEN":"zk"}}"#,
        )
        .answer(GLM_GLOBAL, Ok(glm_limits()));
    let reading = glm(&host).unwrap();
    assert_eq!(reading.quotas.len(), 2);
    assert_eq!(reading.services[0]["client"], "Claude");
    assert_eq!(reading.services[0]["region"], "international");
}

#[test]
fn an_unreadable_opencode_config_hides_its_keys() {
    let host = FakeHost::new("/u")
        .file("/u/.config/opencode/opencode.json", "not json")
        .file(
            "/u/.local/share/opencode/auth.json",
            r#"{"opencode-go": {"type": "api", "key": "ok"}}"#,
        );
    let reading = opencode_go(&host).unwrap();
    assert!(reading.notice.is_some());
    assert!(host.asked.borrow().is_empty());
}

#[test]
fn pi_keys_may_name_an_environment_variable() {
    let host = FakeHost::new("/u")
        .env("PI_CODING_AGENT_DIR", "/pi")
        .env("ZKEY", "secret")
        .file(
            "/pi/auth.json",
            r#"{"zai": {"type": "api_key", "key": "ZKEY"}}"#,
        )
        .answer(GLM_GLOBAL, Ok(glm_limits()));
    let reading = glm(&host).unwrap();
    assert_eq!(host.asked.borrow()[0].1.as_deref(), Some("Bearer secret"));
    assert_eq!(reading.services[0]["region"], "international");
}

#[test]
fn a_saved_glm_team_key_asks_for_the_team_quota() {
    let mut host = FakeHost::new("/u").answer(&format!("{GLM_GLOBAL}?type=2"), Ok(glm_limits()));
    host.saved = Ok(Some(
        json!({"apiKey": "tk", "organization": "org", "project": "proj"}),
    ));
    let reading = glm(&host).unwrap();
    assert_eq!(reading.quotas.len(), 2);
    assert_eq!(reading.services[0]["client"], "Agent HUD");
    let headers = host.headers.borrow();
    assert!(headers[0].contains(&("Bigmodel-Organization".into(), "org".into())));
    assert!(headers[0].contains(&("Bigmodel-Project".into(), "proj".into())));
}

#[test]
fn opencode_go_reads_its_limits() {
    let host = FakeHost::new("/u").env("OPENCODE_GO_API_KEY", "ok").answer(
        GO_USAGE,
        Ok(json!({"usage": {
            "rolling": {"percent": 12, "resetsAt": "2026-10-22T12:00:00Z"},
            "weekly": {"percent": "50"},
            "monthly": null,
        }})),
    );
    let reading = opencode_go(&host).unwrap();
    assert_eq!(reading.quotas.len(), 2);
    assert_eq!(reading.quotas[0]["name"], "5-hour limit");
    assert_eq!(reading.quotas[0]["windowDurationSeconds"], 18000.0);
    assert_eq!(reading.quotas[1]["usedPercent"], 50.0);
    assert_eq!(reading.services[0]["provider"], "OpenCode Go");
    assert_eq!(reading.services[0]["region"], "international");
}

#[test]
fn nothing_configured_or_only_expired_keys_leave_a_notice() {
    let expired = format!("h.{}.s", URL_SAFE_NO_PAD.encode(r#"{"exp": 1}"#));
    for host in [
        FakeHost::new("/u"),
        FakeHost::new("/u").env("OPENCODE_GO_API_KEY", &expired),
    ] {
        let reading = opencode_go(&host).unwrap();
        assert_eq!(
            reading.notice.as_deref(),
            Some(
                "No usable coding-plan credential was found in this client's existing configuration"
            )
        );
        assert!(host.asked.borrow().is_empty());
    }
}

#[test]
fn one_unreadable_plan_is_a_notice_when_another_was_read() {
    let alone = FakeHost::new("/u").env("OPENCODE_GO_API_KEY", "a");
    // No answer at all passes by itself.
    assert!(matches!(
        opencode_go(&alone).err(),
        Some(Unread::Transient { reason, .. }) if reason == "Coding plan quota could not be reached"
    ));
    let refused = FakeHost::new("/u").env("OPENCODE_GO_API_KEY", "a").answer(
        "https://opencode.ai/zen/go/v1/usage",
        Err(HttpFailure::Status(401)),
    );
    assert_eq!(
        opencode_go(&refused).err().unwrap(),
        Unread::Failed("Existing coding-plan credentials could not read a verifiable quota; check the source client's login or API key".into())
    );
    let two = FakeHost::new("/u")
        .env("ZAI_API_KEY", "global")
        .env("ZAI_CODING_CN_API_KEY", "china")
        .answer(GLM_CHINA, Ok(glm_limits()));
    let reading = glm(&two).unwrap();
    assert_eq!(reading.quotas.len(), 2);
    assert_eq!(
        reading.notice.as_deref(),
        Some("1 configured coding-plan account(s) could not be read")
    );
    assert_eq!(reading.services.len(), 2);
}

#[test]
fn a_stopped_reading_asks_no_further() {
    let mut host = FakeHost::new("/u").env("OPENCODE_GO_API_KEY", "a");
    host.cancelled = true;
    assert_eq!(
        opencode_go(&host).err().unwrap(),
        "Account reading was stopped"
    );
    assert!(host.asked.borrow().is_empty());
}

#[test]
fn older_kimi_answers_count_units() {
    let entry = Credential {
        provider: "kimi",
        base: "https://api.kimi.com/coding/v1".into(),
        key: "k".into(),
        organization: None,
        project: None,
        clients: HashSet::new(),
        device: None,
    };
    let root = json!({
        "usage": {"limit": 100, "used": 25},
        "limits": [{"window": {"duration": 300, "timeUnit": "TIME_UNIT_MINUTE"},
                    "detail": {"limit": 50, "remaining": 40}}],
    });
    let (windows, _) = parse::parse(&root, &entry, "id").unwrap();
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0]["id"], "id:weekly");
    assert_eq!(windows[0]["usedPercent"], 25.0);
    assert_eq!(windows[1]["id"], "id:limit:TIME_UNIT_MINUTE:300.0");
    assert_eq!(windows[1]["usedPercent"], 20.0);
    assert_eq!(windows[1]["nativeUsage"]["used"], 10.0);

    let odd = json!({"limits": [{"window": {"duration": 1, "timeUnit": "TIME_UNIT_YEAR"}}]});
    assert!(parse::parse(&odd, &entry, "id").is_err());
}

#[test]
fn a_plans_rows_count_the_calls_through_its_own_provider_ids() {
    let entry = |provider, base: &str, clients: &[&str]| Credential {
        provider,
        base: base.into(),
        key: "k".into(),
        organization: None,
        project: None,
        clients: clients.iter().map(|client| client.to_string()).collect(),
        device: None,
    };
    let root = json!({"usage": {"limit": 100, "used": 25}});
    let kimi = entry("kimi", "https://api.kimi.com/coding/v1", &["Kimi", "Pi"]);
    let (windows, _) = parse::parse(&root, &kimi, "id").unwrap();
    assert_eq!(
        windows[0]["consumerRoutes"],
        json!(["kimi-code", "kimi-for-coding", "kimi-coding"])
    );
    assert!(windows[0]["consumerSource"].is_null());
    assert!(windows[0]["modelFamilies"].is_null());

    // Claude Code sending the key to the plan: its calls to Kimi's models count.
    let claude = entry("kimi", "https://api.kimi.com/coding/v1", &["Claude"]);
    let (windows, _) = parse::parse(&root, &claude, "id").unwrap();
    assert_eq!(windows[0]["consumerSource"], "claude");
    assert_eq!(windows[0]["modelFamilies"], json!(["kimi"]));

    // Pi's `zai` is the plan, OpenCode's is pay-as-you-go; each region has its own ids.
    let mut glm = entry(
        "glm",
        "https://open.bigmodel.cn/api/coding/paas/v4",
        &["OpenCode"],
    );
    let mut row = json!({"allModels": true});
    parse::link(&mut row, &glm);
    assert_eq!(
        row["consumerRoutes"],
        json!(["zhipuai-coding-plan", "pi#zai-coding-cn"])
    );
    glm.base = "https://api.z.ai/api/coding/paas/v4".into();
    parse::link(&mut row, &glm);
    assert_eq!(row["consumerRoutes"], json!(["zai-coding-plan", "pi#zai"]));
}

#[test]
fn expired_kimi_oauth_token_returns_transient_and_glm_without_plan_records_free_account() {
    let host = FakeHost::new("/u").file(
        "/u/.kimi-code/credentials/kimi-code.json",
        &json!({"access_token": "tok", "refresh_token": "ref", "expires_at": 1}).to_string(),
    );
    assert!(matches!(
        kimi(&host).err(),
        Some(Unread::Transient { reason, .. })
            if reason == "Kimi Code renews its sign-in the next time it runs; the quota updates then"
    ));

    let no_plan = FakeHost::new("/u")
        .env("ZAI_CODING_CN_API_KEY", "china-key")
        .answer(
            GLM_CHINA,
            Ok(json!({"code": 500, "msg": "当前用户不存在coding plan", "success": false})),
        );
    let reading = glm(&no_plan).unwrap();
    assert!(reading.quotas.is_empty());
    assert_eq!(reading.account.unwrap()["plan"], "Free");
    assert_eq!(
        reading.notice.as_deref(),
        Some("No active Coding Plan subscription on this account")
    );
}
