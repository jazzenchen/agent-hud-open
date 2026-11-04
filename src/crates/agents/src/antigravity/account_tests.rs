use super::*;
use crate::account::fake::FakeHost;
use crate::account::{account_id, legacy_account, legacy_unresolved};
use crate::Os;

fn status_body() -> Value {
    json!({"userStatus": {
        "email": "Me@Example.com",
        "userTier": {"name": "Pro"},
        "cascadeModelConfigData": {"clientModelConfigs": [
            {"label": "Gemini 3 Pro", "modelOrAlias": {"model": "gemini-3-pro"},
             "quotaInfo": {"remainingFraction": 0.25, "resetTime": "2026-10-23T00:00:00Z"}},
            {"label": "Gemini 3 Flash", "modelOrAlias": {"model": "gemini-3-flash"},
             "quotaInfo": {"remainingFraction": 0.5}},
            {"label": "Claude Sonnet", "modelOrAlias": {"model": "claude-sonnet"},
             "quotaInfo": {"remainingFraction": 1.0}},
            {"label": "Gemini Lite", "modelOrAlias": {"model": "gemini-lite"},
             "quotaInfo": {"remainingFraction": 0.0}},
        ]},
    }})
}

fn answering(body: Result<Value, String>) -> FakeHost {
    let mut host = FakeHost::new("/u");
    host.natives.insert(
        ("antigravity".into(), "ReadAccounts".into()),
        body.map(|status| json!({"accounts":[{"source":"/u","status":status}]})),
    );
    host
}

#[test]
fn families_keep_their_most_used_model() {
    let reading = read(&answering(Ok(status_body()))).unwrap();
    let identity = legacy_account("Antigravity", "me@example.com", "").unwrap();
    assert_eq!(reading.quotas.len(), 2);
    let gemini = &reading.quotas[1];
    assert_eq!(gemini["name"], "Gemini Models");
    assert_eq!(gemini["usedPercent"], 75.0);
    assert_eq!(gemini["status"], "warning");
    assert_eq!(gemini["resetAt"], "2026-10-23T00:00:00Z");
    assert_eq!(gemini["accountId"], identity.as_str());
    assert_eq!(gemini["allModels"], false);
    assert_eq!(gemini["consumerSource"], "antigravity");
    let id = account_id("antigravity", &["me@example.com", ""]);
    assert_eq!(gemini["id"], format!("{id}:legacy:gemini"));
    assert_eq!(reading.quotas[0]["modelFamilies"], json!(["claude", "gpt"]));
    // The Swift app's account for me@example.com, as its quota history keys it.
    let open = "account:53881ee06d7a6f9562ecf52163857631d9ffdf31717da2d0a7b37b430ff6d1e2";
    assert_eq!(identity, open);
    assert_eq!(
        reading.quotas[0]["legacyWindowId"],
        format!("{open}/antigravity:legacy:claude-gpt")
    );
    assert_eq!(
        gemini["legacyWindowId"],
        format!("{open}/antigravity:legacy:gemini")
    );
    let account = reading.account.unwrap();
    assert_eq!(account["plan"], "Pro");
    assert_eq!(account["identity"], "account");
}

#[test]
fn without_an_email_the_account_is_the_home() {
    let mut body = status_body();
    body["userStatus"]["email"] = json!(" ");
    let reading = read(&answering(Ok(body))).unwrap();
    let account = reading.account.unwrap();
    assert_eq!(account["identity"], "local-home");
    assert_eq!(
        account["id"],
        legacy_unresolved("Antigravity", "/u").as_str()
    );
    assert!(reading.quotas[0]["legacyWindowId"].is_null());
}

#[test]
fn windows_reads_the_account_service() {
    let mut host = answering(Ok(status_body()));
    host.os = Os::Windows;
    let reading = read(&host).unwrap();
    assert!(!reading.quotas.is_empty());
}

#[test]
fn a_service_error_or_odd_answer_is_an_error() {
    assert_eq!(
        read(&answering(Err("no service".into()))).err().unwrap(),
        "no service"
    );
    assert!(read(&answering(Ok(json!({"userStatus": {}})))).is_err());
}

#[test]
fn every_account_is_kept_and_the_same_sign_in_is_not_duplicated() {
    let first = status_body();
    let mut second = status_body();
    second["userStatus"]["email"] = json!("work@example.com");
    let mut duplicate = first.clone();
    duplicate["userStatus"]["cascadeModelConfigData"]["clientModelConfigs"][0]["quotaInfo"]
        ["remainingFraction"] = json!(0.1);
    let mut host = FakeHost::new("/u");
    host.natives.insert(
        ("antigravity".into(), "ReadAccounts".into()),
        Ok(json!({"accounts":[
            {"source":"service-1","status":first},
            {"source":"service-2","status":second},
            {"source":"service-3","status":duplicate}
        ]})),
    );
    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 4);
    assert_eq!(reading.additional_accounts.len(), 1);
    assert_eq!(reading.additional_accounts[0]["label"], "work@example.com");
    assert_eq!(reading.quotas[1]["usedPercent"], 90.0);
    assert!(reading.notice.is_none());
}

#[test]
fn anonymous_services_stay_separate_and_partial_failure_keeps_good_quota() {
    let mut anonymous = status_body();
    anonymous["userStatus"]["email"] = Value::Null;
    let mut host = FakeHost::new("/u");
    host.natives.insert(
        ("antigravity".into(), "ReadAccounts".into()),
        Ok(json!({"accounts":[
            {"source":"service-1","status":anonymous},
            {"source":"service-2","status":anonymous},
            {"source":"service-3","status":{}}
        ]})),
    );
    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 4);
    assert_eq!(reading.additional_accounts.len(), 1);
    assert_ne!(
        reading.quotas[0]["accountId"],
        reading.quotas[2]["accountId"]
    );
    assert_eq!(
        reading.notice.as_deref(),
        Some("Some Antigravity accounts could not be read.")
    );
}

#[test]
fn a_readable_native_account_does_not_launch_the_cli() {
    let mut host = FakeHost::new("/u");
    host.natives.insert(
        ("antigravity".into(), "ReadAccounts".into()),
        Ok(json!({
            "accounts":[{"source":"service-1","status":status_body()}],
            "cliExecutable":"/installed/agy"
        })),
    );
    assert!(!read(&host).unwrap().quotas.is_empty());
    assert!(host.started.borrow().is_empty());
}

#[test]
fn a_tokenless_running_cli_is_read_when_no_native_quota_is_available() {
    let mut host = FakeHost::new("/u");
    host.natives.insert(
        ("antigravity".into(), "ReadAccounts".into()),
        Ok(json!({"accounts":[],"cliExecutable":"/installed/agy"})),
    );
    host.engine_output.insert("-p".into(), vec![json!({
            "status":"SUCCESS","command":{"name":"usage","data":{"groups":[
                {"name":"Gemini Models","buckets":[{"id":"gemini-5h","window":"5h","remaining_fraction":0.25}]}
            ]}}
        })]);
    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas[0]["usedPercent"], 75.0);
    assert_eq!(reading.account.unwrap()["label"], "Antigravity CLI");
    assert_eq!(host.started.borrow().len(), 1);
}

#[test]
fn duplicate_services_prefer_the_new_cycle_over_exhausted_old_quota() {
    let mut old = status_body();
    old["userStatus"]["cascadeModelConfigData"]["clientModelConfigs"][0]["quotaInfo"] = json!({
        "remainingFraction":0.05,"resetTime":"2026-10-22T00:00:00Z"
    });
    let mut new = status_body();
    new["userStatus"]["cascadeModelConfigData"]["clientModelConfigs"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    new["userStatus"]["cascadeModelConfigData"]["clientModelConfigs"][0]["quotaInfo"] = json!({
        "remainingFraction":0.95,"resetTime":"2026-10-23T00:00:00Z"
    });
    for statuses in [[old.clone(), new.clone()], [new, old]] {
        let mut reading = Reading::empty("antigravity");
        for body in statuses {
            merge(&mut reading, parse("/u", &body, &Value::Null).unwrap());
        }
        let gemini = reading
            .quotas
            .iter()
            .find(|row| row["name"] == "Gemini Models")
            .unwrap();
        assert!((gemini["usedPercent"].as_f64().unwrap() - 5.0).abs() < 0.001);
        assert_eq!(gemini["resetAt"], "2026-10-23T00:00:00Z");
    }
}

#[test]
fn native_quota_summary_supplies_exact_windows_with_account_identity() {
    let mut host = FakeHost::new("/u");
    host.natives.insert(
            ("antigravity".into(), "ReadAccounts".into()),
            Ok(json!({
                "accounts":[{
                    "source":"service-1",
                    "status":status_body(),
                    "summary":{
                        "response":{
                            "groups":[
                                {
                                    "displayName":"Gemini Models",
                                    "buckets":[
                                        {"bucketId":"gemini-weekly","displayName":"Weekly Limit Remaining","window":"weekly","remainingFraction":0.75,"resetTime":"2026-11-02T00:00:00Z"},
                                        {"bucketId":"gemini-5h","displayName":"Five Hour Limit Remaining","window":"5h","remainingFraction":1.0,"resetTime":"2026-10-26T12:00:00Z"}
                                    ]
                                },
                                {
                                    "displayName":"Claude and GPT models",
                                    "buckets":[
                                        {"bucketId":"3p-weekly","displayName":"Weekly Limit Remaining","window":"weekly","remainingFraction":0.2,"resetTime":"2026-10-26T15:00:00Z"},
                                        {"bucketId":"3p-5h","displayName":"Five Hour Limit Remaining","window":"5h","remainingFraction":0.5,"resetTime":"2026-10-26T12:00:00Z"}
                                    ]
                                }
                            ]
                        }
                    }
                }]
            })),
        );
    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 4);
    assert_eq!(reading.quotas[0]["shortLabel"], "Gemini 7d");
    assert_eq!(reading.quotas[0]["windowDurationSeconds"], 604800);
    assert_eq!(reading.quotas[0]["usedPercent"], 25.0);
    assert_eq!(reading.quotas[1]["shortLabel"], "Gemini 5h");
    assert_eq!(reading.quotas[1]["windowDurationSeconds"], 18000);
    assert_eq!(reading.quotas[2]["shortLabel"], "3rd-party 7d");
    assert!((reading.quotas[2]["usedPercent"].as_f64().unwrap() - 80.0).abs() < 0.001);
    assert_eq!(reading.quotas[2]["modelFamilies"], json!(["claude", "gpt"]));
    assert_eq!(reading.quotas[3]["shortLabel"], "3rd-party 5h");
    assert_eq!(reading.quotas[3]["account"], "Me@Example.com");
}

fn report(email: &str, observed_at: &str, remaining: f64, reset: &str) -> Value {
    super::super::statusline::sanitize(
        &json!({"email":email,"plan_tier":"Pro",
            "quota":{"gemini-weekly":{"remaining_fraction":remaining,"reset_time":reset}}
        }),
        observed_at,
    )
    .unwrap()
}

#[test]
fn cli_reports_track_multiple_accounts_without_a_running_service() {
    let mut host = FakeHost::new("/u");
    host.os = Os::Windows;
    host.account_reports.insert(
        "antigravity".into(),
        vec![
            report(
                "Me@Example.com",
                "2026-10-23T08:00:00Z",
                0.75,
                "2026-10-30T00:00:00Z",
            ),
            report(
                "work@example.com",
                "2026-10-23T08:00:00Z",
                0.5,
                "2026-10-30T00:00:00Z",
            ),
        ],
    );
    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 2);
    assert_eq!(reading.additional_accounts.len(), 1);
    assert_ne!(
        reading.quotas[0]["accountId"],
        reading.quotas[1]["accountId"]
    );
    assert_eq!(reading.quotas[0]["observedAt"], "2026-10-23T08:00:00+00:00");
    assert!(host.started.borrow().is_empty());
}

#[test]
fn cli_summary_supersedes_native_same_account_and_keeps_other_accounts() {
    let mut other = status_body();
    other["userStatus"]["email"] = json!("other@example.com");
    let mut host = FakeHost::new("/u");
    host.natives.insert(("antigravity".into(), "ReadAccounts".into()), Ok(json!({
            "accounts":[{"source":"ide-1","status":status_body()},{"source":"ide-2","status":other}],
            "cliExecutable":"/installed/agy"
        })));
    host.account_reports.insert(
        "antigravity".into(),
        vec![report(
            "me@example.com",
            "2026-10-23T08:00:00Z",
            0.75,
            "2026-10-30T00:00:00Z",
        )],
    );
    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 3);
    assert_eq!(reading.additional_accounts.len(), 1);
    let account = legacy_account("Antigravity", "me@example.com", "").unwrap();
    let cli_rows: Vec<_> = reading
        .quotas
        .iter()
        .filter(|row| row["accountId"] == account)
        .collect();
    assert_eq!(cli_rows.len(), 1);
    assert_eq!(cli_rows[0]["windowDurationSeconds"], 604800);
    assert_eq!(
        reading.account.as_ref().unwrap()["checkedAt"],
        "2026-10-23T08:00:00+00:00"
    );
    assert!(host.started.borrow().is_empty());
}

#[test]
fn unknown_cli_quota_preserves_identity_and_readable_native_windows() {
    let mut host = answering(Ok(status_body()));
    host.account_reports.insert(
        "antigravity".into(),
        vec![json!({
            "email":"me@example.com","observedAt":"2026-10-23T08:00:00Z","quota":{}
        })],
    );
    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 2);
    assert!(reading.additional_accounts.is_empty());
}

#[test]
fn a_native_failure_does_not_discard_identified_cli_accounts() {
    let mut host = answering(Err("unavailable".into()));
    host.account_reports.insert(
        "antigravity".into(),
        vec![report(
            "me@example.com",
            "2026-10-23T08:00:00Z",
            0.75,
            "2026-10-30T00:00:00Z",
        )],
    );
    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 1);
    assert_eq!(
        reading.notice.as_deref(),
        Some("Some Antigravity accounts could not be read.")
    );
}

#[test]
fn duplicate_cli_reports_use_newer_cycle_then_newer_sample_in_any_order() {
    let old = report(
        "me@example.com",
        "2026-10-23T08:00:00Z",
        0.05,
        "2026-10-23T09:00:00Z",
    );
    let new = report(
        "ME@example.com",
        "2026-10-23T09:01:00Z",
        0.95,
        "2026-10-30T09:00:00Z",
    );
    for reports in [[old.clone(), new.clone()], [new.clone(), old]] {
        let mut reading = Reading::empty("antigravity");
        for report in reports {
            merge(
                &mut reading,
                super::super::statusline::parse(&report).unwrap(),
            );
        }
        assert_eq!(reading.quotas.len(), 1);
        assert!((reading.quotas[0]["usedPercent"].as_f64().unwrap() - 5.0).abs() < 0.001);
        assert_eq!(reading.quotas[0]["resetAt"], "2026-10-30T09:00:00+00:00");
        assert!(reading.additional_accounts.is_empty());
    }
    let newer = report(
        "me@example.com",
        "2026-10-23T09:02:00Z",
        0.97,
        "2026-10-30T09:00:00Z",
    );
    for reports in [[new.clone(), newer.clone()], [newer, new]] {
        let mut reading = Reading::empty("antigravity");
        for report in reports {
            merge(
                &mut reading,
                super::super::statusline::parse(&report).unwrap(),
            );
        }
        assert!((reading.quotas[0]["usedPercent"].as_f64().unwrap() - 3.0).abs() < 0.001);
        assert_eq!(reading.quotas[0]["observedAt"], "2026-10-23T09:02:00+00:00");
        assert_eq!(
            reading.account.unwrap()["checkedAt"],
            "2026-10-23T09:02:00+00:00"
        );
    }
}
