//! Doubao quotas: the personal subscription and the activity gift, from the
//! desktop app's existing sign-in.
//! `POST /alice/commerce/sale/subscription/quota/summary/`
//! `POST /alice/commerce/sale/subscription/activity_reward_quota/`
//! (`https://www.doubao.com`).
use crate::account::{
    account_id, status, Answer, Host, HttpFailure, HttpRequest, Provider, Reading, Unread,
    WEB_USER_AGENT,
};
use crate::l10n;
use crate::naming::window_names::Period;
use crate::Base;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::time::Duration;

const ORIGIN: &str = "https://www.doubao.com";
const SUMMARY: &str = "/alice/commerce/sale/subscription/quota/summary/";
const REWARD: &str = "/alice/commerce/sale/subscription/activity_reward_quota/";
const TIMELINE: &str = "/alice/commerce/usage/timeline/";
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

/// Quota changes on other devices, so a reading is not tied to local use.
pub static PROVIDER: Provider = Provider {
    id: "doubao",
    read,
    local_work: false,
    work_sources: &["doubao"],
    keeps_unreported_windows: false,
    reported: None,
    status: None,
    web: None,
};

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let Some(cookie) = session_cookie(host)? else {
        return Ok(Reading::empty("doubao"));
    };
    let summary = post(
        host,
        &cookie,
        SUMMARY,
        Some(&json!({"product_line": "membership"})),
    )?;
    let reward = post(host, &cookie, REWARD, Some(&json!({})))?;
    let summary_refusal = refusal(&summary);
    let reward_refusal = refusal(&reward);
    if summary_refusal.is_some() && reward_refusal.is_some() {
        return Err(summary_refusal.unwrap_or_else(|| {
            reward_refusal.unwrap_or_else(|| {
                Unread::Failed(l10n::text(
                    "豆包返回的额度格式无法识别",
                    "Doubao's quota has an unsupported format",
                ))
            })
        }));
    }
    let blank = Value::Null;
    let summary_body = if summary_refusal.is_none() {
        &summary
    } else {
        &blank
    };
    let reward_body = if reward_refusal.is_none() {
        &reward
    } else {
        &blank
    };
    match parse(summary_body, reward_body) {
        Err(unread) => {
            if let Some(refusal) = summary_refusal.or(reward_refusal) {
                return Err(refusal);
            }
            Err(unread)
        }
        Ok(mut reading) => {
            attach_usage(&mut reading, host, &cookie);
            Ok(reading)
        }
    }
}

/// Doubao's usage list. A miss leaves the quotas already read.
/// `entries[].usage` carries `occurred_at_ms`, `display_name`, and a nested
/// `quota_source`; the share of the 7-day window is a percent field or a
/// string such as `0.11%` somewhere under that source.
fn attach_usage(reading: &mut Reading, host: &dyn Host, cookie: &str) {
    let Ok(response) = post(host, cookie, TIMELINE, Some(&json!({"page_size": 50}))) else {
        return;
    };
    if refusal(&response).is_some() {
        return;
    }
    let records = usage_records(payload(&response).unwrap_or(&response));
    if records.is_empty() {
        return;
    }
    if let Some(account) = reading.account.as_mut() {
        account["usageDetail"] = json!({ "records": records });
    }
}

fn usage_records(value: &Value) -> Vec<Value> {
    let mut found = Vec::new();
    collect_usage(value, &mut found);
    found.sort_by(|left, right| right["at"].as_str().cmp(&left["at"].as_str()));
    found.truncate(40);
    found
}

fn collect_usage(value: &Value, out: &mut Vec<Value>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_usage(item, out);
            }
        }
        Value::Object(object) => {
            if let Some(record) = usage_record(object) {
                out.push(record);
                return;
            }
            for child in object.values() {
                collect_usage(child, out);
            }
        }
        _ => {}
    }
}

/// A short label written for a person, such as a scene name, not an id or URL.
fn human_text(object: &serde_json::Map<String, Value>) -> Option<String> {
    object.values().find_map(|value| {
        let text = value.as_str()?.trim();
        let count = text.chars().count();
        (count >= 2 && count <= 40 && !text.contains("://") && text.chars().any(|c| !c.is_ascii()))
            .then(|| text.to_owned())
    })
}

fn any_time(object: &serde_json::Map<String, Value>) -> Option<String> {
    first_instant(
        object,
        &[
            "occurred_at_ms",
            "occurred_at",
            "consume_time",
            "occur_time",
            "event_time",
            "create_time",
            "created_at",
            "stat_time",
            "used_at",
            "usage_time",
            "record_time",
            "timestamp",
            "time",
            "ts",
        ],
    )
    .or_else(|| {
        object.iter().find_map(|(key, value)| {
            let name = key.to_ascii_lowercase();
            if name.contains("end")
                || name.contains("start")
                || name.contains("reset")
                || name.contains("expire")
            {
                return None;
            }
            if !(name.contains("time")
                || name.contains("date")
                || name.contains("occurred")
                || name.ends_with("_at")
                || name.ends_with("_ms")
                || name == "ts")
            {
                return None;
            }
            value
                .as_str()
                .and_then(normalize_time)
                .or_else(|| value.as_i64().and_then(instant))
                .or_else(|| value.as_f64().and_then(|n| instant(n as i64)))
        })
    })
}

fn usage_record(object: &serde_json::Map<String, Value>) -> Option<Value> {
    let at = any_time(object)?;
    let title = first_text(
        object,
        &[
            "scene_name",
            "scene",
            "consume_scene",
            "feature_name",
            "title",
            "name",
            "display_name",
        ],
    )
    .or_else(|| human_text(object));
    let source = source_label(object);
    let model = first_text(object, &["model_name", "model", "model_display_name"]);
    let percent = first_number(
        object,
        &[
            "used_percent",
            "window_percent",
            "occupy_percent",
            "percent",
            "ratio",
        ],
    )
    .or_else(|| share_in(object));
    let amount = first_number(
        object,
        &["used_amount", "consume_amount", "amount", "cost_amount"],
    );
    // A window reset is announced beside the rows. It has a time and a label,
    // and neither a source nor a share, so it is not a consumption.
    if percent.is_none() && amount.is_none() && source.is_none() {
        return None;
    }
    Some(json!({
        "at": at,
        "title": title.unwrap_or_default(),
        "source": source,
        "model": model.filter(|text| text != "-" && text != "—" && text != "--"),
        "percent": percent.map(round_share),
        "amount": amount,
    }))
}

/// `quota_source` is either the label itself or `{display_name, display_text, ...}`.
fn source_label(object: &serde_json::Map<String, Value>) -> Option<String> {
    if let Some(text) = first_text(
        object,
        &[
            "source_name",
            "pool_name",
            "feature_group_name",
            "quota_pool",
            "source",
        ],
    ) {
        return Some(text);
    }
    match object.get("quota_source") {
        Some(Value::String(text)) => label_text(text),
        Some(value) => label_in(value),
        None => None,
    }
}

fn label_text(text: &str) -> Option<String> {
    let text = text.trim();
    let count = text.chars().count();
    if text.is_empty()
        || count > 40
        || text.contains("://")
        || percent_text(text).is_some()
        || text.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some(text.to_owned())
}

fn label_in(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => label_text(text),
        Value::Object(object) => {
            for key in [
                "display_name",
                "plain_text",
                "text",
                "content",
                "default",
                "value",
                "name",
                "title",
                "display_text",
            ] {
                if let Some(child) = object.get(key) {
                    if let Some(text) = label_in(child) {
                        return Some(text);
                    }
                }
            }
            object.values().find_map(label_in)
        }
        Value::Array(items) => items.iter().find_map(label_in),
        _ => None,
    }
}

/// A share of the quota window: a percent field, or text like `0.11%` / `<0.01%`.
fn share_in(object: &serde_json::Map<String, Value>) -> Option<f64> {
    let mut found = None;
    let mut below = false;
    for (key, child) in object {
        walk_share(key, child, &mut found, &mut below);
        if found.is_some() {
            break;
        }
    }
    found
        .or(below.then_some(0.005))
        .or_else(|| object.get("quota_source").and_then(loose_share))
}

/// A small number under the quota source, when the response never names it
/// as a percent. Codes, ids and timestamps are not a share of the window.
fn loose_share(value: &Value) -> Option<f64> {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let name = key.to_ascii_lowercase();
                let skip = name.contains("code")
                    || name.contains("id")
                    || name.contains("type")
                    || name.contains("level")
                    || name.contains("status")
                    || name.contains("time")
                    || name.ends_with("_ms");
                if skip {
                    if let Some(n) = loose_share(child) {
                        return Some(n);
                    }
                    continue;
                }
                if let Some(n) = number(child).filter(|n| *n <= 100.0) {
                    return Some(n);
                }
                if let Some(n) = loose_share(child) {
                    return Some(n);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(loose_share),
        _ => None,
    }
}

fn walk_share(key: &str, value: &Value, found: &mut Option<f64>, below: &mut bool) {
    if found.is_some() {
        return;
    }
    let name = key.to_ascii_lowercase();
    if name.contains("less_than") && value.as_bool() == Some(true) {
        *below = true;
    }
    if let Some(n) = value.as_str().and_then(percent_text) {
        *found = Some(n);
        return;
    }
    let skip_number = name.contains("time")
        || name.contains("id")
        || name.ends_with("_ms")
        || name.contains("cursor")
        || name.contains("code")
        || name.contains("type")
        || name.contains("level")
        || name.contains("status");
    if !skip_number
        && (name.contains("percent")
            || name.contains("ratio")
            || name.contains("occupy")
            || name.contains("share"))
    {
        if let Some(n) = number(value) {
            *found = Some(n);
            return;
        }
    }
    match value {
        Value::Object(object) => {
            for (child_key, child) in object {
                walk_share(child_key, child, found, below);
                if found.is_some() {
                    return;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                walk_share(key, item, found, below);
                if found.is_some() {
                    return;
                }
            }
        }
        _ => {}
    }
}

fn percent_text(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    let below = trimmed.starts_with('<');
    let bare = trimmed.trim_start_matches('<').trim();
    let bare = bare.strip_suffix('%')?.trim();
    let n: f64 = bare.parse().ok()?;
    if !n.is_finite() || !(0.0..=100.0).contains(&n) {
        return None;
    }
    if below {
        return Some((n / 2.0).clamp(0.0001, n));
    }
    Some(n)
}

fn round_share(n: f64) -> f64 {
    (n * 10_000.0).round() / 10_000.0
}

fn session_cookie(host: &dyn Host) -> Result<Option<String>, Unread> {
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
fn endpoint(host: &dyn Host, path: &str) -> String {
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
fn refusal(body: &Value) -> Option<Unread> {
    let code = code_of(body)?;
    if code == 0 || code == 200 {
        return None;
    }
    let message = body["msg"]
        .as_str()
        .or_else(|| body["message"].as_str())
        .unwrap_or("")
        .trim();
    let signed_out = code == 7_100_120_01
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

fn post(host: &dyn Host, cookie: &str, path: &str, body: Option<&Value>) -> Result<Value, Unread> {
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

fn parse(summary: &Value, reward: &Value) -> Result<Reading, Unread> {
    let summary_body = payload(summary).unwrap_or(summary);
    let reward_body = payload(reward).unwrap_or(reward);
    // The live payload names two buckets: `window_limit_section` is the
    // personal subscription, and reward `entitlements` are the activity gift.
    let mut subscription = section_windows(&summary_body["window_limit_section"]);
    let mut gift = entitlement_windows(reward_body.get("entitlements"));
    if gift.is_empty() {
        gift = section_windows(&summary_body["incentive_quota_section"]);
    }
    if subscription.is_empty() && gift.is_empty() {
        let mut found = Vec::new();
        collect(summary_body, &mut found);
        let (legacy, summary_gift): (Vec<_>, Vec<_>) = found
            .into_iter()
            .partition(|window| !window.period.as_deref().is_some_and(is_gift));
        subscription = legacy;
        gift = Vec::new();
        collect(reward_body, &mut gift);
        if gift.is_empty() {
            gift = summary_gift;
        }
    }
    gift = one_gift_pool(gift);
    if subscription.is_empty() && gift.is_empty() {
        return Err(Unread::Failed(l10n::text(
            "豆包返回的额度格式无法识别",
            "Doubao's quota has an unsupported format",
        )));
    }
    let id = account_id("doubao", &["doubao.com"]);
    let observed = Utc::now().to_rfc3339();
    let mut reading = Reading::empty("doubao");
    let plan = plan_name(summary_body).or_else(|| plan_name(reward_body));
    push_windows(
        &mut reading,
        &id,
        &observed,
        &subscription,
        true,
        plan.as_deref(),
    );
    push_windows(&mut reading, &id, &observed, &gift, false, plan.as_deref());
    reading.account = Some(json!({
        "id": id,
        "provider": "doubao",
        "label": null,
        "plan": plan,
        "checkedAt": observed,
        "signIn": "app",
    }));
    Ok(reading)
}

fn push_windows(
    reading: &mut Reading,
    id: &str,
    observed: &str,
    windows: &[Window],
    subscription: bool,
    plan: Option<&str>,
) {
    for (index, window) in windows.iter().enumerate() {
        let (name, short) = if subscription {
            subscription_names(window.period.as_deref(), windows.len())
        } else if windows.len() == 1 {
            (
                l10n::text("活动赠送额度", "Activity gift"),
                l10n::text("赠送", "Gift"),
            )
        } else {
            (
                l10n::text(
                    &format!("活动赠送额度 {}", index + 1),
                    &format!("Activity gift {}", index + 1),
                ),
                l10n::text(
                    &format!("赠送{}", index + 1),
                    &format!("Gift {}", index + 1),
                ),
            )
        };
        let key = if subscription { "subscription" } else { "gift" };
        let mut row = json!({
            "id": format!("{id}:{key}:{index}"),
            "vendor": "doubao",
            "consumerSource": "doubao",
            "accountId": id,
            "account": plan,
            "name": name,
            "shortLabel": short,
            "usedPercent": window.percent,
            "resetAt": window.reset,
            "windowDurationSeconds": window.duration,
            "allModels": true,
            "observedAt": observed,
            "status": status(window.percent),
        });
        if window.has_capacity {
            row["nativeUsage"] = json!({
                "used": window.used,
                "capacity": window.total,
                "unit": "credits",
            });
        }
        reading.quotas.push(row);
    }
}

/// Several activity grants are one gift pool: their amounts add, and the
/// soonest expiry is the reset.
fn one_gift_pool(windows: Vec<Window>) -> Vec<Window> {
    if windows.len() <= 1 || windows.iter().any(|window| !window.has_capacity) {
        return windows;
    }
    let used = windows.iter().map(|window| window.used).sum();
    let total = windows.iter().map(|window| window.total).sum();
    let reset = windows.into_iter().filter_map(|window| window.reset).min();
    vec![Window {
        used,
        total,
        has_capacity: total > 0.0,
        percent: if total > 0.0 {
            (used / total * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        },
        period: None,
        reset,
        duration: None,
    }]
}

fn section_windows(section: &Value) -> Vec<Window> {
    let mut out = Vec::new();
    let Some(groups) = section["window_limit_groups"].as_array() else {
        return out;
    };
    for group in groups {
        let title = group["feature_group_name"]
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty());
        let Some(limits) = group["window_limits"].as_array() else {
            continue;
        };
        for limit in limits {
            let Some(object) = limit.as_object() else {
                continue;
            };
            let Some(mut window) = window_of(object) else {
                continue;
            };
            if window.duration.is_none() {
                if let Some(title) = title {
                    if is_gift(title) {
                        window.period = Some(title.to_owned());
                    } else if let Some(seconds) = period_seconds(title) {
                        window.period = Some(title.to_owned());
                        window.duration = Some(seconds);
                    }
                }
            }
            out.push(window);
        }
    }
    out
}

fn entitlement_windows(list: Option<&Value>) -> Vec<Window> {
    let mut out = Vec::new();
    let Some(items) = list.and_then(Value::as_array) else {
        return out;
    };
    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
        let Some(mut window) = window_of(object) else {
            continue;
        };
        if let Some(title) = item["display"]["title"].as_str() {
            if is_gift(title) {
                window.period = Some(title.to_owned());
            }
        }
        out.push(window);
    }
    out
}

fn subscription_names(period: Option<&str>, count: usize) -> (String, String) {
    let short = period.and_then(period_short).filter(|_| count > 1);
    if let Some(short) = short {
        (
            l10n::text(
                &format!("个人订阅 · {short}"),
                &format!("Personal subscription · {short}"),
            ),
            short,
        )
    } else {
        (
            l10n::text("个人订阅", "Personal subscription"),
            l10n::text("订阅", "Plan"),
        )
    }
}

fn period_short(text: &str) -> Option<String> {
    Period::from_seconds(period_seconds(text)).map(|period| period.after_word())
}

struct Window {
    used: f64,
    total: f64,
    has_capacity: bool,
    percent: f64,
    period: Option<String>,
    reset: Option<String>,
    duration: Option<f64>,
}

fn collect(value: &Value, out: &mut Vec<Window>) {
    collect_at(None, value, out);
}

/// A quota may sit in a list, or under a key such as `5h` / `7d`. A parent
/// that only sums its children yields those children.
fn collect_at(key: Option<&str>, value: &Value, out: &mut Vec<Window>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_at(None, item, out);
            }
        }
        Value::Object(object) => {
            if object.values().any(contains_window) {
                for (child_key, child) in object {
                    collect_at(Some(child_key), child, out);
                }
                return;
            }
            if let Some(mut window) = window_of(object) {
                if window.duration.is_none() {
                    if let Some(seconds) = key.and_then(period_seconds) {
                        window.period = key.map(str::to_owned);
                        window.duration = Some(seconds);
                    }
                }
                out.push(window);
            }
        }
        _ => {}
    }
}

fn contains_window(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(contains_window),
        Value::Object(object) => {
            window_of(object).is_some() || object.values().any(contains_window)
        }
        _ => false,
    }
}

fn window_of(object: &serde_json::Map<String, Value>) -> Option<Window> {
    let total = first_number(
        object,
        &[
            "total_quota",
            "totalQuota",
            "total_amount",
            "totalAmount",
            "total",
            "limit",
            "capacity",
            "quota",
        ],
    );
    let used = first_number(
        object,
        &[
            "used_quota",
            "usedQuota",
            "used",
            "used_amount",
            "usedAmount",
            "consumed",
            "consume",
        ],
    );
    let remain = first_number(
        object,
        &[
            "remain_quota",
            "remainQuota",
            "remaining_quota",
            "remainingQuota",
            "remain",
            "remaining",
            "left",
            "available",
        ],
    );
    let reported = first_number(
        object,
        &[
            "total_used_percent",
            "used_percent",
            "usedPercent",
            "usage_percent",
        ],
    );
    let (used, total, has_capacity, percent) = match (used, remain, total, reported) {
        (Some(used), _, Some(total), _) if total > 0.0 => {
            (used, total, true, (used / total * 100.0).clamp(0.0, 100.0))
        }
        (_, Some(remain), Some(total), _) if total > 0.0 => {
            let used = (total - remain).max(0.0);
            (used, total, true, (used / total * 100.0).clamp(0.0, 100.0))
        }
        (_, _, _, Some(percent)) => (0.0, 0.0, false, percent.clamp(0.0, 100.0)),
        _ => return None,
    };
    let (period, duration) = window_period(object);
    let reset = first_instant(
        object,
        &[
            "reset_at",
            "resetAt",
            "reset_time",
            "resetTime",
            "refresh_time",
            "refreshTime",
            "expire_at",
            "expireAt",
            "expire_time",
            "expireTime",
            "end_time",
            "endTime",
        ],
    );
    Some(Window {
        used,
        total,
        has_capacity,
        percent,
        period,
        reset,
        duration,
    })
}

/// `window_type` is the quota window. 1, 2 and 3 are the 5-hour, 7-day and
/// monthly windows the client shows.
fn window_period(object: &serde_json::Map<String, Value>) -> (Option<String>, Option<f64>) {
    if let Some(text) = first_text(
        object,
        &["period", "window", "quota_window", "cycle", "cycle_type"],
    ) {
        if let Some(seconds) = period_seconds(&text) {
            return (Some(text), Some(seconds));
        }
    }
    let kind = first_number(object, &["window_type", "windowType"]).map(|n| n as i64);
    let label = match kind {
        Some(1) => Some("5h"),
        Some(2) => Some("7d"),
        Some(3) => Some("1m"),
        _ => None,
    };
    (label.map(str::to_owned), label.and_then(period_seconds))
}

fn period_seconds(text: &str) -> Option<f64> {
    let key: String = text
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != '-')
        .flat_map(|c| c.to_lowercase())
        .collect();
    if matches!(
        key.as_str(),
        "5h" | "5hour" | "5hours" | "session" | "5小时"
    ) {
        Some(5.0 * 3600.0)
    } else if matches!(
        key.as_str(),
        "7d" | "7day" | "7days" | "week" | "weekly" | "7天"
    ) {
        Some(7.0 * 86400.0)
    } else if matches!(
        key.as_str(),
        "1m" | "30d" | "month" | "monthly" | "每月" | "1个月"
    ) {
        Some(30.0 * 86400.0)
    } else {
        None
    }
}

fn is_gift(text: &str) -> bool {
    let key = text.to_ascii_lowercase();
    key.contains("gift")
        || key.contains("reward")
        || key.contains("activity")
        || key.contains("incentive")
        || text.contains("赠送")
        || text.contains("活动")
}

fn plan_name(summary: &Value) -> Option<String> {
    let mut found = None;
    find_plan(summary, &mut found);
    found
}

fn find_plan(value: &Value, found: &mut Option<String>) {
    if found.is_some() {
        return;
    }
    let Some(object) = value.as_object() else {
        if let Some(items) = value.as_array() {
            for item in items {
                find_plan(item, found);
            }
        }
        return;
    };
    if let Some(name) = first_text(
        object,
        &[
            "plan_display_name",
            "plan_name",
            "short_name",
            "package_name",
            "sku_name",
            "display_name",
        ],
    ) {
        if !name.chars().all(|c| c.is_ascii_digit()) {
            *found = Some(name);
            return;
        }
    }
    for child in object.values() {
        find_plan(child, found);
    }
}

fn payload(body: &Value) -> Option<&Value> {
    let code = body.get("code");
    let ok = match code {
        Some(Value::Number(n)) => n.as_i64() == Some(0) || n.as_i64() == Some(200),
        Some(Value::String(text)) => text == "0" || text == "200",
        Some(Value::Null) | None => body.get("success") == Some(&Value::Bool(true)),
        _ => false,
    } || body.get("success") == Some(&Value::Bool(true));
    ok.then(|| body.get("data")).flatten()
}

fn first_number(object: &serde_json::Map<String, Value>, names: &[&str]) -> Option<f64> {
    names
        .iter()
        .find_map(|name| object.get(*name).and_then(number))
}

fn first_instant(object: &serde_json::Map<String, Value>, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        let value = object.get(*name)?;
        value
            .as_str()
            .and_then(normalize_time)
            .or_else(|| value.as_i64().and_then(instant))
            .or_else(|| value.as_f64().and_then(|n| instant(n as i64)))
    })
}

fn first_text(object: &serde_json::Map<String, Value>, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        object
            .get(*name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    })
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
        .filter(|n| n.is_finite() && *n >= 0.0)
}

fn normalize_time(text: &str) -> Option<String> {
    if let Ok(time) = DateTime::parse_from_rfc3339(text) {
        return Some(time.with_timezone(&Utc).to_rfc3339());
    }
    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(time) = chrono::NaiveDateTime::parse_from_str(text, format) {
            if let Some(local) = time.and_local_timezone(chrono::Local).single() {
                return Some(local.to_rfc3339());
            }
        }
    }
    chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .and_then(|time| time.and_local_timezone(chrono::Local).single())
        .map(|time| time.to_rfc3339())
        .or_else(|| text.parse::<i64>().ok().and_then(instant))
}

fn instant(raw: i64) -> Option<String> {
    let millis = if raw.abs() < 10_000_000_000 {
        raw.saturating_mul(1000)
    } else {
        raw
    };
    DateTime::from_timestamp_millis(millis).map(|time| time.to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    fn host() -> FakeHost {
        let mut host = FakeHost::new("/u");
        host.cookies.insert(
            "Doubao|doubao.com|sessionid".into(),
            Answer::Found("sid".into()),
        );
        host
    }

    #[test]
    fn the_subscription_and_the_gift_are_separate_quotas() {
        let mut host = host();
        host.web.insert(
            endpoint(&host, SUMMARY),
            Ok(json!({
                "code": 0,
                "data": {
                    "plan_name": "标准套餐",
                    "windows": [
                        {"period": "7d", "used_quota": 400, "total_quota": 2100, "reset_time": "2026-11-16T00:00:00Z"},
                        {"period": "5h", "used_quota": 10, "total_quota": 100, "reset_time": "2026-11-12T20:00:00Z"}
                    ]
                }
            })),
        );
        host.web.insert(
            endpoint(&host, REWARD),
            Ok(json!({
                "code": 0,
                "data": {"remain_quota": 800, "total_quota": 8800, "expire_time": "2026-12-01T00:00:00Z"}
            })),
        );
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 3);
        assert_eq!(reading.quotas[0]["name"], "Personal subscription · 7d");
        assert_eq!(reading.quotas[0]["shortLabel"], "7d");
        assert!((reading.quotas[0]["usedPercent"].as_f64().unwrap() - 400.0 / 21.0).abs() < 0.01);
        assert_eq!(reading.quotas[0]["windowDurationSeconds"], 7.0 * 86400.0);
        assert_eq!(reading.quotas[1]["shortLabel"], "5h");
        assert_eq!(reading.quotas[2]["name"], "Activity gift");
        assert_eq!(reading.quotas[2]["shortLabel"], "Gift");
        assert_eq!(reading.quotas[2]["nativeUsage"]["used"], 8000.0);
        assert_eq!(reading.quotas[2]["nativeUsage"]["capacity"], 8800.0);
        assert_eq!(reading.account.unwrap()["plan"], "标准套餐");
        let sent = host.sent.borrow();
        assert_eq!(sent[0].0[0].0, "Cookie");
        assert!(sent[0].0[0].1.contains("sessionid=sid"));
        let asked = host.asked.borrow();
        assert!(asked[0].0.contains("aid=582478"));
        assert!(asked[0].0.contains("version_code=20800"));
        assert!(asked[1].0.contains("activity_reward_quota"));
    }

    #[test]
    fn an_expired_sign_in_is_not_called_an_unknown_format() {
        let mut host = host();
        let body = json!({
            "code": 710012001,
            "msg": "登录已过期，请重新登录",
            "data": null
        });
        host.web.insert(endpoint(&host, SUMMARY), Ok(body.clone()));
        host.web.insert(endpoint(&host, REWARD), Ok(body));
        let Err(unread) = read(&host) else {
            panic!("an expired sign-in should not read as a quota");
        };
        assert_eq!(unread.kind(), "signedOut");
    }

    #[test]
    fn string_amounts_and_period_keys_are_read() {
        let summary = json!({
            "code": "0",
            "data": {
                "sku_name": "豆包标准版",
                "5h": {"used_quota": "10", "total_quota": "100"},
                "7d": {"usedQuota": "400", "totalQuota": "2100"}
            }
        });
        let reward = json!({
            "code": "0",
            "data": {"remain_quota": "800", "total_quota": "8800"}
        });
        let reading = parse(&summary, &reward).unwrap();
        assert_eq!(reading.quotas.len(), 3);
        assert_eq!(reading.quotas[0]["shortLabel"], "5h");
        assert_eq!(reading.quotas[1]["shortLabel"], "7d");
        assert_eq!(reading.quotas[2]["name"], "Activity gift");
        assert_eq!(reading.account.unwrap()["plan"], "豆包标准版");
    }

    #[test]
    fn the_live_subscription_window_and_gift_grants_are_two_quotas() {
        let summary = json!({
            "code": 0,
            "data": {
                "current_subscription": {
                    "display": {"product_name": "个人订阅", "short_name": "标准套餐"},
                    "sku_key": "doubao_personal_std"
                },
                "window_limit_section": {
                    "window_limit_groups": [{
                        "feature_group": "general",
                        "feature_group_name": "",
                        "window_limits": [{
                            "window_type": 2,
                            "used_percent": 36,
                            "end_time": 1_794_468_193_000_i64
                        }]
                    }]
                }
            }
        });
        let reward = json!({
            "code": 0,
            "data": {
                "entitlements": [
                    {"display": {"title": "活动赠送额度"}, "total_amount": "200", "used_amount": "200", "total_used_percent": 100},
                    {"display": {"title": "活动赠送额度"}, "total_amount": "200", "used_amount": "177", "total_used_percent": 88, "end_time": 1_796_000_000_000_i64},
                    {"display": {"title": "活动赠送额度"}, "total_amount": "200", "used_amount": "0", "total_used_percent": 0},
                    {"display": {"title": "活动赠送额度"}, "total_amount": "200", "used_amount": "0", "total_used_percent": 0}
                ]
            }
        });
        let reading = parse(&summary, &reward).unwrap();
        assert_eq!(reading.quotas.len(), 2);
        assert_eq!(reading.quotas[0]["name"], "Personal subscription");
        assert_eq!(reading.quotas[0]["usedPercent"], 36.0);
        assert!(reading.quotas[0].get("nativeUsage").is_none());
        assert_eq!(reading.quotas[1]["name"], "Activity gift");
        assert_eq!(reading.quotas[1]["nativeUsage"]["used"], 377.0);
        assert_eq!(reading.quotas[1]["nativeUsage"]["capacity"], 800.0);
        assert!((reading.quotas[1]["usedPercent"].as_f64().unwrap() - 47.125).abs() < 0.01);
        assert_eq!(reading.account.unwrap()["plan"], "标准套餐");
    }

    #[test]
    fn the_usage_timeline_is_kept_with_the_account() {
        let summary = json!({
            "code": 0,
            "data": {
                "current_subscription": {"display": {"short_name": "标准套餐"}},
                "window_limit_section": {"window_limit_groups": [{
                    "window_limits": [{"window_type": 2, "used_percent": 0}]
                }]}
            }
        });
        let reward = json!({"code": 0, "data": {"entitlements": []}});
        let timeline = json!({
            "code": 0,
            "data": {
                "records": [
                    {"create_time": "2026-11-10T05:49:00Z", "scene_name": "飞书知识问答", "quota_source": "个人订阅", "used_percent": 0.11},
                    {"create_time": "2026-11-09T10:41:00Z", "scene_name": "豆包工作伙伴", "quota_source": "个人订阅", "model_name": "Auto", "used_percent": 0.02}
                ],
                "has_more": false
            }
        });
        let mut host = host();
        host.web.insert(endpoint(&host, SUMMARY), Ok(summary));
        host.web.insert(endpoint(&host, REWARD), Ok(reward));
        host.web.insert(endpoint(&host, TIMELINE), Ok(timeline));
        let reading = read(&host).unwrap();
        let records = &reading.account.unwrap()["usageDetail"]["records"];
        assert_eq!(records.as_array().unwrap().len(), 2);
        assert_eq!(records[0]["title"], "飞书知识问答");
        assert_eq!(records[0]["source"], "个人订阅");
        assert_eq!(records[0]["percent"], 0.11);
        assert_eq!(records[1]["model"], "Auto");
    }

    #[test]
    fn the_live_timeline_entries_keep_scene_source_and_share() {
        let records = usage_records(&json!({
            "entries": [{
                "entry_type": 1,
                "usage": {
                    "display_name": "飞书知识问答",
                    "item_id": "not-a-title",
                    "model_display_name": "Auto",
                    "occurred_at_ms": 1_762_760_940_000_i64,
                    "quota_source": {
                        "display_name": "个人订阅",
                        "display_text": "0.11%",
                        "quota_source_code": 1
                    }
                }
            }, {
                "entry_type": 2,
                "usage": {
                    "display_name": "7天重置",
                    "occurred_at_ms": 1_762_760_500_000_i64
                }
            }, {
                "entry_type": 1,
                "usage": {
                    "display_name": "智能纪要",
                    "model_display_name": "—",
                    "occurred_at_ms": 1_762_760_000_000_i64,
                    "quota_source": {
                        "display_name": {"text": "活动赠送额度"},
                        "display_text": "<0.01%"
                    }
                }
            }]
        }));
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["title"], "飞书知识问答");
        assert_eq!(records[0]["source"], "个人订阅");
        assert_eq!(records[0]["model"], "Auto");
        assert_eq!(records[0]["percent"], 0.11);
        assert!(records[0].get("item_id").is_none());
        assert!(records.iter().all(|row| row["title"] != "7天重置"));
        assert_eq!(records[1]["title"], "智能纪要");
        assert_eq!(records[1]["source"], "活动赠送额度");
        assert!(records[1]["model"].is_null());
        assert!(records[1]["percent"].as_f64().unwrap() < 0.01);
    }

    #[test]
    fn a_clock_time_without_a_zone_is_a_usage_row() {
        let records = usage_records(&json!({
            "list": [{
                "create_time": "2026-11-10 13:49:05",
                "scene_name": "豆包工作伙伴",
                "used_percent": "0.11"
            }]
        }));
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["title"], "豆包工作伙伴");
        assert_eq!(records[0]["percent"], 0.11);
    }

    #[test]
    fn without_a_sign_in_there_is_nothing_to_say() {
        let reading = read(&FakeHost::new("/u")).unwrap();
        assert!(reading.account.is_none() && reading.quotas.is_empty());
    }
}
