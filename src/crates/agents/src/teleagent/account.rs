//! TeleAgent's points detail, from its own account API
//! `GET /user/portal/companyStore/userCenter`
//! (`https://agent.teleai.com.cn/superCowork/sapi`).
//! The sign-in token is `users/<owner>/app-auth/token.json`, sealed with
//! Electron safe storage.
//! Points spent over the last 7 days come from
//! `GET /user/portal/usage/stats/overview`.
use crate::account::{Answer, Host, HttpFailure, HttpRequest, Provider, Reading, Unread};
use crate::l10n;
use crate::Base;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

mod reading;
mod spend;

use reading::parse;
use spend::recent_spend;

const BASE: &str = "https://agent.teleai.com.cn/superCowork/sapi";
const CENTER: &str = "/user/portal/companyStore/userCenter";
const OVERVIEW: &str = "/user/portal/usage/stats/overview";
const RECORDS: &str = "/user/portal/usage/stats/record";
/// Days in the daily pace, including today.
const SPEND_DAYS: i64 = 7;
/// Daily bars on the detail card, covering the statistics page's 90 days.
const HISTORY_DAYS: i64 = 90;
const PAGE_SIZE: i64 = 100;
const PAGE_LIMIT: i64 = 4;

/// Points change on other devices, so a reading is not tied to local use.
pub static PROVIDER: Provider = Provider {
    id: "teleagent",
    read,
    local_work: false,
    work_sources: &["teleagent"],
    keeps_unreported_windows: false,
    reported: None,
    status: None,
    web: None,
};

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let root = host.folder(Base::DataHome).join("TeleAgent").join("users");
    let owners = match host.list_dir(&root) {
        Answer::Found(entries) => entries,
        _ => return Ok(Reading::empty("teleagent")),
    };
    let mut reading = Reading::empty("teleagent");
    let mut failures = Vec::new();
    let mut saw = false;
    for owner in owners {
        let name = owner
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if !account_name(name) {
            continue;
        }
        saw = true;
        match read_owner(host, &owner, name) {
            Ok(mut one) => {
                // `merge` folds quotas and the account, not balances.
                let billing = std::mem::take(&mut one.billing);
                crate::account::merge(&mut reading, one);
                reading.billing.extend(billing);
            }
            Err(unread) => failures.push(unread),
        }
    }
    if !saw {
        return Ok(Reading::empty("teleagent"));
    }
    if reading.account.is_none() {
        return Err(Unread::combined(failures).unwrap_or_else(|| {
            Unread::Failed(l10n::text(
                "TeleAgent 的积分无法读取",
                "TeleAgent's points could not be read",
            ))
        }));
    }
    if let Some(unread) = Unread::combined(failures) {
        crate::account::append_notice(&mut reading, unread.reason().to_owned());
    }
    Ok(reading)
}

fn account_name(name: &str) -> bool {
    name.len() <= 64
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn read_owner(host: &dyn Host, owner: &Path, name: &str) -> Result<Reading, Unread> {
    let path = owner.join("app-auth").join("token.json");
    let bytes = match host.read_file(&path, 64 * 1024) {
        Answer::Found(bytes) => bytes,
        Answer::Missing => {
            return Err(Unread::SignedOut(l10n::text(
                "登录 TeleAgent 后读取积分",
                "Sign in to TeleAgent to read its points",
            )))
        }
        _ => {
            return Err(Unread::Failed(l10n::text(
                "无法读取 TeleAgent 的登录信息",
                "TeleAgent's sign-in could not be read",
            )))
        }
    };
    let stored: Value = serde_json::from_slice(&bytes).map_err(|_| {
        Unread::Failed(l10n::text(
            "TeleAgent 的登录信息格式无法识别",
            "TeleAgent's sign-in has an unsupported format",
        ))
    })?;
    let token = open_token(host, &stored)?;
    let url = format!("{BASE}{CENTER}");
    let os = match host.os() {
        crate::Os::MacOs => "macOS",
        crate::Os::Windows => "Windows",
        crate::Os::Linux => "Linux",
    };
    let body = host
        .http_json(&HttpRequest {
            url: &url,
            secrets: &[("X-Token", &token)],
            headers: &[("X-OS-Type", os)],
            body: None,
            timeout: Duration::from_secs(12),
        })
        .map_err(|failure| match failure {
            HttpFailure::Status(401 | 403) => Unread::SignedOut(l10n::text(
                "TeleAgent 的登录已失效，请在 TeleAgent 中重新登录",
                "TeleAgent's sign-in expired; sign in again in TeleAgent",
            )),
            HttpFailure::Unreachable => Unread::transient(l10n::text(
                "无法读取 TeleAgent 积分，请检查网络",
                "TeleAgent's points could not be reached; check the connection",
            )),
            other => other.unread(),
        })?;
    // A missing spend rate still leaves the points balance valid.
    let spend = recent_spend(host, &token, os);
    parse(name, &data(&body)?, spend)
}

fn open_token(host: &dyn Host, stored: &Value) -> Result<String, Unread> {
    if stored["scheme"] != "safe_storage" {
        return Err(Unread::Failed(l10n::text(
            "TeleAgent 的登录信息格式无法识别",
            "TeleAgent's sign-in has an unsupported format",
        )));
    }
    let encoded = stored["encryptedToken"].as_str().ok_or_else(|| {
        Unread::Failed(l10n::text(
            "TeleAgent 的登录信息格式无法识别",
            "TeleAgent's sign-in has an unsupported format",
        ))
    })?;
    let encrypted = STANDARD.decode(encoded).map_err(|_| {
        Unread::Failed(l10n::text(
            "TeleAgent 的登录信息格式无法识别",
            "TeleAgent's sign-in has an unsupported format",
        ))
    })?;
    match host.unseal_safe_storage("TeleAgent", &encrypted) {
        Answer::Found(token) if !token.is_empty() => Ok(token),
        Answer::Denied => Err(Unread::Failed(l10n::text(
            "TeleAgent 加密了登录信息，系统拒绝了钥匙串访问。允许后会再试",
            "TeleAgent encrypted its sign-in, and the system refused keychain access. Allow it to retry.",
        ))),
        Answer::Missing => Err(Unread::Failed(l10n::text(
            "TeleAgent 加密了登录信息，但找不到它的钥匙串项",
            "TeleAgent encrypted its sign-in, but its keychain item is missing.",
        ))),
        Answer::Unsupported => Err(Unread::Failed(l10n::text(
            "TeleAgent 加密了登录信息，此系统还不能解密",
            "TeleAgent encrypted its sign-in; this system cannot unlock it yet.",
        ))),
        _ => Err(Unread::Failed(l10n::text(
            "TeleAgent 加密的登录信息无法解密",
            "TeleAgent's encrypted sign-in could not be unlocked.",
        ))),
    }
}

/// The API puts the payload in `data` when `code` is 0 or 200.
fn data(body: &Value) -> Result<Value, Unread> {
    let ok = body["code"] == 0 || body["code"] == 200 || body["success"] == true;
    if ok {
        return Ok(body["data"].clone());
    }
    if body["current_points_balance"].is_number() || body["activities"].is_array() {
        return Ok(body.clone());
    }
    Err(Unread::Failed(l10n::text(
        "TeleAgent 返回的积分格式无法识别",
        "TeleAgent's points have an unsupported format",
    )))
}

pub(super) fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().trim_end_matches('%').parse().ok())
        .filter(|n| n.is_finite() && *n >= 0.0)
}

pub(super) fn label_text(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;
    use crate::account::{Answer, HttpFailure};
    use serde_json::json;

    const URL: &str =
        "https://agent.teleai.com.cn/superCowork/sapi/user/portal/companyStore/userCenter";

    fn sealed() -> (Vec<u8>, String) {
        let bytes = b"v10-teleagent-token".to_vec();
        let stored = json!({
            "version": 1,
            "encrypted": true,
            "scheme": "safe_storage",
            "encryptedToken": STANDARD.encode(&bytes),
        })
        .to_string();
        (bytes, stored)
    }

    fn host_with(body: Result<Value, HttpFailure>) -> FakeHost {
        let (bytes, stored) = sealed();
        let mut host = FakeHost::new("/u").answer(URL, body);
        host.files
            .insert(token_file(&host), Answer::Found(stored.into_bytes()));
        host.sealed
            .insert(bytes, Answer::Found("secret-token".into()));
        host
    }

    /// The token file, built the same way `read` looks it up, on every OS.
    fn token_file(host: &FakeHost) -> std::path::PathBuf {
        host.folder(crate::Base::DataHome)
            .join("TeleAgent")
            .join("users")
            .join("v1_public_1")
            .join("app-auth")
            .join("token.json")
    }

    #[test]
    fn the_points_balance_and_each_activity_are_read() {
        let body = json!({
            "code": 0,
            "data": {
                "plan_display_name": "免费版",
                "current_points_balance": 19575.06,
                "activities": [{
                    "name": "积分福利社",
                    "balance": 12875,
                    "remaining_activity_points": 19575.06,
                    "total_activity_points": 19700,
                    "usage_percent": "99.37%",
                    "expire_days": 18
                }]
            }
        });
        let reading = read(&host_with(Ok(body))).unwrap();
        assert!(reading.quotas.is_empty());
        assert_eq!(reading.billing.len(), 1);
        assert_eq!(reading.billing[0]["currency"], "credits");
        let group = &reading.billing[0]["pointsDetail"]["groups"][0];
        assert_eq!(group["total"], 19700.0);
        assert!((group["remaining"].as_f64().unwrap() - 19575.06).abs() < 0.001);
        assert!((group["percent"].as_f64().unwrap() - 99.37).abs() < 0.001);
        assert_eq!(group["grant"]["name"], "积分福利社");
        assert_eq!(group["grant"]["points"], 12875.0);
        assert_eq!(group["grant"]["expireDays"], 18);
        assert!(reading.billing[0]["pointsDetail"]["spent"].is_null());
    }

    #[test]
    fn the_recent_spend_is_a_daily_rate() {
        let today = chrono::Local::now().date_naive();
        let start = today - chrono::Duration::days(6);
        let overview = format!(
            "{BASE}{OVERVIEW}?startDate={}&endDate={}",
            start.format("%Y-%m-%d"),
            today.format("%Y-%m-%d")
        );
        let mut host = host_with(Ok(json!({
            "code": 0,
            "data": {
                "current_points_balance": 100,
                "activities": [{
                    "name": "积分福利社",
                    "remaining_activity_points": 80,
                    "total_activity_points": 100,
                    "usage_percent": "80%"
                }]
            }
        })));
        let history_start = today - chrono::Duration::days(HISTORY_DAYS - 1);
        let records = format!(
            "{BASE}{RECORDS}?startDate={}&endDate={}&page=1&pageSize={PAGE_SIZE}",
            history_start.format("%Y-%m-%d"),
            today.format("%Y-%m-%d")
        );
        host.web
            .insert(overview, Ok(json!({"code": 0, "data": {"pointsUsed": 70}})));
        host.web.insert(
            records,
            Ok(json!({
                "code": 0,
                "data": {
                    "total": 1,
                    "list": [{
                        "statTime": format!("{} 09:00:00", today.format("%Y-%m-%d")),
                        "totalUsage": 20
                    }]
                }
            })),
        );
        let reading = read(&host).unwrap();
        let detail = &reading.billing[0]["pointsDetail"];
        assert_eq!(detail["spent"]["days"], 7);
        assert!((detail["spent"]["perDay"].as_f64().unwrap() - 10.0).abs() < 0.001);
        let today_key = today.format("%Y-%m-%d").to_string();
        let today_row = detail["days"]
            .as_array()
            .unwrap()
            .iter()
            .find(|day| day["date"] == today_key)
            .unwrap();
        assert_eq!(today_row["points"], 20.0);
    }

    #[test]
    fn the_request_sends_the_unsealed_token() {
        let host = host_with(Ok(json!({
            "code": 200,
            "data": {"current_points_balance": 1, "plan_display_name": "个人版"}
        })));
        read(&host).unwrap();
        let sent = host.sent.borrow();
        assert_eq!(sent[0].0[0].0, "X-Token");
        assert_eq!(sent[0].0[0].1, "secret-token");
    }

    #[test]
    fn without_teleagent_there_is_nothing_to_say() {
        let reading = read(&FakeHost::new("/u")).unwrap();
        assert!(reading.account.is_none() && reading.billing.is_empty());
    }

    #[test]
    fn an_expired_sign_in_says_so() {
        let Err(error) = read(&host_with(Err(HttpFailure::Status(401)))) else {
            panic!("an expired sign-in is an error");
        };
        assert_eq!(
            error.reason(),
            "TeleAgent's sign-in expired; sign in again in TeleAgent"
        );
        assert_eq!(error.kind(), "signedOut");
    }

    #[test]
    fn a_sign_in_this_system_cannot_open_says_so() {
        let (_bytes, stored) = sealed();
        let mut host = FakeHost::new("/u");
        host.files
            .insert(token_file(&host), Answer::Found(stored.into_bytes()));
        let Err(error) = read(&host) else {
            panic!("a sealed sign-in this system cannot open is an error");
        };
        assert_eq!(
            error.reason(),
            "TeleAgent encrypted its sign-in; this system cannot unlock it yet."
        );
    }
}
