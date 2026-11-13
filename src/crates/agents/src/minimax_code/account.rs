//! MiniMax Code's (`id: "minimax-code"`) read-only local account reader.
//! Reads `~/.minimax/auth/<env>/<region>/mcode-public/auth.json` (and companion
//! `auth-state.json`) without ever calling `/oauth2/token` (which single-use
//! rotates MiniMax's refresh token), then queries MiniMax's user & membership APIs.
use crate::account::{account_id, Host, HttpFailure, HttpRequest, Provider, Reading, Unread};
use crate::l10n;
use chrono::Utc;
use serde_json::{json, Value};
use std::time::Duration;

mod local;
use local::{discover_auth, str_field};

const ORIGIN_CN: &str = "https://agent.minimax.cn";
const ORIGIN_EN: &str = "https://agent.minimax.io";

pub static PROVIDER: Provider = Provider {
    local_work: false,
    work_sources: &["minimax-code"],
    ..Provider::new("minimax-code", read)
};

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let Some(local) = discover_auth(host)? else {
        return Ok(Reading::empty("minimax-code"));
    };
    let origin = if local.region.eq_ignore_ascii_case("en")
        || local.region.eq_ignore_ascii_case("global")
        || local.region.eq_ignore_ascii_case("io")
    {
        ORIGIN_EN
    } else {
        ORIGIN_CN
    };
    let bearer = format!("Bearer {}", local.access_token);
    let info = request(host, &format!("{origin}/v1/api/user/info"), &bearer, None)?;
    let (uid, nickname, email) = parse_user_info(&info);

    let extra = request(
        host,
        &format!("{origin}/matrix/api/v1/user/get_user_extra_info"),
        &bearer,
        Some(&json!({})),
    )?;
    let personal = extra["workspaces"]
        .as_array()
        .and_then(|items| items.iter().find(|w| w["workspace_type"] == 0));
    let ws_body = personal
        .and_then(|w| w.get("workspace_id"))
        .filter(|v| !v.is_null())
        .map(|id| json!({"workspace_id": id}))
        .unwrap_or_else(|| json!({}));
    let membership = request(
        host,
        &format!("{origin}/matrix/api/v1/commerce/get_membership_info"),
        &bearer,
        Some(&ws_body),
    )?;

    build_reading(
        &local.region,
        uid.as_deref(),
        nickname,
        email,
        personal,
        &membership,
    )
}

fn request(
    host: &dyn Host,
    url: &str,
    bearer: &str,
    body: Option<&Value>,
) -> Result<Value, Unread> {
    let root = host
        .http_json(&HttpRequest {
            url,
            secrets: &[("Authorization", bearer)],
            headers: &[("Accept", "application/json")],
            body,
            timeout: Duration::from_secs(15),
        })
        .map_err(|failure| match failure {
            HttpFailure::Status(401 | 403) => signed_out(),
            other => other.unread(),
        })?;
    check_envelope(&root)?;
    Ok(root)
}

fn check_envelope(body: &Value) -> Result<(), Unread> {
    if let Some(err) = body["error"].as_str().filter(|s| !s.is_empty()) {
        if err.to_ascii_lowercase().contains("token") || err.to_ascii_lowercase().contains("auth") {
            return Err(signed_out());
        }
        return Err(Unread::from(err.to_owned()));
    }
    for (env_key, code_key) in [("base_resp", "status_code"), ("statusInfo", "code")] {
        if let Some(code) = body[env_key][code_key].as_i64() {
            if code == 1004 || code == 401 {
                return Err(signed_out());
            }
            if code != 0 {
                return Err(format_err());
            }
        }
    }
    Ok(())
}

fn parse_user_info(root: &Value) -> (Option<String>, Option<String>, Option<String>) {
    let inner = root
        .get("data")
        .or_else(|| root.get("userInfo"))
        .or_else(|| root.get("user_info"))
        .filter(|v| v.is_object())
        .unwrap_or(root);
    let uid = str_field(&inner["id"])
        .or_else(|| str_field(&inner["user_id"]))
        .or_else(|| str_field(&inner["userId"]));
    let nickname = str_field(&inner["name"])
        .or_else(|| str_field(&inner["nickname"]))
        .or_else(|| str_field(&inner["username"]));
    let email = str_field(&inner["email"]);
    (uid, nickname, email)
}

fn build_reading(
    region: &str,
    uid: Option<&str>,
    nickname: Option<String>,
    email: Option<String>,
    workspace: Option<&Value>,
    membership: &Value,
) -> Result<Reading, Unread> {
    if membership.get("subscription_type").is_none()
        && membership.get("op_credit_summary").is_none()
        && membership.get("opcredit_balance").is_none()
    {
        return Err(format_err());
    }
    let credits = &membership["op_credit_summary"];
    let total = credit_num(&credits["total_remaining_amount"])
        .or_else(|| credit_num(&membership["opcredit_balance"]))
        .or_else(|| workspace.and_then(|w| credit_num(&w["opcredit_balance"])));
    let purchased = credit_num(&credits["purchased_remaining_amount"])
        .or_else(|| credit_num(&credits["purchased_credit"]));
    let gifted = credit_num(&credits["free_remaining_amount"])
        .or_else(|| credit_num(&credits["free_credit"]));

    let unsubscribed = membership["subscription_type"] == "none";
    let plan = (!unsubscribed)
        .then(|| {
            str_field(&membership["token_plan_tier"])
                .or_else(|| str_field(&membership["plan_name"]))
        })
        .flatten();

    let id = account_id(
        "minimax-code",
        &[region, uid.or(email.as_deref()).unwrap_or("default")],
    );
    let label = email
        .as_deref()
        .or(nickname.as_deref())
        .unwrap_or("MiniMax Code")
        .to_owned();

    let mut reading = Reading::empty("minimax-code");
    if let Some(amount) = total.filter(|&n| n > 0.0 || !unsubscribed) {
        reading.billing.push(json!({
            "id": format!("{id}:credits"),
            "vendor": "minimax-code",
            "consumerSource": "minimax-code",
            "accountId": id,
            "account": label,
            "label": l10n::text("可用积分", "Available credits"),
            "amount": round2(amount),
            "currency": "credits",
            "planCredits": purchased.filter(|&n| n > 0.0).map(round2),
            "rewardCredits": gifted.filter(|&n| n > 0.0).map(round2),
        }));
    } else {
        reading.notice = Some(l10n::text(
            "MiniMax Code 当前没有可用的套餐或积分额度",
            "MiniMax Code reports no active plan or credit balance for this account.",
        ));
    }

    let mut acc = json!({
        "id": id,
        "provider": "minimax-code",
        "label": label,
        "plan": plan,
        "checkedAt": Utc::now().to_rfc3339(),
        "signIn": "local",
    });
    if let Some(email) = email {
        acc["email"] = json!(email);
    }
    reading.account = Some(acc);
    Ok(reading)
}

fn credit_num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let cleaned = s.replace(',', "");
            let trimmed = cleaned.trim();
            (!trimmed.is_empty())
                .then(|| trimmed.parse::<f64>().ok())
                .flatten()
        }
        _ => None,
    }
    .filter(|n| n.is_finite() && *n >= 0.0)
}

fn round2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

fn signed_out() -> Unread {
    Unread::SignedOut(l10n::text(
        "MiniMax Code 保存的登录已失效，请打开 MiniMax Code 重新登录",
        "MiniMax Code's saved sign-in has ended; sign in again in MiniMax Code.",
    ))
}

fn format_err() -> Unread {
    Unread::from(l10n::text(
        "MiniMax Code 的额度格式无法识别",
        "MiniMax Code's membership response has an unsupported format.",
    ))
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
