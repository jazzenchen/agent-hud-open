use super::parse::parse;
use super::*;
use crate::account::cipher::aes_128_cbc_seal_for_test;
use crate::account::fake::FakeHost;
use crate::account::{HttpFailure, WebSession};
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::digest::{digest, SHA512};
use serde_json::Value;

const ROW_TOKEN: &str = "https://ug-normal.trae.ai/cloudide/api/v3/common/GetUserToken";
const ROW_LIST: &str = "https://ug-normal.trae.ai/trae/api/v1/pay/user_current_entitlement_list";
const US_TOKEN: &str = "https://ug-normal.us.trae.ai/cloudide/api/v3/common/GetUserToken";
const US_LIST: &str = "https://ug-normal.us.trae.ai/trae/api/v1/pay/user_current_entitlement_list";
const CN_TOKEN: &str = "https://api.trae.cn/cloudide/api/v3/common/GetUserToken";
const CN_LIST: &str = "https://api.trae.cn/trae/api/v2/pay/user_current_entitlement_list";
const CN_LOCAL_USAGE: &str = "https://api.trae.cn/trae/api/v2/pay/ide_user_ent_usage";
const SG_LOCAL_USAGE: &str = "https://growsg-normal.trae.ai/trae/api/v1/pay/ide_user_ent_usage";

fn home() -> String {
    std::env::temp_dir()
        .join("agent-hud-trae-fixture")
        .to_string_lossy()
        .into_owned()
}

fn signed_in(provider: &str, site: &str) -> FakeHost {
    let mut fake = FakeHost::new(&home());
    fake.web_sessions.insert(
        format!("{provider}|{site}"),
        WebSession {
            host: site.into(),
            cookie: "sessionid=fixture".into(),
            user_agent: crate::account::WEB_USER_AGENT.into(),
        },
    );
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

fn seal_byte_crypto(payload: &Value) -> String {
    const AES_A: [u8; 64] = [
        82, 9, 106, 213, 48, 54, 165, 56, 191, 64, 163, 158, 129, 243, 215, 251, 124, 227, 57, 130,
        155, 47, 255, 135, 52, 142, 67, 68, 196, 222, 233, 203, 84, 123, 148, 50, 166, 194, 35, 61,
        238, 76, 149, 11, 66, 250, 195, 78, 8, 46, 161, 102, 40, 217, 36, 178, 118, 91, 162, 73,
        109, 139, 209, 37,
    ];
    const AES_B: [u8; 64] = [
        31, 221, 168, 51, 136, 7, 199, 49, 177, 18, 16, 89, 39, 128, 236, 95, 96, 81, 127, 169, 25,
        181, 74, 13, 45, 229, 122, 159, 147, 201, 156, 239, 160, 224, 59, 77, 174, 42, 245, 176,
        200, 235, 187, 60, 131, 83, 153, 97, 23, 43, 4, 126, 186, 119, 214, 38, 225, 105, 20, 99,
        85, 33, 12, 125,
    ];
    let header = [116u8, 99, 5, 16, 0, 0];
    let nonce = [9u8; 32];
    let mut salt = [0u8; 64];
    for i in 0..64 {
        salt[i] = AES_A[i] ^ AES_B[i];
    }
    let key_hash = digest(&SHA512, &nonce);
    let mut merged_in = [0u8; 128];
    merged_in[..64].copy_from_slice(key_hash.as_ref());
    merged_in[64..].copy_from_slice(&salt);
    let merged = digest(&SHA512, &merged_in);
    let key: [u8; 16] = merged.as_ref()[..16].try_into().unwrap();
    let iv: [u8; 16] = merged.as_ref()[16..32].try_into().unwrap();
    let json_bytes = serde_json::to_vec(payload).unwrap();
    let checksum = digest(&SHA512, &json_bytes);
    let mut plain = Vec::with_capacity(64 + json_bytes.len());
    plain.extend_from_slice(checksum.as_ref());
    plain.extend_from_slice(&json_bytes);
    let ct = aes_128_cbc_seal_for_test(&key, &iv, &plain);
    let mut raw = Vec::with_capacity(38 + ct.len());
    raw.extend_from_slice(&header);
    raw.extend_from_slice(&nonce);
    raw.extend_from_slice(&ct);
    STANDARD.encode(raw)
}

#[test]
fn nothing_is_read_before_the_user_signs_in() {
    let host = FakeHost::new(&home());
    let reading = read_global(&host).unwrap();
    assert!(reading.quotas.is_empty() && reading.account.is_none());
    let cn = read_china(&host).unwrap();
    assert!(cn.quotas.is_empty() && cn.account.is_none());
    assert!(host.asked.borrow().is_empty());
}

#[test]
fn local_byte_crypto_storage_reads_trae_and_trae_cn_as_separate_providers() {
    let base = home();
    let trae_storage =
        std::path::Path::new(&base).join("AppData/Trae/User/globalStorage/storage.json");
    let cn_storage =
        std::path::Path::new(&base).join("AppData/Trae CN/User/globalStorage/storage.json");
    let global_cipher = seal_byte_crypto(&json!({
        "token": "local-global-jwt",
        "userId": "u-global",
        "region": "SG",
        "expiredAt": 2_099_000_000_i64,
        "account": {"email": "dev@trae.ai", "username": "dev"}
    }));
    let cn_cipher = seal_byte_crypto(&json!({
        "token": "local-cn-jwt",
        "userId": "u-cn",
        "account": {"username": "cn-user"}
    }));
    let host = FakeHost::new(&base)
        .file(
            trae_storage.to_str().unwrap(),
            &json!({
                "iCubeAuthInfo://usertag": "SG",
                "iCubeAuthInfo://icube.cloudide": global_cipher,
            })
            .to_string(),
        )
        .file(
            cn_storage.to_str().unwrap(),
            &json!({"iCubeAuthInfo://icube.cloudide": cn_cipher}).to_string(),
        )
        .answer_for(
            SG_LOCAL_USAGE,
            "Cloud-IDE-JWT local-global-jwt",
            Ok(json!({
                "Result": {
                    "is_dollar_usage_billing": true,
                    "user_entitlement_pack_list": [{
                        "entitlement_base_info": {"product_type": 1, "end_time": 1_796_083_199, "quota": {"basic_usage_limit": 20}},
                        "usage": {"basic_usage_amount": 5}
                    }]
                }
            })),
        )
        .answer_for(
            CN_LOCAL_USAGE,
            "Cloud-IDE-JWT local-cn-jwt",
            Ok(json!({"data": credits(2000.0, 500.0)})),
        );

    let g = read_global(&host).unwrap();
    assert_eq!(g.provider, "trae");
    assert_eq!(g.quotas[0]["vendor"], "trae");
    assert_eq!(g.quotas[0]["usedPercent"], 25.0);
    let g_acc = g.account.unwrap();
    assert_eq!(g_acc["provider"], "trae");
    assert_eq!(g_acc["signIn"], "local");
    assert_eq!(g_acc["email"], "dev@trae.ai");
    assert_eq!(g_acc["plan"], "Pro");

    let c = read_china(&host).unwrap();
    assert_eq!(c.provider, "trae-cn");
    assert_eq!(c.quotas[0]["vendor"], "trae-cn");
    assert_eq!(c.quotas[0]["usedPercent"], 25.0);
    assert_eq!(c.quotas[0]["nativeUsage"]["capacity"], 2000.0);
    let c_acc = c.account.unwrap();
    assert_eq!(c_acc["provider"], "trae-cn");
    assert_eq!(c_acc["label"], "cn-user");
    assert_eq!(c_acc["signIn"], "local");
}

#[test]
fn expired_local_trae_token_reports_signed_out_without_exchange() {
    let base = home();
    let path = std::path::Path::new(&base).join("AppData/Trae/User/globalStorage/storage.json");
    let expired = seal_byte_crypto(&json!({
        "token": "stale",
        "refreshToken": "do-not-touch",
        "expiredAt": 1_700_000_000_i64,
    }));
    let host = FakeHost::new(&base).file(
        path.to_str().unwrap(),
        &json!({"iCubeAuthInfo://icube.cloudide": expired}).to_string(),
    );
    assert_eq!(read_global(&host).err().unwrap().kind(), "signedOut");
    assert!(host.asked.borrow().is_empty());
}

#[test]
fn trae_cn_web_fallback_reads_legacy_and_dedicated_web_sessions() {
    // Legacy web session stored under "trae|trae.cn" continues to work seamlessly.
    let host = signed_in("trae", "trae.cn")
        .answer(CN_TOKEN, token("fixture-jwt"))
        .answer_for(
            CN_LIST,
            "Cloud-IDE-JWT fixture-jwt",
            Ok(credits(4000.0, 1000.0)),
        );
    let reading = read_china(&host).unwrap();
    let quota = &reading.quotas[0];
    assert_eq!(quota["vendor"], "trae-cn");
    assert_eq!(quota["usedPercent"], 25.0);
    assert_eq!(quota["nativeUsage"]["capacity"], 4000.0);
    assert_eq!(quota["nativeUsage"]["unit"], "credits");
    assert!(quota["resetAt"].is_null());
    let account = reading.account.unwrap();
    assert_eq!(account["provider"], "trae-cn");
    assert_eq!(account["label"], "TRAE CN");
    assert_eq!(account["plan"], "Pro");
    assert_eq!(account["signIn"], "web");
}

fn pack(kind: i64, quota: Value, usage: Value) -> Value {
    json!({"entitlement_base_info": {"product_type": kind, "end_time": 1_796_083_199, "quota": quota},
           "usage": usage})
}

fn dollars(packs: Value) -> Value {
    json!({"code": 0, "is_dollar_usage_billing": true, "user_entitlement_pack_list": packs})
}

fn share(body: &Value) -> f64 {
    let summary = parse(body).unwrap();
    let quota = summary.quota(&GLOBAL, GLOBAL.site, "trae:fixture").unwrap();
    assert!(quota["nativeUsage"].is_null());
    quota["usedPercent"].as_f64().unwrap()
}

#[test]
fn international_dollar_usage_and_legacy_fast_requests_parse_cleanly() {
    let body = dollars(json!([pack(
        0,
        json!({"basic_usage_limit": 4}),
        json!({"basic_usage_amount": 1})
    )]));
    let host = signed_in("trae", "trae.ai")
        .answer(ROW_TOKEN, token("row-jwt"))
        .answer_for(ROW_LIST, "Cloud-IDE-JWT row-jwt", Ok(body));
    let reading = read_global(&host).unwrap();
    assert_eq!(reading.quotas[0]["usedPercent"], 25.0);
    assert_eq!(reading.quotas[0]["resetAt"], "2026-12-01T00:00:00+00:00");

    // Legacy fast-request packs and product_type 100 ("尝鲜版").
    let fast = json!({
        "user_entitlement_pack_list": [{
            "entitlement_base_info": {"product_type": 100, "quota": {"premium_model_fast_request_limit": 500}},
            "usage": {"premium_model_fast_request_amount": 125}
        }]
    });
    let summary = parse(&fast).unwrap();
    assert_eq!(summary.plan.as_deref(), Some("尝鲜版"));
    let quota = summary.quota(&CHINA, CHINA.site, "trae-cn:f").unwrap();
    assert_eq!(quota["usedPercent"], 25.0);
    assert_eq!(quota["nativeUsage"]["unit"], "requests");
}

#[test]
fn bonus_tiers_unlimited_and_error_classifications_hold() {
    let bonus = json!({"basic_usage_limit": 10, "bonus_usage_limit": 10});
    let used = json!({"basic_usage_amount": 10, "bonus_usage_amount": 5});
    assert_eq!(share(&dollars(json!([pack(1, bonus, used.clone())]))), 75.0);
    let none = json!({"basic_usage_limit": 10, "bonus_usage_limit": 10, "no_bonus_quota": true});
    assert_eq!(share(&dollars(json!([pack(1, none, used)]))), 100.0);

    let unlimited = dollars(json!([pack(
        6,
        json!({"basic_usage_limit": -1}),
        json!({"basic_usage_amount": 120})
    )]));
    assert!(parse(&unlimited).unwrap().unlimited());

    let host = signed_in("trae", "trae.ai")
        .answer(ROW_TOKEN, Err(HttpFailure::Status(401)))
        .answer(US_TOKEN, token("us-jwt"))
        .answer_for(US_LIST, "Cloud-IDE-JWT us-jwt", Ok(credits(100.0, 50.0)));
    assert_eq!(read_global(&host).unwrap().quotas[0]["usedPercent"], 50.0);

    let host = signed_in("trae-cn", "trae.cn")
        .answer(CN_TOKEN, token("t"))
        .answer(CN_LIST, Ok(json!({"code": 1001, "message": "not login"})));
    assert_eq!(read_china(&host).err().unwrap().kind(), "signedOut");
}
