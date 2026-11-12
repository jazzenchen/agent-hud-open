//! The desktop sign-in and the query string Doubao's quota API expects.
use crate::account::{Answer, Host, HttpFailure, HttpRequest, Unread, WEB_USER_AGENT};
use crate::l10n;
use crate::Base;
use serde_json::Value;
use std::time::Duration;

const ORIGIN: &str = "https://www.doubao.com";
/// The browser sends the whole sign-in. `sessionid` alone still gets
/// "login expired" from the quota API.
const COOKIES: &[&str] = &[
    "sessionid",
    "sessionid_ss",
    "sid_tt",
    "sid_guard",
    "uid_tt",
    "uid_tt_ss",
    "ttwid",
    "passport_csrf_token",
    "passport_csrf_token_default",
    "odin_tt",
    "d_ticket",
    "sid_ucp_v1",
    "ssid_ucp_v1",
    "multi_sids",
    "passport_auth_status",
    "passport_auth_status_ss",
    "session_tlb_tag",
    "n_mh",
    "s_v_web_id",
];

pub(super) fn session_cookie(host: &dyn Host) -> Result<Option<String>, Unread> {
    let mut parts = Vec::new();
    let mut denied = false;
    for name in COOKIES {
        match host.app_cookie("Doubao", "doubao.com", name) {
            Answer::Found(value) if !value.trim().is_empty() => {
                parts.push(format!("{name}={}", value.trim()));
            }
            Answer::Denied => denied = true,
            Answer::Failed if *name == "sessionid" => {
                return Err(Unread::transient(l10n::text(
                    "暂时无法读取豆包的登录，稍后自动重试",
                    "Couldn't read Doubao's sign-in; retrying shortly",
                )))
            }
            _ => {}
        }
    }
    if parts.iter().any(|part| part.starts_with("sessionid=")) {
        return Ok(Some(parts.join("; ")));
    }
    if denied {
        return Err(Unread::transient(l10n::text(
            "豆包加密了登录信息，系统拒绝了钥匙串访问。允许后会再试",
            "Doubao encrypted its sign-in, and the system refused keychain access. Allow it to retry.",
        )));
    }
    Ok(None)
}

/// Query the desktop client sends with a quota request. Without `aid` and
/// the device id, the API only answers that nobody is signed in.
pub(super) fn endpoint(host: &dyn Host, path: &str) -> String {
    let client = desktop_client(host);
    let mut pairs = vec![
        ("version_code", "20800".to_owned()),
        ("language", "zh".to_owned()),
        ("device_platform", "web".to_owned()),
        ("doubao_device_platform", "desktop".to_owned()),
        ("aid", "582478".to_owned()),
        ("real_aid", "582478".to_owned()),
        ("pkg_type", "release_version".to_owned()),
        ("region", "CN".to_owned()),
        ("sys_region", "CN".to_owned()),
        ("samantha_web", "1".to_owned()),
        ("web_platform", "desktop".to_owned()),
        ("use-olympus-account", "1".to_owned()),
        ("runtime", "web".to_owned()),
        ("client_platform", "pc_client".to_owned()),
    ];
    if let Some(id) = client.device_id {
        pairs.push(("device_id", id.clone()));
        pairs.push(("tea_uuid", id));
    }
    if let Some(version) = client.version {
        pairs.push(("pc_version", version.clone()));
        pairs.push(("doubao_pc_version", version));
    }
    let query = pairs
        .into_iter()
        .map(|(name, value)| format!("{name}={}", query_escape(&value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{ORIGIN}{path}?{query}")
}

struct DesktopClient {
    device_id: Option<String>,
    version: Option<String>,
}

fn desktop_client(host: &dyn Host) -> DesktopClient {
    let path = host.folder(Base::AppData).join("Doubao/Local State");
    let Some(bytes) = host.read_file(&path, 1_000_000).found() else {
        return DesktopClient {
            device_id: None,
            version: None,
        };
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return DesktopClient {
            device_id: None,
            version: None,
        };
    };
    let device_id = value["aha"]["device"]["device_id"]
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty() && text.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_owned);
    let version = [
        &value["saman"]["extension"]["hotfix_native_version"],
        &value["saman"]["last_create_browser_alias_version"],
        &value["saman"]["hotfix"]["native_version"],
    ]
    .into_iter()
    .find_map(|item| {
        item.as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty() && text.chars().all(|c| c.is_ascii_digit() || c == '.'))
            .map(str::to_owned)
    });
    DesktopClient { device_id, version }
}

fn query_escape(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// A business code that is not success. A dead sign-in is said on its own;
/// anything else keeps the API's own message.
pub(super) fn refusal(body: &Value) -> Option<Unread> {
    let code = code_of(body)?;
    if code == 0 || code == 200 {
        return None;
    }
    let message = body["msg"]
        .as_str()
        .or_else(|| body["message"].as_str())
        .unwrap_or("")
        .trim();
    let signed_out = code == 710_012_001
        || message.contains("登录")
        || message.to_ascii_lowercase().contains("login");
    if signed_out {
        return Some(Unread::SignedOut(l10n::text(
            "豆包的登录已失效，请在豆包中重新登录",
            "Doubao's sign-in expired; sign in again in Doubao",
        )));
    }
    let reason = if message.is_empty() {
        l10n::text("豆包没有返回额度", "Doubao did not return a quota")
    } else {
        message.to_owned()
    };
    Some(Unread::Failed(reason))
}

fn code_of(body: &Value) -> Option<i64> {
    match body.get("code")? {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

pub(super) fn post(
    host: &dyn Host,
    cookie: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, Unread> {
    let url = endpoint(host, path);
    host.http_json(&HttpRequest {
        url: &url,
        secrets: &[("Cookie", cookie)],
        headers: &[
            ("Accept", "application/json, text/plain, */*"),
            ("Origin", ORIGIN),
            ("Referer", "https://www.doubao.com/chat/"),
            ("User-Agent", WEB_USER_AGENT),
            ("Agw-Js-Conv", "str"),
        ],
        body,
        timeout: Duration::from_secs(12),
    })
    .map_err(|failure| match failure {
        HttpFailure::Status(401 | 403) => Unread::SignedOut(l10n::text(
            "豆包的登录已失效，请在豆包中重新登录",
            "Doubao's sign-in expired; sign in again in Doubao",
        )),
        HttpFailure::Unreachable => Unread::transient(l10n::text(
            "无法读取豆包额度，请检查网络",
            "Doubao's quota could not be reached; check the connection",
        )),
        other => other.unread(),
    })
}
