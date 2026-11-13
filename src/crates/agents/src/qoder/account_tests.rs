use super::*;
use crate::account::fake::FakeHost;
use crate::account::{Answer, HttpFailure, WebSession};
use rusqlite::Connection;
use serde_json::json;

const OPENAPI_GLOBAL: &str = "https://openapi.qoder.sh/sash/api/v2/me/usage";
const OPENAPI_CN: &str = "https://openapi.qoder.com.cn/sash/api/v2/me/usage";
const WEB_GLOBAL_CREDITS: &str = "https://qoder.com/api/v2/me/usages/big_model_credits";
const WEB_CN_CREDITS: &str = "https://qoder.com.cn/api/v2/me/usages/big_model_credits";

fn home() -> String {
    std::env::temp_dir()
        .join("agent-hud-qoder-fixture")
        .to_string_lossy()
        .into_owned()
}

fn write_qoder_vscdb(path: &std::path::Path, rows: &[(&str, &str)]) {
    let conn = Connection::open(path).unwrap();
    conn.execute(
        "CREATE TABLE ItemTable(key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .unwrap();
    for (k, v) in rows {
        conn.execute("INSERT INTO ItemTable VALUES (?1, ?2)", [k, v])
            .unwrap();
    }
}

#[test]
fn qoder_desktop_auth_v1_dat_queries_sash_usage_and_keeps_provider_qoder() {
    let base = home();
    let dat_path = std::path::Path::new(&base).join("AppData/com.qoder.app.stable/auth.v1.dat");
    let cipher = b"v10-qoder-app-dat".to_vec();
    let mut host = FakeHost::new(&base).answer_for(
        OPENAPI_GLOBAL,
        "Bearer qoder-desktop-token",
        Ok(json!({
            "result": {
                "qoderUsage": {
                    "plan_tier_name": "Pro",
                    "expiresAt": 1_794_000_000_000_i64,
                    "userQuota": {"used": 250, "total": 1000, "percentage": 25},
                    "addOnQuota": {"used": 50, "total": 1000}
                }
            }
        })),
    );
    host.files.insert(dat_path, Answer::Found(cipher.clone()));
    host.sealed.insert(
        [b"Qoder App|".as_slice(), cipher.as_slice()].concat(),
        Answer::Found(
            json!({
                "id": "u-qoder-1",
                "email": "user@qoder.com",
                "token": "qoder-desktop-token"
            })
            .to_string(),
        ),
    );

    let reading = read_qoder(&host).unwrap();
    assert_eq!(reading.provider, "qoder");
    assert_eq!(reading.quotas[0]["vendor"], "qoder");
    assert_eq!(reading.quotas[0]["usedPercent"], 15.0);
    assert_eq!(reading.quotas[0]["nativeUsage"]["capacity"], 2000.0);
    let acc = reading.account.unwrap();
    assert_eq!(acc["provider"], "qoder");
    assert_eq!(acc["plan"], "Pro");
    assert_eq!(acc["email"], "user@qoder.com");
    assert_eq!(acc["signIn"], "local");
}

#[test]
fn qoder_cn_reads_state_vscdb_and_a_refused_qoder_work_token_is_a_sign_out() {
    let temp = tempfile::tempdir().unwrap();
    let cn_db = temp.path().join("qodercn.vscdb");
    let work_db = temp.path().join("qoderwork.vscdb");

    let cn_user = b"v10-cn-u".to_vec();
    write_qoder_vscdb(
        &cn_db,
        &[(
            "secret://aicoding.auth.userInfo",
            &json!({"type": "Buffer", "data": cn_user}).to_string(),
        )],
    );

    let work_user = b"v10-work-u".to_vec();
    let work_plan = b"v10-work-p".to_vec();
    let work_usage = b"v10-work-q".to_vec();
    write_qoder_vscdb(
        &work_db,
        &[
            (
                "secret://aicoding.auth.userInfo",
                &json!({"type": "Buffer", "data": work_user}).to_string(),
            ),
            (
                "secret://aicoding.auth.userPlan",
                &json!({"type": "Buffer", "data": work_plan}).to_string(),
            ),
            (
                "secret://aicoding.auth.creditUsage",
                &json!({"type": "Buffer", "data": work_usage}).to_string(),
            ),
        ],
    );

    let base = home();
    let mut host = FakeHost::new(&base)
        .answer_for(
            OPENAPI_CN,
            "Bearer cn-token",
            Ok(json!({
                "qoderUsage": {
                    "plan_tier_name": "CN Pro",
                    "userQuota": {"used": 100, "total": 500}
                }
            })),
        )
        // Work token is expired (401): a sign-out, never hidden behind the
        // cached creditUsage, and no refresh is asked for.
        .answer_for(
            OPENAPI_GLOBAL,
            "Bearer expired-work-token",
            Err(HttpFailure::Status(401)),
        );
    host.databases.insert(
        std::path::Path::new(&base).join("AppData/QoderCN/User/globalStorage/state.vscdb"),
        cn_db,
    );
    host.databases.insert(
        std::path::Path::new(&base).join("AppData/QoderWork/User/globalStorage/state.vscdb"),
        work_db,
    );
    host.sealed.insert(
        [b"QoderCN|".as_slice(), cn_user.as_slice()].concat(),
        Answer::Found(
            json!({"id": "cn-1", "name": "cn-dev", "token": "cn-token", "loginMethod": "qodercn"})
                .to_string(),
        ),
    );
    host.sealed.insert(
        [b"QoderWork|".as_slice(), work_user.as_slice()].concat(),
        Answer::Found(
            json!({"id": "wk-1", "name": "wk-dev", "token": "expired-work-token", "loginMethod": "qoder"})
                .to_string(),
        ),
    );
    host.sealed.insert(
        [b"QoderWork|".as_slice(), work_plan.as_slice()].concat(),
        Answer::Found(json!({"plan_tier_name": "Enterprise"}).to_string()),
    );
    host.sealed.insert(
        [b"QoderWork|".as_slice(), work_usage.as_slice()].concat(),
        Answer::Found(json!({"used": 300, "total": 1200}).to_string()),
    );

    let cn = read_qoder_cn(&host).unwrap();
    assert_eq!(cn.provider, "qoder-cn");
    assert_eq!(cn.quotas[0]["vendor"], "qoder-cn");
    assert_eq!(cn.quotas[0]["usedPercent"], 20.0);
    assert_eq!(cn.account.unwrap()["plan"], "CN Pro");

    assert!(matches!(read_qoder_work(&host), Err(Unread::SignedOut(_))));
}

#[test]
fn web_sign_in_fallback_and_legacy_qoder_cn_session_still_work() {
    let mut host = FakeHost::new(&home());
    host.web_sessions.insert(
        "qoder|qoder.com".into(),
        WebSession {
            host: "qoder.com".into(),
            cookie: "sid=g".into(),
            user_agent: crate::account::WEB_USER_AGENT.into(),
        },
    );
    // Legacy CN web session stored under "qoder|qoder.com.cn".
    host.web_sessions.insert(
        "qoder|qoder.com.cn".into(),
        WebSession {
            host: "qoder.com.cn".into(),
            cookie: "sid=c".into(),
            user_agent: crate::account::WEB_USER_AGENT.into(),
        },
    );
    let host = host
        .answer(
            WEB_GLOBAL_CREDITS,
            Ok(json!({
                "totalQuota": {"quotaSummary": {"usedValue": 300, "limitValue": 2000}},
                "nextResetAt": 1_794_000_000_000_i64,
            })),
        )
        .answer(
            WEB_CN_CREDITS,
            Ok(json!({
                "total_quota": {"quota_summary": {"used_value": 100, "limit_value": 1000}},
                "shared_quota": {"quota_summary": {"used_value": 100, "limit_value": 1000}},
            })),
        );

    let g = read_qoder(&host).unwrap();
    assert_eq!(g.quotas[0]["vendor"], "qoder");
    assert_eq!(g.quotas[0]["usedPercent"], 15.0);

    let c = read_qoder_cn(&host).unwrap();
    assert_eq!(c.quotas[0]["vendor"], "qoder-cn");
    assert_eq!(c.quotas[0]["usedPercent"], 10.0);
}
