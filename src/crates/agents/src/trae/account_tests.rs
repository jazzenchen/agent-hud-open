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
            {"group_type": 0, "status": 1, "display_desc": " Pro ", "expire_time": 2_000_000_000},
            {"group_type": 4, "status": 1, "expire_time": 1_850_000_000},
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

#[test]
fn international_dollar_usage_is_a_share_of_the_plans_allowance() {
    let body = json!({
        "is_dollar_usage_billing": true,
        "user_entitlement_pack_list": [
            {"group_type": 0, "status": 1, "display_desc": "Pro",
             "entitlement_base_info": {"quota": {"basic_usage_limit": 10}},
             "usage": {"basic_usage_amount": 2.5}},
            {"is_hide": true, "entitlement_base_info": {"quota": {"basic_usage_limit": 99}}},
            {"entitlement_base_info": {"product_extra": {"subscription_extra": {"quota": {"basic_usage_limit": 10}}}}},
        ],
    });
    let host = signed_in(&["trae.ai"])
        .answer(ROW_TOKEN, token("row-jwt"))
        .answer_for(ROW_LIST, "Cloud-IDE-JWT row-jwt", Ok(body));
    let reading = read(&host).unwrap();
    let quota = &reading.quotas[0];
    assert_eq!(quota["vendor"], "trae");
    assert_eq!(quota["name"], "Usage");
    assert_eq!(quota["usedPercent"], 12.5);
    assert!(quota["nativeUsage"].is_null());
    assert_eq!(reading.account.unwrap()["label"], "TRAE");
    // An unlimited plan has no share to show; a missing summary is no use.
    let unlimited = json!({"is_dollar_usage_billing": true, "user_entitlement_pack_list": [
            {"entitlement_base_info": {"quota": {"basic_usage_limit": -1}}}]});
    assert!(parse(&unlimited).is_none());
    assert!(parse(&json!({"is_credits_billing": true})).is_none());
    assert!(parse(&json!({"usage_summary": {"total_amount": 9, "consumed_amount": 1}})).is_none());
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
