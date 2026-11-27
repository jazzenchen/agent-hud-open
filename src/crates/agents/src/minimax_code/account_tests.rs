use super::*;
use crate::account::fake::FakeHost;
use serde_json::json;

fn home() -> String {
    std::env::temp_dir()
        .join("agent-hud-minimax-code-fixture")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn reads_plaintext_auth_json_and_membership_balance_under_minimax_code_provider() {
    let base = home();
    let auth_path =
        std::path::Path::new(&base).join(".minimax/auth/prod/cn/mcode-public/auth.json");
    let host = FakeHost::new(&base)
        .file(
            auth_path.to_str().unwrap(),
            &json!({
                "accessToken": "mcode-token-1",
                "refreshToken": "do-not-rotate",
                "expiresAtMs": 2_099_000_000_000_i64,
                "region": "cn"
            })
            .to_string(),
        )
        .answer_for(
            "https://agent.minimax.cn/v1/api/user/info",
            "Bearer mcode-token-1",
            Ok(json!({
                "data": {"id": "mm-1", "name": "MiniMax Dev", "email": "dev@minimax.cn"},
                "base_resp": {"status_code": 0}
            })),
        )
        .answer_for(
            "https://agent.minimax.cn/matrix/api/v1/user/get_user_extra_info",
            "Bearer mcode-token-1",
            Ok(json!({
                "workspaces": [{"workspace_id": "ws-personal", "workspace_type": 0}],
                "base_resp": {"status_code": 0}
            })),
        )
        .answer_for(
            "https://agent.minimax.cn/matrix/api/v1/commerce/get_membership_info",
            "Bearer mcode-token-1",
            Ok(json!({
                "subscription_type": "monthly",
                "token_plan_tier": "M Plan Plus",
                "op_credit_summary": {
                    "total_remaining_amount": "1,250.50",
                    "purchased_remaining_amount": "1,000.50",
                    "free_remaining_amount": "250"
                },
                "base_resp": {"status_code": 0}
            })),
        );

    let reading = read(&host).unwrap();
    assert_eq!(reading.provider, "minimax-code");
    assert_eq!(reading.billing.len(), 1);
    assert_eq!(reading.billing[0]["vendor"], "minimax-code");
    assert_eq!(reading.billing[0]["amount"], 1250.5);
    assert_eq!(reading.billing[0]["planCredits"], 1000.5);
    assert_eq!(reading.billing[0]["rewardCredits"], 250.0);
    let acc = reading.account.unwrap();
    assert_eq!(acc["provider"], "minimax-code");
    assert_eq!(acc["plan"], "M Plan Plus");
    assert_eq!(acc["email"], "dev@minimax.cn");
}

#[test]
fn respects_signed_out_companion_state_and_never_rotates_expired_refresh_token() {
    let base = home();
    let folder = std::path::Path::new(&base).join(".minimax/auth/prod/cn/mcode-public");
    let auth_path = folder.join("auth.json");
    let state_path = folder.join("auth-state.json");

    // Signed out in auth-state.json -> empty reading, zero HTTP calls.
    let signed_out_host = FakeHost::new(&base)
        .file(
            auth_path.to_str().unwrap(),
            &json!({"accessToken": "tok", "expiresAtMs": 2_099_000_000_000_i64}).to_string(),
        )
        .file(
            state_path.to_str().unwrap(),
            &json!({"status": "signed_out"}).to_string(),
        );
    let empty = read(&signed_out_host).unwrap();
    assert!(empty.billing.is_empty() && empty.account.is_none());
    assert!(signed_out_host.asked.borrow().is_empty());

    // Expired access token -> transient Unread, zero HTTP calls (/oauth2/token never touched).
    let expired_host = FakeHost::new(&base).file(
        auth_path.to_str().unwrap(),
        &json!({
            "accessToken": "stale",
            "refreshToken": "single-use",
            "expiresAtMs": 1_700_000_000_000_i64
        })
        .to_string(),
    );
    assert_eq!(read(&expired_host).err().unwrap().kind(), "transient");
    assert!(expired_host.asked.borrow().is_empty());
}

#[test]
fn reads_the_current_record_from_the_auth_envelope_without_using_an_older_login() {
    let base = home();
    let folder = std::path::Path::new(&base).join(".minimax/auth/prod/cn/mcode-public");
    let auth_path = folder.join("auth.json");
    let current = json!({
        "schemaVersion": 1,
        "records": {
            "opaque-old": {
                "accessToken": "old-login",
                "refreshToken": "do-not-use-old-refresh",
                "expiresAtMs": 2_099_000_000_000_i64,
                "generation": 1,
                "loginEpoch": "100"
            },
            "opaque-current": {
                "accessToken": "current-login",
                "refreshToken": "do-not-rotate-current-refresh",
                "expiresAtMs": 2_099_000_000_000_i64,
                "generation": 2,
                "loginEpoch": "200",
                "region": "cn"
            }
        }
    });
    let host = FakeHost::new(&base)
        .file(auth_path.to_str().unwrap(), &current.to_string())
        .answer_for(
            "https://agent.minimax.cn/v1/api/user/info",
            "Bearer current-login",
            Ok(json!({"data": {"id": "mm-current"}, "base_resp": {"status_code": 0}})),
        )
        .answer_for(
            "https://agent.minimax.cn/matrix/api/v1/user/get_user_extra_info",
            "Bearer current-login",
            Ok(json!({"workspaces": [], "base_resp": {"status_code": 0}})),
        )
        .answer_for(
            "https://agent.minimax.cn/matrix/api/v1/commerce/get_membership_info",
            "Bearer current-login",
            Ok(json!({
                "subscription_type": "monthly",
                "opcredit_balance": 25,
                "base_resp": {"status_code": 0}
            })),
        );

    let reading = read(&host).unwrap();
    assert_eq!(reading.billing[0]["amount"], 25.0);
    assert_eq!(host.asked.borrow().len(), 3);

    // An expired current generation must not silently reuse an older, still
    // unexpired access token. No request or refresh-token rotation is allowed.
    let mut expired = current.clone();
    expired["records"]["opaque-current"]["expiresAtMs"] = json!(1_700_000_000_000_i64);
    let expired_host = FakeHost::new(&base).file(auth_path.to_str().unwrap(), &expired.to_string());
    assert_eq!(read(&expired_host).err().unwrap().kind(), "transient");
    assert!(expired_host.asked.borrow().is_empty());
}
