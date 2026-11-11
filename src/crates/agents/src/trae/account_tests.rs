use super::entitlements::parse;
use super::*;
use crate::account::fake::FakeHost;
use crate::account::{HttpFailure, WebSession};
use serde_json::Value;

const ROW_TOKEN: &str = "https://ug-normal.trae.ai/cloudide/api/v3/common/GetUserToken";
const ROW_LIST: &str = "https://ug-normal.trae.ai/trae/api/v1/pay/user_current_entitlement_list";
const US_TOKEN: &str = "https://ug-normal.us.trae.ai/cloudide/api/v3/common/GetUserToken";
const US_LIST: &str = "https://ug-normal.us.trae.ai/trae/api/v1/pay/user_current_entitlement_list";
const CN_TOKEN: &str = "https://api.trae.cn/cloudide/api/v3/common/GetUserToken";
const CN_LIST: &str = "https://api.trae.cn/trae/api/v2/pay/user_current_entitlement_list";

fn signed_in(hosts: &[&str]) -> FakeHost {
    let mut fake = FakeHost::new("/u");
    for host in hosts {
        fake.web_sessions.insert(
            format!("trae|{host}"),
            WebSession {
                host: (*host).into(),
                cookie: "sessionid=fixture".into(),
                user_agent: crate::account::WEB_USER_AGENT.into(),
            },
        );
    }
    fake
}

fn token(value: &str) -> Result<Value, HttpFailure> {
    Ok(json!({"ResponseMetadata": {}, "Result": {"Token": value}}))
}

fn credits(total: f64, used: f64) -> Value {
    json!({
        "code": 0, "is_credits_billing": true,
        "usage_summary": {"total_amount": total, "consumed_amount": used},
        "user_entitlement_pack_list": [
            {"entitlement_base_info": {"product_type": 1, "end_time": 2_000_000_000}},
            {"entitlement_base_info": {"product_type": 2, "end_time": 1_850_000_000}},
        ],
    })
}

#[test]
fn nothing_is_read_before_the_user_signs_in() {
    let host = FakeHost::new("/u");
    let reading = read(&host).unwrap();
    assert!(reading.quotas.is_empty() && reading.account.is_none());
    assert!(host.asked.borrow().is_empty());
}

#[test]
fn trae_cn_credits_are_read_with_a_token_the_session_is_traded_for() {
    let host = signed_in(&["trae.cn"])
        .answer(CN_TOKEN, token("fixture-jwt"))
        .answer_for(
            CN_LIST,
            "Cloud-IDE-JWT fixture-jwt",
            Ok(credits(4000.0, 1000.0)),
        );
    let reading = read(&host).unwrap();
    let quota = &reading.quotas[0];
    assert_eq!(quota["vendor"], "trae-cn");
    assert_eq!(quota["usedPercent"], 25.0);
    assert_eq!(quota["nativeUsage"]["capacity"], 4000.0);
    assert_eq!(quota["nativeUsage"]["unit"], "credits");
    assert!(quota["resetAt"].is_null());
    let account = reading.account.unwrap();
    assert_eq!(account["label"], "TRAE CN");
    assert_eq!(account["plan"], "Pro");
    let sent = host.sent.borrow();
    assert_eq!(
        sent[0].0,
        [("Cookie".to_owned(), "sessionid=fixture".to_owned())]
    );
    assert_eq!(
        sent[1].1,
        Some(json!({"require_usage": true, "full_data": true}))
    );
    let headers = host.headers.borrow();
    assert!(headers[1]
        .iter()
        .any(|(name, value)| name == "Origin" && value == "https://www.trae.cn"));
}

/// One pack of the entitlement list: its kind (`product_type`), quota and
/// usage.
fn pack(kind: i64, quota: Value, usage: Value) -> Value {
    json!({"entitlement_base_info": {"product_type": kind, "end_time": 1_796_083_199, "quota": quota},
           "usage": usage})
}

fn dollars(packs: Value) -> Value {
    json!({"code": 0, "is_dollar_usage_billing": true, "user_entitlement_pack_list": packs})
}

/// The share of a dollar allowance used, which shows no amounts.
fn share(body: &Value) -> f64 {
    let summary = parse(body).unwrap();
    let quota = summary
        .quota(&GLOBAL, GLOBAL.site(), "trae:fixture")
        .unwrap();
    assert!(quota["nativeUsage"].is_null());
    quota["usedPercent"].as_f64().unwrap()
}

#[test]
fn international_dollar_usage_is_a_share_of_the_plans_allowance() {
    let body = dollars(json!([pack(
        0,
        json!({"basic_usage_limit": 4}),
        json!({"basic_usage_amount": 1})
    )]));
    let host = signed_in(&["trae.ai"])
        .answer(ROW_TOKEN, token("row-jwt"))
        .answer_for(ROW_LIST, "Cloud-IDE-JWT row-jwt", Ok(body));
    let reading = read(&host).unwrap();
    let quota = &reading.quotas[0];
    assert_eq!(quota["vendor"], "trae");
    assert_eq!(quota["name"], "Usage");
    assert_eq!(quota["usedPercent"], 25.0);
    // TRAE's panel: the plan resets a second after its period ends.
    assert_eq!(quota["resetAt"], "2026-12-01T00:00:00+00:00");
    assert!(quota["nativeUsage"].is_null());
    let account = reading.account.unwrap();
    assert_eq!(account["label"], "TRAE");
    assert_eq!(account["plan"], "Free");
    // An answer that is neither credits nor dollars is no use.
    assert!(parse(&json!({"usage_summary": {"total_amount": 9, "consumed_amount": 1}})).is_none());
}

#[test]
fn a_plans_bonus_counts_once_its_basic_allowance_is_used_up() {
    let bonus = json!({"basic_usage_limit": 10, "bonus_usage_limit": 10});
    let used = json!({"basic_usage_amount": 10, "bonus_usage_amount": 5});
    assert_eq!(share(&dollars(json!([pack(1, bonus, used.clone())]))), 75.0);
    // A plan without a bonus counts its basic allowance alone.
    let none = json!({"basic_usage_limit": 10, "bonus_usage_limit": 10, "no_bonus_quota": true});
    assert_eq!(share(&dollars(json!([pack(1, none, used.clone())]))), 100.0);
    let missing = json!({"basic_usage_limit": 20});
    assert_eq!(share(&dollars(json!([pack(1, missing, used)]))), 50.0);
}

#[test]
fn the_plan_is_the_highest_tier_and_other_packs_are_no_part_of_it() {
    let body = dollars(json!([
        pack(0, json!({"basic_usage_limit": 3}), json!({"basic_usage_amount": 3})),
        pack(8, json!({"basic_usage_limit": 5}), json!({})),
        pack(7, json!({"basic_usage_limit": 100}), json!({"basic_usage_amount": 50})),
        pack(2, json!({"basic_usage_limit": 50}), json!({})),
        pack(3, json!({"basic_usage_limit": 30}), json!({"basic_usage_amount": 30})),
        {"entitlement_base_info": {"product_type": 4, "end_time": 1_798_761_599,
            "quota": {"basic_usage_limit": 60, "bonus_usage_limit": 20}},
         "usage": {"basic_usage_amount": 20}, "status": 3, "expire_time": 1_900_000_000},
    ]));
    let summary = parse(&body).unwrap();
    assert_eq!(summary.plan.as_deref(), Some("Pro+"));
    let quota = summary
        .quota(&GLOBAL, GLOBAL.site(), "trae:fixture")
        .unwrap();
    assert_eq!(quota["usedPercent"], 25.0);
    // The reset is the plan's period end, not its pack's expiry.
    assert_eq!(quota["resetAt"], "2027-01-01T00:00:00+00:00");
    for (kind, plan) in [(6, "Ultra"), (1, "Pro"), (8, "Lite")] {
        let body = dollars(json!([
            pack(0, json!({"basic_usage_limit": 1}), json!({})),
            pack(kind, json!({"basic_usage_limit": 1}), json!({})),
        ]));
        assert_eq!(parse(&body).unwrap().plan.as_deref(), Some(plan));
    }
    // Promo codes alone are no plan, and so no allowance.
    let promo = dollars(json!([pack(
        3,
        json!({"basic_usage_limit": 30}),
        json!({})
    )]));
    let summary = parse(&promo).unwrap();
    assert!(summary.plan.is_none() && !summary.unlimited());
    assert!(summary
        .quota(&GLOBAL, GLOBAL.site(), "trae:fixture")
        .is_none());
}

#[test]
fn a_limit_of_minus_one_is_no_limit_and_not_an_unreadable_answer() {
    let unlimited = dollars(json!([pack(
        6,
        json!({"basic_usage_limit": -1}),
        json!({"basic_usage_amount": 120})
    )]));
    let host = signed_in(&["trae.ai"])
        .answer(ROW_TOKEN, token("row-jwt"))
        .answer(ROW_LIST, Ok(unlimited));
    let reading = read(&host).unwrap();
    assert!(reading.quotas.is_empty() && reading.notice.is_none());
    assert_eq!(reading.account.unwrap()["plan"], "Ultra");
    // A bonus with no limit is not counted.
    let bonus = json!({"basic_usage_limit": 10, "bonus_usage_limit": -1});
    let used = json!({"basic_usage_amount": 4, "bonus_usage_amount": 3});
    assert_eq!(share(&dollars(json!([pack(1, bonus, used)]))), 40.0);
    // A plan whose basic limit is missing has changed format.
    assert!(parse(&dollars(json!([pack(1, json!({}), json!({}))]))).is_none());
    // Credits: one pack with no limit leaves the account with none.
    let credits = json!({"is_credits_billing": true, "user_entitlement_pack_list": [
        {"entitlement_base_info": {"quota": {"credits_limit": 500}}, "usage": {"credits_amount": 100}},
        {"entitlement_base_info": {"quota": {"credits_limit": -1}}, "usage": {"credits_amount": 900}},
    ]});
    assert!(parse(&credits).unwrap().unlimited());
}

#[test]
fn trae_cn_credits_are_what_each_pack_has_left_as_its_title_bar_counts_them() {
    let body = json!({
        "is_credits_billing": true,
        "usage_summary": {"total_amount": 1, "consumed_amount": 1},
        "user_entitlement_pack_list": [
            {"entitlement_base_info": {"product_type": 1, "quota": {"credits_limit": 3000}},
             "usage": {"credits_amount": 1000}},
            // Spent past its limit: nothing left of it, and no more counted.
            {"entitlement_base_info": {"product_type": 2, "quota": {"credits_limit": 1000}},
             "usage": {"credits_amount": 1500}},
            // A promo code's credits count too; a pack of none does not.
            {"entitlement_base_info": {"product_type": 3, "quota": {"credits_limit": 0}},
             "usage": {"credits_amount": 700}},
            {"entitlement_base_info": {"product_type": 7, "quota": {}}},
        ],
    });
    let summary = parse(&body).unwrap();
    assert_eq!(summary.plan.as_deref(), Some("Pro"));
    let quota = summary.quota(&CHINA, CHINA.site(), "trae:cn").unwrap();
    assert_eq!(quota["usedPercent"], 50.0);
    assert_eq!(quota["nativeUsage"]["used"], 2000.0);
    assert_eq!(quota["nativeUsage"]["capacity"], 4000.0);
    assert!(quota["resetAt"].is_null());
    // Packs with no credits limits leave the site's own summary; with
    // neither, the answer is no use.
    let summary = parse(&credits(4000.0, 1000.0)).unwrap();
    let quota = summary.quota(&CHINA, CHINA.site(), "trae:cn").unwrap();
    assert_eq!(quota["usedPercent"], 25.0);
    assert!(parse(&json!({"is_credits_billing": true})).is_none());
}

#[test]
fn a_us_account_is_read_from_the_host_that_knows_it() {
    let host = signed_in(&["trae.ai"])
        .answer(ROW_TOKEN, Err(HttpFailure::Status(401)))
        .answer(US_TOKEN, token("us-jwt"))
        .answer_for(US_LIST, "Cloud-IDE-JWT us-jwt", Ok(credits(100.0, 50.0)));
    assert_eq!(read(&host).unwrap().quotas[0]["usedPercent"], 50.0);
    // Refused by every host: signed out.
    let host = signed_in(&["trae.ai"])
        .answer(ROW_TOKEN, Err(HttpFailure::Status(401)))
        .answer(US_TOKEN, Err(HttpFailure::Status(403)));
    assert_eq!(read(&host).err().unwrap().kind(), "signedOut");
    // A host that cannot be reached says nothing about the sign-in, and
    // the next is not asked in its place.
    let host = signed_in(&["trae.ai"])
        .answer(ROW_TOKEN, Err(HttpFailure::Unreachable))
        .answer(US_TOKEN, Err(HttpFailure::Status(401)));
    assert_eq!(read(&host).err().unwrap().kind(), "transient");
    assert_eq!(host.asked.borrow().len(), 1);
}

#[test]
fn a_refused_session_is_a_sign_out_and_a_changed_answer_is_not() {
    let host = signed_in(&["trae.cn"])
        .answer(CN_TOKEN, token("t"))
        .answer(CN_LIST, Ok(json!({"code": 1001, "message": "not login"})));
    let why = read(&host).err().unwrap();
    assert_eq!(why.kind(), "signedOut");
    assert!(why.reason().contains("TRAE CN"));
    let host = signed_in(&["trae.cn"]).answer(CN_TOKEN, Err(HttpFailure::Status(302)));
    assert_eq!(read(&host).err().unwrap().kind(), "signedOut");
    let host = signed_in(&["trae.cn"]).answer(CN_TOKEN, Ok(json!({"Result": {}})));
    assert_eq!(read(&host).err().unwrap().kind(), "failed");
    let host = signed_in(&["trae.cn"])
        .answer(CN_TOKEN, token("t"))
        .answer(CN_LIST, Ok(json!({"is_credits_billing": false})));
    assert_eq!(read(&host).err().unwrap().kind(), "failed");
    let host = signed_in(&["trae.cn"]).answer(CN_TOKEN, Err(HttpFailure::Status(503)));
    assert_eq!(read(&host).err().unwrap().kind(), "transient");
}

#[test]
fn each_site_is_its_own_account_and_one_failing_keeps_the_other() {
    let host = signed_in(&["trae.ai", "trae.cn"])
        .answer(ROW_TOKEN, token("a"))
        .answer(ROW_LIST, Ok(credits(0.0, 0.0)))
        .answer(CN_TOKEN, Err(HttpFailure::Status(401)));
    let reading = read(&host).unwrap();
    assert!(reading.quotas.is_empty());
    assert_eq!(reading.account.unwrap()["label"], "TRAE");
    assert!(reading.notice.unwrap().contains("sign in again"));
    let host = signed_in(&["trae.ai", "trae.cn"])
        .answer(ROW_TOKEN, token("a"))
        .answer(ROW_LIST, Ok(credits(10.0, 1.0)))
        .answer(CN_TOKEN, token("b"))
        .answer(CN_LIST, Ok(credits(20.0, 5.0)));
    let reading = read(&host).unwrap();
    assert_eq!(reading.quotas.len(), 2);
    assert_ne!(
        reading.quotas[0]["accountId"],
        reading.quotas[1]["accountId"]
    );
    assert_eq!(reading.additional_accounts[0]["label"], "TRAE CN");
}
