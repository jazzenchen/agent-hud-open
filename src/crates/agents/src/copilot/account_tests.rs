use super::*;
use crate::account::fake::FakeHost;
use crate::account::Answer;
use crate::copilot::tokens::STORED_TOKEN;
use base64::Engine;
use std::sync::atomic::Ordering;

const PROFILE: &str = "https://api.github.com/user";

fn token(host: &dyn Host) -> Option<String> {
    tokens(host).next()
}

fn consenting(host: FakeHost) -> FakeHost {
    host.consent.store(true, Ordering::Release);
    host
}

#[test]
fn the_token_comes_from_env_then_the_store_then_hosts_yml() {
    let yml = "github.com:\n    user: me\n    oauth_token: from-file\n";
    let file = FakeHost::new("/home/u").file("/home/u/.config/gh/hosts.yml", yml);
    assert_eq!(token(&file).as_deref(), Some("from-file"));

    let mut stored = FakeHost::new("/home/u").file("/home/u/.config/gh/hosts.yml", yml);
    let encoded = base64::engine::general_purpose::STANDARD.encode("from-store");
    stored.secrets.insert(
        STORED_TOKEN.into(),
        Answer::Found(format!("go-keyring-base64:{encoded}")),
    );
    assert_eq!(token(&stored).as_deref(), Some("from-store"));

    let env = stored.env("GITHUB_TOKEN", " 'from-env' ");
    assert_eq!(token(&env).as_deref(), Some("from-env"));
}

#[test]
fn an_os_without_a_readable_store_falls_back_to_hosts_yml() {
    let mut host = FakeHost::new("C:\\Users\\u")
        .env("GH_CONFIG_DIR", "/cfg")
        .file("/cfg/hosts.yml", "github.com:\n  oauth_token: t\n");
    host.secrets
        .insert(STORED_TOKEN.into(), Answer::Unsupported);
    assert_eq!(token(&host).as_deref(), Some("t"));
}

#[test]
fn windows_gh_config_uses_appdata_after_explicit_overrides() {
    let yml = |token: &str| format!("github.com:\n  oauth_token: {token}\n");
    let mut host = FakeHost::new("/home/u")
        .env("APPDATA", "/roaming")
        .env("GH_CONFIG_DIR", "")
        .env("XDG_CONFIG_HOME", "")
        .file("/roaming/GitHub CLI/hosts.yml", &yml("roaming"))
        .file("/xdg/gh/hosts.yml", &yml("xdg"))
        .file("/custom/hosts.yml", &yml("custom"))
        .file("/home/u/.config/gh/hosts.yml", &yml("home"));
    host.os = crate::Os::Windows;
    assert_eq!(token(&host).as_deref(), Some("roaming"));
    host.env.insert("XDG_CONFIG_HOME".into(), "/xdg".into());
    assert_eq!(token(&host).as_deref(), Some("xdg"));
    host.env.insert("GH_CONFIG_DIR".into(), "/custom".into());
    assert_eq!(token(&host).as_deref(), Some("custom"));
    host.env.remove("GH_CONFIG_DIR");
    host.env.remove("XDG_CONFIG_HOME");
    host.os = crate::Os::MacOs;
    assert_eq!(token(&host).as_deref(), Some("home"));
}

#[test]
fn windows_selected_config_token_precedes_the_global_keyring_slot() {
    let mut host = FakeHost::new("/home/u")
        .env("GH_CONFIG_DIR", "/selected")
        .file(
            "/selected/hosts.yml",
            "github.com:\n  oauth_token: selected-account\n",
        );
    host.os = crate::Os::Windows;
    host.secrets
        .insert(STORED_TOKEN.into(), Answer::Found("global-account".into()));
    assert_eq!(token(&host).as_deref(), Some("selected-account"));
    host.env
        .insert("GH_TOKEN".into(), "environment-account".into());
    assert_eq!(token(&host).as_deref(), Some("environment-account"));
    host.env.remove("GH_TOKEN");
    host.files.clear();
    assert_eq!(token(&host).as_deref(), Some("global-account"));
}

#[test]
fn a_hosts_file_over_a_megabyte_is_not_read() {
    let big = format!("github.com:\n  oauth_token: t\n{}", "#".repeat(1024 * 1024));
    let host = FakeHost::new("/h").file("/h/.config/gh/hosts.yml", &big);
    assert_eq!(token(&host), None);
}

#[test]
fn a_reading_names_the_account_and_its_windows() {
    let body = json!({
        "copilot_plan": "individual",
        "quota_reset_date_utc": "2026-11-01T00:00:00Z",
        "quota_snapshots": {
            "chat": {"unlimited": true},
            "premium_interactions": {"entitlement": 300, "remaining": 75, "percent_remaining": 25},
        }
    });
    let host = consenting(
        FakeHost::new("/h")
            .env("GH_TOKEN", "abc")
            .answer(USER, Ok(body))
            .answer(PROFILE, Ok(json!({"login": "octo", "id": 7}))),
    );
    let reading = read(&host).unwrap();
    let id = legacy_account("Copilot", "github.com:7", "").unwrap();
    assert_eq!(reading.account.as_ref().unwrap()["id"], id.as_str());
    assert_eq!(reading.quotas.len(), 1);
    let row = &reading.quotas[0];
    assert_eq!(row["id"], format!("{id}/copilot:premium_interactions"));
    assert_eq!(row["usedPercent"], 75.0);
    assert_eq!(row["status"], "warning");
    assert_eq!(row["account"], "octo");
    assert_eq!(row["resetAt"], "2026-11-01T00:00:00+00:00");
    assert_eq!(row["nativeUsage"]["used"], 225.0);
    let asked = host.asked.borrow();
    assert_eq!(asked[0], (USER.into(), Some("token abc".into())));
    assert_eq!(asked.len(), 2);
}

#[test]
fn without_a_profile_the_account_is_the_home() {
    let host = consenting(
        FakeHost::new("/h")
            .env("GH_TOKEN", "abc")
            .answer(USER, Ok(json!({}))),
    );
    let reading = read(&host).unwrap();
    let expected = format!(
        "account:{}",
        crate::account::hash(&["Copilot", "", "home:/h"])
    );
    assert_eq!(reading.account.unwrap()["id"], expected.as_str());
}

#[test]
fn failures_are_worded_for_copilot() {
    let host = consenting(FakeHost::new("/h").env("GH_TOKEN", "abc"));
    assert_eq!(read(&host).err().unwrap(), "Copilot account request failed");
    let host = consenting(
        FakeHost::new("/h")
            .env("GH_TOKEN", "abc")
            .answer(USER, Err(HttpFailure::BadHeader)),
    );
    assert_eq!(
        read(&host).err().unwrap(),
        "GitHub token cannot be used as a header"
    );
    let host = consenting(
        FakeHost::new("/h")
            .env("GH_TOKEN", "abc")
            .answer(USER, Err(HttpFailure::Status(401))),
    );
    assert_eq!(
        read(&host).err().unwrap(),
        HttpFailure::Status(401).message()
    );
    let host = consenting(FakeHost::new("/h"));
    assert_eq!(
        read(&host).err().unwrap(),
        "Sign in to GitHub CLI or Copilot before reading Copilot quota"
    );
}

fn all(host: &FakeHost) -> Vec<String> {
    tokens(host).collect()
}

/// A Copilot sign-ins file with a github.com entry and an Enterprise one.
fn sign_ins(token: &str) -> String {
    json!({
        "ghe.example.com:Iv1.enterprise": {"user": "corp", "oauth_token": "enterprise-token"},
        "github.com:Iv1.app": {"user": "octo", "oauth_token": token, "githubAppId": "Iv1.app"},
    })
    .to_string()
}

#[test]
fn copilots_own_sign_in_follows_the_github_clis() {
    let apps = "/h/.config/github-copilot/apps.json";
    let host = FakeHost::new("/h").file(apps, &sign_ins("from-copilot"));
    assert_eq!(all(&host), ["from-copilot"], "never the Enterprise token");

    let host = FakeHost::new("/h")
        .file(apps, &sign_ins("from-copilot"))
        .file(
            "/h/.config/gh/hosts.yml",
            "github.com:\n  oauth_token: from-gh\n",
        );
    assert_eq!(all(&host), ["from-gh", "from-copilot"]);

    let host = FakeHost::new("/h").file(apps, &sign_ins("same")).file(
        "/h/.config/gh/hosts.yml",
        "github.com:\n  oauth_token: same\n",
    );
    assert_eq!(all(&host), ["same"], "each token once");

    let host = FakeHost::new("/h")
        .file(apps, &sign_ins("from-copilot"))
        .env("GH_TOKEN", "from-env");
    assert_eq!(
        all(&host),
        ["from-env"],
        "the environment's is the only one"
    );

    let older = FakeHost::new("/h").file(
        "/h/.config/github-copilot/hosts.json",
        &json!({"github.com": {"oauth_token": "older"}}).to_string(),
    );
    assert_eq!(all(&older), ["older"]);
}

#[test]
fn copilots_folder_follows_xdg_and_windows_local_app_data() {
    let home = "/h";
    let xdg = format!("{home}/xdg");
    let host = FakeHost::new(home)
        .env("XDG_CONFIG_HOME", &xdg)
        .file("/h/.config/github-copilot/apps.json", &sign_ins("default"))
        .file(&format!("{xdg}/github-copilot/apps.json"), &sign_ins("xdg"));
    assert_eq!(all(&host), ["xdg"]);

    let local = format!("{home}/Local");
    let mut host = FakeHost::new(home)
        .file(
            "/h/AppData/Local/github-copilot/apps.json",
            &sign_ins("app-data"),
        )
        .file(
            &format!("{local}/github-copilot/apps.json"),
            &sign_ins("local"),
        );
    host.os = crate::Os::Windows;
    assert_eq!(all(&host), ["app-data"]);
    host.env.insert("LOCALAPPDATA".into(), local);
    assert_eq!(all(&host), ["local"]);
}

#[test]
fn a_reading_with_copilots_sign_in_waits_for_consent() {
    let host = FakeHost::new("/h").file(
        "/h/.config/github-copilot/apps.json",
        &sign_ins("from-copilot"),
    );
    assert!(read(&host).unwrap().notice.is_some());
    assert!(host.asked.borrow().is_empty());

    let host = consenting(host).answer(USER, Ok(json!({"copilot_plan": "individual"})));
    let reading = read(&host).unwrap();
    assert_eq!(reading.account.unwrap()["plan"], "individual");
    assert_eq!(
        host.asked.borrow()[0],
        (USER.into(), Some("token from-copilot".into()))
    );
}

#[test]
fn a_token_that_no_longer_signs_in_gives_way_to_the_next() {
    let host = consenting(
        FakeHost::new("/h")
            .file(
                "/h/.config/gh/hosts.yml",
                "github.com:\n  oauth_token: old\n",
            )
            .file("/h/.config/github-copilot/apps.json", &sign_ins("current"))
            .answer_for(USER, "token old", Err(HttpFailure::Status(401)))
            .answer_for(
                USER,
                "token current",
                Ok(json!({"copilot_plan": "business"})),
            )
            .answer_for(
                PROFILE,
                "token current",
                Ok(json!({"login": "octo", "id": 7})),
            ),
    );
    let reading = read(&host).unwrap();
    assert_eq!(reading.account.unwrap()["label"], "octo");
    let asked = host.asked.borrow();
    assert_eq!(asked[0].1.as_deref(), Some("token old"));
    assert_eq!(asked[1].1.as_deref(), Some("token current"));
    assert_eq!(asked[2], (PROFILE.into(), Some("token current".into())));

    let host = consenting(
        FakeHost::new("/h")
            .file(
                "/h/.config/gh/hosts.yml",
                "github.com:\n  oauth_token: old\n",
            )
            .answer(USER, Err(HttpFailure::Status(401))),
    );
    assert!(matches!(read(&host), Err(Unread::SignedOut(_))));
}

fn premium(extra: Value) -> Value {
    let mut premium = json!({"entitlement": 300, "remaining": 0, "percent_remaining": 0});
    for (key, value) in extra.as_object().unwrap() {
        premium[key] = value.clone();
    }
    json!({"copilot_plan": "individual", "quota_snapshots": {"premium_interactions": premium}})
}

#[test]
fn premium_requests_beyond_the_plan_are_priced_as_overage() {
    let body = premium(json!({"overage_count": 37, "overage_permitted": true}));
    let reading = parse(&body, "id", Some("octo"));
    assert_eq!(reading.quotas[0]["usedPercent"], 100.0);
    assert_eq!(reading.billing.len(), 1);
    let row = &reading.billing[0];
    assert_eq!(row["id"], "id/copilot:overage");
    assert_eq!(row["vendor"], "copilot");
    assert_eq!(row["accountId"], "id");
    assert_eq!(row["amount"], 1.48);
    assert_eq!(row["currency"], "USD");
    assert_eq!(row["label"], "Overage (estimated)");
    assert!(row["notice"]
        .as_str()
        .unwrap()
        .starts_with("37 premium requests"));

    // Allowed unless turned off, as the field may be missing.
    let body = premium(json!({"overage_count": "2"}));
    assert_eq!(parse(&body, "id", None).billing[0]["amount"], 0.08);
}

#[test]
fn no_overage_is_shown_without_any_or_when_it_is_off_or_billed_in_credits() {
    for extra in [
        json!({}),
        json!({"overage_count": 0, "overage_permitted": true}),
        json!({"overage_count": 12, "overage_permitted": false}),
        json!({"overage_count": 12, "token_based_billing": true}),
    ] {
        let body = premium(extra.clone());
        assert!(parse(&body, "id", None).billing.is_empty(), "{extra}");
    }
    let mut body = premium(json!({"overage_count": 12}));
    body["token_based_billing"] = json!(true);
    assert!(parse(&body, "id", None).billing.is_empty());
}

#[test]
fn the_plan_comes_from_the_access_sku_before_copilot_plan() {
    let plan_of = |sku: Option<&str>| {
        let mut body = json!({"copilot_plan": "individual"});
        if let Some(sku) = sku {
            body["access_type_sku"] = json!(sku);
        }
        parse(&body, "id", None).account.unwrap()["plan"].clone()
    };
    assert_eq!(plan_of(Some("free_limited_copilot")), "free");
    assert_eq!(plan_of(Some("monthly_subscriber_quota")), "pro");
    assert_eq!(plan_of(Some("yearly_subscriber_quota")), "pro");
    assert_eq!(plan_of(Some("plus_monthly_subscriber_quota")), "pro_plus");
    assert_eq!(plan_of(Some("plus_yearly_subscriber_quota")), "pro_plus");
    assert_eq!(plan_of(Some("copilot_for_business_seat_quota")), "business");
    assert_eq!(plan_of(Some("copilot_enterprise_seat_quota")), "enterprise");
    assert_eq!(
        plan_of(Some(" Free_Educational_Quota ")),
        "free_educational_quota",
        "an SKU not known here is kept, in lower case"
    );
    assert_eq!(plan_of(Some("")), "individual");
    assert_eq!(plan_of(None), "individual");
    assert!(parse(&json!({}), "id", None).account.unwrap()["plan"].is_null());
}

#[test]
fn limited_users_read_their_monthly_quotas() {
    let body = json!({
        "access_type_sku": "free_limited_copilot",
        "limited_user_quotas": {"chat": 40, "completions": 1000},
        "monthly_quotas": {"chat": 50, "completions": 2000},
        "limited_user_reset_date": "2026-11-01",
    });
    let reading = parse(&body, "id", None);
    assert_eq!(reading.quotas.len(), 2);
    let chat = reading
        .quotas
        .iter()
        .find(|row| row["id"] == "id/copilot:chat")
        .unwrap();
    assert_eq!(chat["usedPercent"], 20.0);
    assert_eq!(chat["resetAt"], "2026-11-01T00:00:00+00:00");
    assert_eq!(chat["allModels"], false);
}
