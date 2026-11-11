use super::*;
use crate::account::fake::FakeHost;
use crate::account::legacy_account;
use crate::codex::limits::IDENTITIES;

fn window(used: f64, minutes: u64) -> Value {
    json!({"usedPercent": used, "windowDurationMins": minutes, "resetsAt": 1_792_000_000})
}

#[test]
fn a_present_bucket_map_is_authoritative_even_when_empty() {
    let host = FakeHost::new("/h");
    let legacy = json!({"limitId": "codex", "primary": window(10.0, 300), "planType": "plus"});
    let empty = json!({"accountId": "ws", "rateLimitsByLimitId": {}, "rateLimits": legacy});
    let account = json!({"email": "a@example.com", "planType": "plus"});
    let reading = parse(&host, &empty, &account, "/h", "", true).unwrap();
    assert!(reading.quotas.is_empty(), "the empty map leaves no window");
    let absent = json!({"accountId": "ws", "rateLimits": legacy});
    let reading = parse(&host, &absent, &account, "/h", "", true).unwrap();
    assert_eq!(
        reading.quotas.len(),
        1,
        "without the map the legacy bucket counts"
    );
}

#[test]
fn the_plan_is_the_first_buckets_then_the_accounts() {
    let host = FakeHost::new("/h");
    let limits = json!({"accountId": "ws", "rateLimitsByLimitId": {
        "gpt-reserve": {"primary": window(1.0, 300), "planType": "team"},
        "codex": {"primary": window(10.0, 300), "secondary": window(20.0, 10080), "planType": "pro"}
    }});
    let account = json!({"email": "a@example.com", "planType": "plus"});
    let reading = parse(&host, &limits, &account, "/h", "", true).unwrap();
    assert_eq!(
        reading.account.unwrap()["plan"],
        "pro",
        "the codex bucket sorts first"
    );
    let unnamed = json!({"accountId": "ws", "rateLimitsByLimitId": {"codex": {"primary": window(10.0, 300)}}});
    let reading = parse(&host, &unnamed, &account, "/h", "", true).unwrap();
    assert_eq!(reading.account.unwrap()["plan"], "plus");
}

fn installed(limits: Value, account: Value) -> FakeHost {
    let mut host = FakeHost::new("/u");
    host.executables.insert("codex".into(), "/bin/codex".into());
    host.engine_replies
        .insert(1, json!({"id": 1, "result": {}}));
    host.engine_replies
        .insert(2, json!({"id": 2, "result": limits}));
    host.engine_replies
        .insert(3, json!({"id": 3, "result": {"account": account}}));
    host
}

fn limits() -> Value {
    json!({"accountId": "ws", "rateLimitsByLimitId": {
        "codex": {"primary": window(10.0, 300), "secondary": window(40.0, 10080), "planType": "pro"}
    }})
}

fn additional(host: &mut FakeHost, home: &Path, limits: Value, account: Value) {
    host.account_homes
        .entry("codex".into())
        .or_default()
        .push(home.to_path_buf());
    host.engine_replies_by_home.insert(
        home.to_path_buf(),
        installed(limits, account).engine_replies,
    );
}

#[test]
fn each_account_home_uses_its_own_engine_and_keeps_its_windows() {
    let mut host = installed(limits(), json!({"email": "personal@example.com"}))
        .env("CODEX_HOME", "/accounts/personal");
    let work = host.home().join("codex-work");
    let mut work_limits = limits();
    work_limits["accountId"] = json!("work");
    work_limits["rateLimitsByLimitId"]["codex"]["primary"]["usedPercent"] = json!(75);
    additional(
        &mut host,
        &work,
        work_limits,
        json!({"email": "work@example.com"}),
    );

    let reading = read(&host).unwrap();
    let started = host.started.borrow();
    assert_eq!(started.len(), 2);
    assert_eq!(
        started[0].3,
        [("CODEX_HOME".to_owned(), "/accounts/personal".to_owned())]
    );
    assert_eq!(
        started[1].3,
        [("CODEX_HOME".to_owned(), work.to_string_lossy().into_owned())]
    );
    assert_eq!(reading.account.unwrap()["label"], "personal@example.com");
    assert_eq!(reading.additional_accounts.len(), 1);
    assert_eq!(reading.additional_accounts[0]["label"], "work@example.com");
    assert_eq!(reading.quotas.len(), 4);
    assert_eq!(reading.quotas[0]["usedPercent"], 10.0);
    assert_eq!(reading.quotas[2]["usedPercent"], 75.0);
    assert_ne!(
        reading.quotas[0]["accountId"],
        reading.quotas[2]["accountId"]
    );
    assert!(reading.notice.is_none());
}

#[test]
fn repeated_homes_and_symlink_aliases_are_read_once() {
    let mut host = installed(limits(), json!({"email": "a@example.com"}));
    let default = host.home().join(".codex");
    let alias = host.home().join("codex-alias");
    host.canonical_paths.insert(alias.clone(), default.clone());
    host.account_homes
        .insert("codex".into(), vec![default, alias]);

    let reading = read(&host).unwrap();
    assert_eq!(host.started.borrow().len(), 1);
    assert_eq!(reading.quotas.len(), 2);
}

#[test]
fn the_same_subscription_in_different_homes_is_not_duplicated() {
    let mut host = installed(limits(), json!({"email": "A@example.com"}));
    let work = host.home().join("codex-work");
    additional(
        &mut host,
        &work,
        limits(),
        json!({"email": "a@example.com"}),
    );

    let reading = read(&host).unwrap();
    assert_eq!(host.started.borrow().len(), 2);
    assert_eq!(reading.quotas.len(), 2);
    assert!(reading.additional_accounts.is_empty());
}

#[test]
fn different_workspaces_for_the_same_email_keep_separate_subscriptions() {
    let mut host = installed(limits(), json!({"email": "a@example.com"}));
    let work = host.home().join("codex-work");
    let mut work_limits = limits();
    work_limits["accountId"] = json!("work");
    additional(
        &mut host,
        &work,
        work_limits,
        json!({"email": "a@example.com"}),
    );

    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 4);
    assert_eq!(reading.additional_accounts.len(), 1);
    assert_ne!(
        reading.account.unwrap()["id"],
        reading.additional_accounts[0]["id"]
    );
}

#[test]
fn one_failed_home_does_not_hide_another_account_or_its_error() {
    let mut host = installed(limits(), json!({"email": "a@example.com"}));
    host.engine_replies
        .insert(2, json!({"id": 2, "error": {"message": "Sign-in expired"}}));
    let work = host.home().join("codex-work");
    additional(
        &mut host,
        &work,
        limits(),
        json!({"email": "work@example.com"}),
    );
    let host = pi_signed_in(host);

    let reading = read(&host).unwrap();
    assert_eq!(reading.account.unwrap()["label"], "work@example.com");
    assert_eq!(reading.quotas.len(), 3);
    assert_eq!(reading.additional_accounts.len(), 1);
    assert_eq!(host.asked.borrow().len(), 1);
    let notice = reading.notice.unwrap();
    assert!(notice.contains("Sign-in expired"));
    assert!(notice.contains(&*host.home().join(".codex").to_string_lossy()));
}

#[test]
fn all_failed_homes_return_an_error_with_each_home() {
    let mut host = installed(limits(), Value::Null);
    host.engine_replies
        .insert(2, json!({"id": 2, "error": {"message": "Sign-in expired"}}));
    let work = host.home().join("codex-work");
    host.account_homes
        .insert("codex".into(), vec![work.clone()]);

    let error = read(&host).err().unwrap().reason().to_owned();
    assert_eq!(host.started.borrow().len(), 2);
    assert!(error.contains(&*host.home().join(".codex").to_string_lossy()));
    assert!(error.contains(&*work.to_string_lossy()));
    assert_eq!(error.matches("Sign-in expired").count(), 2);
}

#[test]
fn the_app_server_reports_limits_and_the_account() {
    let host = installed(
        limits(),
        json!({"email": "A@example.com", "planType": "pro"}),
    );
    let reading = read(&host).unwrap();
    let started = host.started.borrow();
    assert_eq!(started[0].0, std::path::PathBuf::from("/bin/codex"));
    assert_eq!(started[0].1, ["app-server", "--listen", "stdio://"]);
    assert_eq!(started[0].2, "codex");
    let home = host.home().join(".codex").to_string_lossy().into_owned();
    assert_eq!(started[0].3, [("CODEX_HOME".to_owned(), home.clone())]);
    let methods: Vec<_> = host
        .engine_sent
        .borrow()
        .iter()
        .map(|m| m["method"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        methods,
        [
            "initialize",
            "initialized",
            "account/rateLimits/read",
            "account/read",
            "hooks/list"
        ]
    );
    let identity = legacy_account("Codex", "a@example.com", "ws").unwrap();
    let account = reading.account.unwrap();
    assert_eq!(account["id"], identity.as_str());
    assert_eq!(account["label"], "A@example.com");
    assert_eq!(account["plan"], "pro");
    assert_eq!(reading.quotas.len(), 2);
    assert_eq!(reading.quotas[0]["consumerSource"], "codex");
    assert_eq!(reading.quotas[1]["usedPercent"], 40.0);
    let remembered: Value = serde_json::from_slice(&host.own.borrow()[IDENTITIES]).unwrap();
    assert_eq!(remembered[format!("{home}/ws")], "A@example.com");
}

#[test]
fn a_remembered_email_names_a_workspace_reported_alone() {
    let host = installed(limits(), Value::Null);
    let home = host.home().join(".codex").to_string_lossy().into_owned();
    host.own.borrow_mut().insert(
        IDENTITIES.into(),
        serde_json::to_vec(&json!({format!("{home}/ws"): "a@example.com"})).unwrap(),
    );
    let account = read(&host).unwrap().account.unwrap();
    assert_eq!(account["label"], "a@example.com");
}

#[test]
fn a_home_without_a_chatgpt_plan_says_why_instead_of_the_engine_error() {
    let refused = json!({"id": 2, "error": {"message": "chatgpt authentication required to read rate limits"}});
    let cases = [
        (
            json!({"account": {"type": "apiKey"}, "requiresOpenaiAuth": true}),
            "Codex is signed in with an API key, which has no ChatGPT plan limits",
        ),
        (
            json!({"account": {"type": "amazonBedrock"}, "requiresOpenaiAuth": false}),
            "Codex runs on Amazon Bedrock, which has no ChatGPT plan limits",
        ),
        (
            json!({"account": null, "requiresOpenaiAuth": false}),
            "Codex uses a custom model provider, which has no ChatGPT plan limits",
        ),
    ];
    for (answer, notice) in cases {
        let mut host = installed(limits(), Value::Null);
        host.engine_replies.insert(2, refused.clone());
        host.engine_replies
            .insert(3, json!({"id": 3, "result": answer}));
        let reading = read(&host).unwrap();
        assert!(reading.quotas.is_empty(), "{notice}");
        assert!(reading.account.is_none(), "{notice}");
        assert_eq!(reading.notice.as_deref(), Some(notice));
    }
    // No sign-in at all is Codex's own state, not a plan without limits:
    // a failed read, so the last windows stay.
    let mut host = installed(limits(), Value::Null);
    host.engine_replies.insert(2, refused.clone());
    host.engine_replies.insert(
        3,
        json!({"id": 3, "result": {"account": null, "requiresOpenaiAuth": true}}),
    );
    assert_eq!(
        read(&host).err(),
        Some(Unread::SignedOut(
            "Codex is not signed in. Run codex login with ChatGPT to read its quota".into()
        ))
    );

    // A ChatGPT sign-in whose limits fail still shows what Codex said.
    let mut host = installed(limits(), Value::Null);
    host.engine_replies.insert(2, refused);
    host.engine_replies.insert(
            3,
            json!({"id": 3, "result": {"account": {"type": "chatgpt", "email": "a@example.com", "planType": "pro"}, "requiresOpenaiAuth": true}}),
        );
    assert_eq!(
        read(&host).err().unwrap(),
        "Codex: chatgpt authentication required to read rate limits"
    );
}

#[test]
fn the_reading_says_which_agent_hud_hooks_codex_skips() {
    let mut host = installed(limits(), Value::Null);
    let hud = "'/Applications/Agent HUD.app/Contents/MacOS/Agent HUD'";
    host.engine_replies.insert(
        4,
        json!({"id": 4, "result": {"data": [{"cwd": "/u", "hooks": [
            {"eventName": "permissionRequest", "command": format!("{hud} --permission-hook codex"), "trustStatus": "modified", "enabled": true},
            {"eventName": "stop", "command": format!("{hud} --remote-hook codex"), "trustStatus": "trusted", "enabled": true}
        ]}]}}),
    );
    assert_eq!(read(&host).unwrap().unreviewed_hooks, ["permissionRequest"]);
    let sent = host.engine_sent.borrow();
    let list = sent.iter().find(|m| m["method"] == "hooks/list").unwrap();
    assert_eq!(list["params"]["cwds"], json!(["/u"]));
    drop(sent);
    // An app-server without hooks/list still reports limits, with none waiting.
    host.engine_replies
        .insert(4, json!({"id": 4, "error": {"message": "unknown method"}}));
    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 2);
    assert!(reading.unreviewed_hooks.is_empty());
}

#[test]
fn an_engine_without_account_read_still_reports_limits() {
    let mut host = installed(limits(), Value::Null);
    host.engine_replies
        .insert(3, json!({"id": 3, "error": {"message": "unknown method"}}));
    assert_eq!(read(&host).unwrap().quotas.len(), 2);
}

#[test]
fn no_limits_says_whether_codex_exited_or_timed_out() {
    let mut host = installed(limits(), Value::Null);
    host.engine_replies.remove(&2);
    host.engine_replies.remove(&3);
    assert_eq!(
        read(&host).err().unwrap(),
        "Codex exited before reporting limits; check local sign-in"
    );
    host.engine_ends = false;
    assert_eq!(read(&host).err().unwrap(), "Codex quota query timed out");
    let missing = FakeHost::new("/u");
    assert_eq!(
        read(&missing).err().unwrap(),
        "Install and sign in to read quota"
    );
}

fn pi_signed_in(host: FakeHost) -> FakeHost {
    let expires = (chrono::Utc::now().timestamp() + 3600) * 1000;
    host.file(
            "/u/.pi/agent/auth.json",
            &json!({"openai-codex": {"type": "oauth", "access": "tok", "accountId": "acc", "expires": expires}})
                .to_string(),
        )
        .answer(
            "https://chatgpt.com/backend-api/wham/usage",
            Ok(json!({"account_id": "acc", "email": "p@example.com", "plan_type": "plus",
                      "rate_limit": {"primary_window": {"used_percent": 30, "limit_window_seconds": 18000, "reset_at": 1_792_000_000}}})),
        )
}

#[test]
fn pi_reads_the_same_plan_without_codex() {
    let host = pi_signed_in(FakeHost::new("/u"));
    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 1);
    assert_eq!(reading.quotas[0]["usedPercent"], 30.0);
    assert!(reading.quotas[0]["consumerSource"].is_null());
    assert_eq!(reading.quotas[0]["consumerRoutes"], json!(["openai-codex"]));
    assert_eq!(
        reading.account.unwrap()["id"],
        legacy_account("Codex", "p@example.com", "acc")
            .unwrap()
            .as_str()
    );
    assert_eq!(host.asked.borrow()[0].1.as_deref(), Some("Bearer tok"));
    assert!(host.headers.borrow()[0].contains(&("ChatGPT-Account-Id".into(), "acc".into())));
    assert!(reading.notice.is_none());
}

#[test]
fn a_configured_codex_home_needs_the_cli_even_when_pi_succeeds() {
    let mut host = pi_signed_in(FakeHost::new("/u"));
    let work = host.home().join("codex-work");
    host.account_homes.insert("codex".into(), vec![work]);

    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 1);
    assert_eq!(
        reading.notice.as_deref(),
        Some("Install and sign in to read quota")
    );
}

#[test]
fn pis_second_account_joins_codexs() {
    let host = pi_signed_in(installed(limits(), json!({"email": "a@example.com"})));
    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 3);
    assert_eq!(reading.additional_accounts.len(), 1);
    assert_eq!(reading.additional_accounts[0]["label"], "p@example.com");
}

#[test]
fn pi_does_not_duplicate_an_account_in_an_additional_home() {
    let mut host = installed(limits(), json!({"email": "a@example.com"}));
    let work = host.home().join("codex-work");
    let mut work_limits = limits();
    work_limits["accountId"] = json!("acc");
    additional(
        &mut host,
        &work,
        work_limits,
        json!({"email": "p@example.com"}),
    );
    let host = pi_signed_in(host);

    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 4);
    assert_eq!(reading.additional_accounts.len(), 1);
    assert_eq!(host.asked.borrow().len(), 1);
    assert!(reading
        .quotas
        .iter()
        .all(|quota| quota["consumerSource"] == "codex"));
    // Pi's calls spend only the windows of the account it is signed in to.
    let shared = reading
        .quotas
        .iter()
        .filter(|quota| quota["consumerRoutes"] == json!(["openai-codex"]))
        .count();
    assert_eq!(shared, 2);
}

#[test]
fn pi_success_preserves_a_native_account_error() {
    let mut host = installed(limits(), Value::Null);
    host.engine_replies
        .insert(2, json!({"id": 2, "error": {"message": "Sign-in expired"}}));
    let reading = read(&pi_signed_in(host)).unwrap();
    assert_eq!(reading.quotas.len(), 1);
    assert_eq!(reading.notice.as_deref(), Some("Codex: Sign-in expired"));
}

#[test]
fn a_pi_error_preserves_an_additional_homes_error() {
    let mut host = installed(limits(), json!({"email": "a@example.com"})).file(
        "/u/.pi/agent/auth.json",
        r#"{"openai-codex": {"type": "oauth", "access": "tok", "accountId": "acc", "expires": 1}}"#,
    );
    let work = host.home().join("codex-work");
    additional(&mut host, &work, limits(), Value::Null);
    host.engine_replies_by_home
        .get_mut(&work)
        .unwrap()
        .insert(2, json!({"id": 2, "error": {"message": "Sign-in expired"}}));

    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 2);
    let notice = reading.notice.unwrap();
    assert!(notice.contains("Sign-in expired"));
    assert!(notice.contains("Sign in to ChatGPT again in Pi"));
}
