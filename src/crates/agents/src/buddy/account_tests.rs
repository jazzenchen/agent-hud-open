use super::summary::{PAID_PACKAGES, REWARD_PACKAGES, WEB_HOST};
use super::workbuddy_crypto::{build_aad, derive_workbuddy_key};
use super::*;
use crate::account::fake::FakeHost;
use crate::account::{Answer, HttpFailure, WebSession};
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use rusqlite::Connection;
use serde_json::{json, Value};

const WB_WEB_SUMMARY: &str = "https://www.workbuddy.cn/billing/meter/get-user-resource-summary";
const WB_WEB_PAID: &str = "https://www.workbuddy.cn/billing/meter/get-user-resource-paid-packages";
const WB_WEB_FREE: &str = "https://www.workbuddy.cn/billing/meter/get-user-resource-free-packages";
const WB_TOKEN_SUMMARY: &str =
    "https://copilot.tencent.com/billing/meter/get-user-resource-summary";
const CB_RESOURCE: &str = "https://www.codebuddy.ai/v2/billing/meter/get-user-resource";
const CBCN_ENT: &str = "https://www.codebuddy.cn/v2/billing/meter/get-enterprise-user-usage";

fn home() -> String {
    std::env::temp_dir()
        .join("agent-hud-buddy-fixture")
        .to_string_lossy()
        .into_owned()
}

fn signed_in_web() -> FakeHost {
    let mut fake = FakeHost::new(&home());
    fake.web_sessions.insert(
        format!("workbuddy|{WEB_HOST}"),
        WebSession {
            host: WEB_HOST.into(),
            cookie: "session=1".into(),
            user_agent: crate::account::WEB_USER_AGENT.into(),
        },
    );
    fake
}

fn summary_fixture() -> Value {
    json!({"code": 0, "data": {
        "SubscriptionPackageName": "Pro",
        "Packages": [
            {"CapacityUnit": "credits", "CycleTotalCapacity": "1000", "CycleRemainCapacity": "250"},
            {"CapacityUnit": "minutes", "CycleTotalCapacity": "60", "CycleRemainCapacity": "60"},
        ],
        "CreditGroups": [
            {"Kind": "benefit", "CapacityUnit": "credits", "CycleTotalCapacity": "200", "CycleRemainCapacity": "200"},
        ],
    }})
}

fn seal_wb_envelope(secret: &str, payload: &Value) -> Value {
    let (key_id, key_bytes) = derive_workbuddy_key(secret);
    let aad = build_aad(&key_id);
    let nonce = [3u8; 12];
    let unbound = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &key_bytes).unwrap());
    let mut buf = serde_json::to_vec(payload).unwrap();
    let tag = unbound
        .seal_in_place_separate_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(&aad),
            &mut buf,
        )
        .unwrap();
    let envelope = json!({
        "suite": 1,
        "keyId": key_id,
        "nonce": STANDARD.encode(nonce),
        "authTag": STANDARD.encode(tag.as_ref()),
        "ciphertext": STANDARD.encode(&buf),
    });
    json!({
        "$wbEncrypted": 1,
        "envelope": STANDARD.encode(envelope.to_string()),
    })
}

fn write_vscdb(path: &std::path::Path, key: &str, value: &str) {
    let conn = Connection::open(path).unwrap();
    conn.execute(
        "CREATE TABLE ItemTable(key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO ItemTable VALUES (?1, ?2)", [key, value])
        .unwrap();
}

#[test]
fn workbuddy_decrypts_wb_encrypted_auth_via_node_mode_and_respects_logged_out_marker() {
    let base = home();
    let auth_path = std::path::Path::new(&base)
        .join("AppData/CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info");
    let secret = "ZmFrZS1hdC1yZXN0LXNlY3JldC1rZXktNDQtY2hhcnMh";
    let sealed = seal_wb_envelope(
        secret,
        &json!({
            "auth": {"accessToken": "uid-88+wb-jwt-token"},
            "account": {"uid": "uid-88", "nickname": "wb-dev", "email": "wb@example.com"},
        }),
    );
    let mut host = FakeHost::new(&base)
        .file(auth_path.to_str().unwrap(), &sealed.to_string())
        .answer_for(
            WB_TOKEN_SUMMARY,
            "Bearer wb-jwt-token",
            Ok(summary_fixture()),
        );
    host.executables.insert(
        "workbuddy-app".into(),
        std::path::Path::new(&base).join("WorkBuddy"),
    );
    host.engine_output.insert(
        "-e".into(),
        vec![json!({"version": 1, "atRestSecretKey": secret})],
    );

    let reading = read_workbuddy(&host).unwrap();
    assert_eq!(reading.provider, "workbuddy");
    assert_eq!(reading.billing[0]["vendor"], "workbuddy");
    assert_eq!(reading.billing[0]["amount"], 450.0);
    let acc = reading.account.unwrap();
    assert_eq!(acc["provider"], "workbuddy");
    assert_eq!(acc["signIn"], "local");
    assert_eq!(acc["email"], "wb@example.com");

    // Presence of .logged-out marker skips the local session cleanly.
    let mut marker = auth_path.as_os_str().to_os_string();
    marker.push(".logged-out");
    let logged_out_host = host.file(std::path::Path::new(&marker).to_str().unwrap(), "");
    let empty = read_workbuddy(&logged_out_host).unwrap();
    assert!(empty.billing.is_empty() && empty.account.is_none());
}

#[test]
fn codebuddy_and_codebuddy_cn_read_separate_state_vscdb_providers() {
    let temp = tempfile::tempdir().unwrap();
    let cb_real = temp.path().join("cb.vscdb");
    let cbcn_real = temp.path().join("cbcn.vscdb");

    let cb_cipher = b"v10-cb-sealed".to_vec();
    let cbcn_cipher = b"v10-cbcn-sealed".to_vec();
    write_vscdb(
        &cb_real,
        r#"secret://{"extensionId":"tencent-cloud.coding-copilot","key":"planning-genie.new.accessToken"}"#,
        &json!({"type": "Buffer", "data": cb_cipher}).to_string(),
    );
    write_vscdb(
        &cbcn_real,
        r#"secret://{"extensionId":"tencent-cloud.coding-copilot","key":"planning-genie.new.accessTokencn"}"#,
        &json!({"type": "Buffer", "data": cbcn_cipher}).to_string(),
    );

    let base = home();
    let cb_logical =
        std::path::Path::new(&base).join("AppData/CodeBuddy/User/globalStorage/state.vscdb");
    let cbcn_logical =
        std::path::Path::new(&base).join("AppData/CodeBuddy CN/User/globalStorage/state.vscdb");

    let mut host = FakeHost::new(&base)
        .answer_for(
            CB_RESOURCE,
            "Bearer cb-jwt",
            Ok(json!({
                "code": 0,
                "data": {
                    "Response": {"Data": {"Accounts": [{
                        "PackageCode": "TCACA_code_002_AkiJS3ZHF5",
                        "PackageName": "CodeBuddy Pro",
                        "CycleCapacitySizePrecise": "1000",
                        "CycleCapacityRemainPrecise": "750",
                        "CycleEndTime": "2099-12-31 23:59:59"
                    }]}}
                }
            })),
        )
        .answer_for(
            CBCN_ENT,
            "Bearer cbcn-jwt",
            Ok(json!({
                "code": 0,
                "data": {
                    "limitNum": 2000,
                    "usedNum": 500,
                    "cycleEndTime": "2099-06-30 23:59:59"
                }
            })),
        );
    host.databases.insert(cb_logical, cb_real);
    host.databases.insert(cbcn_logical, cbcn_real);
    host.sealed.insert(
        [b"CodeBuddy|".as_slice(), cb_cipher.as_slice()].concat(),
        Answer::Found(
            json!({
                "auth": {"accessToken": "u1+cb-jwt"},
                "account": {"uid": "u1", "nickname": "cb-global"}
            })
            .to_string(),
        ),
    );
    host.sealed.insert(
        [b"CodeBuddy CN|".as_slice(), cbcn_cipher.as_slice()].concat(),
        Answer::Found(
            json!({
                "auth": {"accessToken": "u2+cbcn-jwt"},
                "account": {"uid": "u2", "nickname": "cb-china", "enterpriseId": "ent-9"}
            })
            .to_string(),
        ),
    );

    let cb = read_codebuddy(&host).unwrap();
    assert_eq!(cb.provider, "codebuddy");
    assert_eq!(cb.quotas[0]["vendor"], "codebuddy");
    assert_eq!(cb.quotas[0]["usedPercent"], 25.0);
    assert_eq!(cb.quotas[0]["nativeUsage"]["capacity"], 1000.0);
    assert_eq!(cb.account.unwrap()["plan"], "CodeBuddy Pro");

    let cbcn = read_codebuddy_cn(&host).unwrap();
    assert_eq!(cbcn.provider, "codebuddy-cn");
    assert_eq!(cbcn.quotas[0]["vendor"], "codebuddy-cn");
    assert_eq!(cbcn.quotas[0]["usedPercent"], 25.0);
    assert_eq!(cbcn.quotas[0]["nativeUsage"]["capacity"], 2000.0);
    assert_eq!(cbcn.account.unwrap()["plan"], "Enterprise");
}

#[test]
fn workbuddy_web_plans_page_fallback_and_errors_still_hold() {
    let paid = json!({"code": 0, "data": {"Accounts": [
        {"CapacityUnit": "credits", "CycleEndTime": "2099-12-31 23:59:59"},
    ]}});
    let host = signed_in_web()
        .answer(WB_WEB_SUMMARY, Ok(summary_fixture()))
        .answer(WB_WEB_PAID, Ok(paid));
    let reading = read_workbuddy(&host).unwrap();
    assert_eq!(reading.billing[0]["amount"], 450.0);
    assert_eq!(reading.billing[0]["planCredits"], 250.0);
    assert_eq!(reading.billing[0]["rewardCredits"], 200.0);
    assert_eq!(reading.billing[0]["resetAt"], "2099-12-31T16:00:00+00:00");
    let sent = host.sent.borrow();
    assert_eq!(
        sent[1].1.as_ref().unwrap()["PackageCodes"][0],
        PAID_PACKAGES[0]
    );

    let free = json!({"code": 0, "data": {"Accounts": [
        {"CapacityUnit": "credits", "PackageCode": REWARD_PACKAGES[0], "CycleEndTime": "2098-01-01 00:00:00"},
        {"CapacityUnit": "credits", "PackageCode": "TCACA_code_001_PqouKr6QWV", "CycleEndTime": "2099-01-01 00:00:00"},
    ]}});
    let host = signed_in_web()
        .answer(WB_WEB_SUMMARY, Ok(summary_fixture()))
        .answer(
            WB_WEB_PAID,
            Ok(json!({"code": 0, "data": {"Accounts": []}})),
        )
        .answer(WB_WEB_FREE, Ok(free));
    assert_eq!(
        read_workbuddy(&host).unwrap().billing[0]["resetAt"],
        "2098-12-31T16:00:01+00:00"
    );

    let host = signed_in_web().answer(WB_WEB_SUMMARY, Err(HttpFailure::Status(401)));
    assert_eq!(read_workbuddy(&host).err().unwrap().kind(), "signedOut");
}
