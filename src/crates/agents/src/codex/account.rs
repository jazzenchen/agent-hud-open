//! Codex's quota: its CLI's own `app-server` (account/rateLimits/read and
//! account/read), and Pi's ChatGPT sign-in, which spends the same plan.
use crate::account::{
    account_id, client_home_key, legacy_account, legacy_unresolved, read_json, status, variable,
    EngineStart, Host, HttpFailure, HttpRequest, Provider, Reading, StatusPage, Unread,
};
use crate::l10n;
use crate::naming::{vendor_windows, window_names};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::Path,
    time::{Duration, Instant},
};

/// Where Agent HUD remembers which email signed in to which workspace.
const IDENTITIES: &str = "codex-identities.json";

/// Codex's plan, read through its CLI's app-server and Pi's ChatGPT
/// sign-in. Pi and other clients spend it without Codex rollouts here, so it
/// is read on the account interval whatever this computer does.
pub static PROVIDER: Provider = Provider {
    local_work: false,
    status: Some(&STATUS),
    ..Provider::new("codex", read)
};
/// OpenAI's status page names Codex by its desktop home.
static STATUS: StatusPage = StatusPage {
    url: "https://status.openai.com/api/v2/summary.json",
    components: &["Codex in ChatGPT Desktop", "Responses", "Login"],
};

pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let executable = host.executable("codex");
    let native = match executable.as_deref() {
        Some(executable) => read_native(host, executable),
        None => Err(Unread::Failed(l10n::text(
            "安装并登录后即可读取额度",
            "Install and sign in to read quota",
        ))),
    };
    let pi = read_pi(host);
    match (native, pi) {
        (Ok(mut native), Ok(Some(pi))) => {
            merge(&mut native, pi);
            Ok(native)
        }
        (Ok(mut native), Err(error)) => {
            append_notice(&mut native, error.reason().to_owned());
            Ok(native)
        }
        (Ok(native), _) => Ok(native),
        (Err(error), Ok(Some(mut pi))) => {
            // Pi can be the only client installed. A configured Codex home
            // still needs its CLI, so a missing executable matters there.
            if executable.is_some() || !host.account_homes("codex").is_empty() {
                append_notice(&mut pi, error.reason().to_owned());
            }
            Ok(pi)
        }
        (Err(native), Err(pi)) => Err(Unread::combined(vec![native, pi]).expect("two failures")),
        (Err(error), _) => Err(error),
    }
}

fn append_notice(reading: &mut Reading, notice: String) {
    match &mut reading.notice {
        Some(current) => {
            current.push('\n');
            current.push_str(&notice);
        }
        None => reading.notice = Some(notice),
    }
}

fn merge(reading: &mut Reading, other: Reading) {
    for quota in other.quotas {
        if !reading.quotas.iter().any(|row| row["id"] == quota["id"]) {
            reading.quotas.push(quota);
        }
    }
    for account in other.account.into_iter().chain(other.additional_accounts) {
        if reading
            .account
            .iter()
            .chain(&reading.additional_accounts)
            .any(|row| row["id"] == account["id"])
        {
            continue;
        }
        if reading.account.is_none() {
            reading.account = Some(account);
        } else {
            reading.additional_accounts.push(account);
        }
    }
    if let Some(notice) = other.notice {
        append_notice(reading, notice);
    }
}

fn read_native(host: &dyn Host, executable: &Path) -> Result<Reading, Unread> {
    let home = variable(host, "CODEX_HOME").unwrap_or_else(|| host.home().join(".codex"));
    let mut seen = HashSet::new();
    let homes: Vec<_> = std::iter::once(home)
        .chain(host.account_homes("codex"))
        .map(|home| host.canonical(&home).unwrap_or(home))
        .filter(|home| seen.insert(home.clone()))
        .collect();
    let mut reading = Reading::empty("codex");
    let mut succeeded = false;
    let mut failures = Vec::new();
    for home in &homes {
        if host.cancelled() {
            return Err(l10n::text("账户读取已停止", "Account reading was stopped").into());
        }
        let prefix = if homes.len() > 1 {
            format!("{}: ", home.display())
        } else {
            String::new()
        };
        let notice = match read_home(host, executable, home) {
            Ok(mut account) => {
                succeeded = true;
                let notice = account.notice.take();
                merge(&mut reading, account);
                notice
            }
            Err(error) => {
                let error = error.prefixed(&prefix);
                let notice = error.reason().to_owned();
                failures.push(error);
                append_notice(&mut reading, notice);
                None
            }
        };
        if let Some(notice) = notice {
            append_notice(&mut reading, format!("{prefix}{notice}"));
        }
    }
    if succeeded {
        Ok(reading)
    } else {
        Err(Unread::combined(failures).unwrap_or_else(|| Unread::Failed(String::new())))
    }
}

fn read_home(host: &dyn Host, executable: &Path, home: &Path) -> Result<Reading, Unread> {
    let mut process = host.start_engine(&EngineStart {
        executable,
        args: &["app-server", "--listen", "stdio://"],
        folder: "codex",
        env: &[("CODEX_HOME", home.to_string_lossy().into_owned())],
    })?;
    process.send(&json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"agent_hud","title":"Agent HUD","version":"0.1.0"}}}))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut grace = deadline;
    let mut limits = None;
    let mut refused = None::<String>;
    let mut account = Value::Null;
    let mut requires_openai_auth = None;
    let mut answered = false;
    while let Some(message) = process.next(deadline.min(grace)) {
        let Some(id) = message["id"].as_u64() else {
            continue;
        };
        if message["error"].is_object() {
            // An engine without account/read still reports limits, only
            // without the signed-in email.
            if id == 3 {
                answered = true;
            } else if id == 2 {
                // Limits need a ChatGPT sign-in; account/read says why
                // there is none.
                refused = Some(
                    message["error"]["message"]
                        .as_str()
                        .unwrap_or("app-server error")
                        .to_owned(),
                );
                grace = Instant::now() + Duration::from_secs(2);
            } else {
                return Err(format!(
                    "Codex: {}",
                    message["error"]["message"]
                        .as_str()
                        .unwrap_or("app-server error")
                )
                .into());
            }
        } else if let Some(result) = message.get("result") {
            match id {
                1 => {
                    process.send(&json!({"method":"initialized"}))?;
                    process.send(&json!({"id":2,"method":"account/rateLimits/read"}))?;
                    process.send(&json!({"id":3,"method":"account/read","params":{}}))?;
                }
                2 => {
                    limits = Some(result.clone());
                    grace = Instant::now() + Duration::from_secs(2);
                }
                3 => {
                    account = result["account"].clone();
                    requires_openai_auth = result["requiresOpenaiAuth"].as_bool();
                    answered = true;
                }
                _ => {}
            }
        }
        if (limits.is_some() || refused.is_some()) && answered {
            break;
        }
    }
    if let (None, Some(error)) = (&limits, refused) {
        // No sign-in at all is Codex's own state: the last windows stay.
        if requires_openai_auth == Some(true) && account.is_null() {
            return Err(Unread::SignedOut(l10n::text(
                "Codex 尚未登录，运行 codex login 登录 ChatGPT 后即可读取额度",
                "Codex is not signed in. Run codex login with ChatGPT to read its quota",
            )));
        }
        return match no_plan_notice(&account, requires_openai_auth) {
            Some(notice) => {
                let mut reading = Reading::empty("codex");
                reading.notice = Some(notice);
                Ok(reading)
            }
            None => Err(format!("Codex: {error}").into()),
        };
    }
    let limits = limits.ok_or_else(|| {
        if process.ended() {
            Unread::Failed(l10n::text(
                "Codex 引擎提前退出，请检查本机登录状态",
                "Codex exited before reporting limits; check local sign-in",
            ))
        } else {
            Unread::transient(l10n::text(
                "Codex 额度查询超时",
                "Codex quota query timed out",
            ))
        }
    })?;
    parse(
        host,
        &limits,
        &account,
        &home.to_string_lossy(),
        &client_home_key(host, home, &host.home().join(".codex")),
        true,
    )
    .map_err(Unread::from)
}

/// Why a Codex home has no ChatGPT plan limits, as account/read tells it: an
/// API key, Amazon Bedrock, a model provider that needs no OpenAI sign-in, or
/// no sign-in at all. None when it does not say.
fn no_plan_notice(account: &Value, requires_openai_auth: Option<bool>) -> Option<String> {
    match account["type"].as_str() {
        Some("apiKey") => Some(l10n::text(
            "Codex 用的是 API key 登录，没有 ChatGPT 套餐额度",
            "Codex is signed in with an API key, which has no ChatGPT plan limits",
        )),
        Some("amazonBedrock") => Some(l10n::text(
            "Codex 走的是 Amazon Bedrock，没有 ChatGPT 套餐额度",
            "Codex runs on Amazon Bedrock, which has no ChatGPT plan limits",
        )),
        Some(_) => None,
        None => match requires_openai_auth {
            Some(false) => Some(l10n::text(
                "Codex 用的是自定义模型服务，没有 ChatGPT 套餐额度",
                "Codex uses a custom model provider, which has no ChatGPT plan limits",
            )),
            _ => None,
        },
    }
}

fn pi_signed_out() -> Unread {
    Unread::SignedOut(l10n::text(
        "请在 Pi 中重新登录 ChatGPT",
        "Sign in to ChatGPT again in Pi",
    ))
}

fn read_pi(host: &dyn Host) -> Result<Option<Reading>, Unread> {
    if host.cancelled() {
        return Ok(None);
    }
    let home =
        variable(host, "PI_CODING_AGENT_DIR").unwrap_or_else(|| host.home().join(".pi/agent"));
    let Some(auth) = read_json(host, &home.join("auth.json")) else {
        return Ok(None);
    };
    let auth = &auth["openai-codex"];
    if !auth.is_object() {
        return Ok(None);
    }
    let token = auth["access"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(pi_signed_out)?;
    let account = auth["accountId"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(pi_signed_out)?;
    if auth["type"] != "oauth"
        || auth["expires"]
            .as_f64()
            .is_none_or(|value| value / 1000.0 <= chrono::Utc::now().timestamp() as f64)
    {
        return Err(pi_signed_out());
    }
    let payload = host
        .http_json(&HttpRequest {
            url: "https://chatgpt.com/backend-api/wham/usage",
            secrets: &[("Authorization", &format!("Bearer {token}"))],
            headers: &[("ChatGPT-Account-Id", account)],
            body: None,
            timeout: Duration::from_secs(12),
        })
        .map_err(|failure| match failure {
            HttpFailure::Unreachable | HttpFailure::BadHeader => failure.unread_as(l10n::text(
                "Pi 的 ChatGPT 用量请求失败",
                "Pi ChatGPT usage request failed",
            )),
            HttpFailure::Status(401) => pi_signed_out(),
            other => other.unread(),
        })?;
    if payload["account_id"] != account
        || payload["email"]
            .as_str()
            .is_none_or(|value| value.is_empty())
    {
        return Err(l10n::text(
            "Pi 的 ChatGPT 用量返回了其他或未知的账户",
            "Pi ChatGPT usage returned a different or unknown account",
        )
        .into());
    }
    let window = |value: &Value| json!({"usedPercent":value["used_percent"],"windowDurationMins":value["limit_window_seconds"].as_i64().map(|seconds|seconds/60),"resetsAt":value["reset_at"]});
    let bucket = |value: &Value, id: &str, name: Option<&str>| json!({"limitId":id,"limitName":name,"planType":payload["plan_type"],"primary":window(&value["primary_window"]),"secondary":window(&value["secondary_window"])});
    let mut buckets = serde_json::Map::new();
    if payload["rate_limit"].is_object() {
        buckets.insert(
            "codex".into(),
            bucket(&payload["rate_limit"], "codex", None),
        );
    }
    for additional in payload["additional_rate_limits"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(id) = additional["metered_feature"].as_str() {
            buckets.insert(
                id.into(),
                bucket(
                    &additional["rate_limit"],
                    id,
                    additional["limit_name"].as_str(),
                ),
            );
        }
    }
    Ok(Some(parse(
        host,
        &json!({"accountId":account,"rateLimitsByLimitId":buckets,"rateLimitResetCredits":payload["rate_limit_reset_credits"]}),
        &json!({"type":"chatgpt","email":payload["email"],"planType":payload["plan_type"]}),
        &home.to_string_lossy(),
        &format!(
            "pi:{}",
            client_home_key(host, &home, &host.home().join(".pi/agent"))
        ),
        false,
    )?))
}
/// The workspace and email of a sign-in, completed from what Agent HUD
/// remembers when Codex reports only one of them.
fn remember_identity(
    host: &dyn Host,
    home: &str,
    workspace: &str,
    email: &str,
) -> (String, String) {
    const LIMIT: u64 = 16 * 1024 * 1024;
    let mut map = host
        .read_own(IDENTITIES, LIMIT)
        .found()
        .filter(|bytes| bytes.len() as u64 <= LIMIT)
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    let mut ws = workspace.trim().to_owned();
    let mut em = email.trim().to_owned();
    let prefix = format!("{home}/");
    if !ws.is_empty() && !em.is_empty() {
        let key = format!("{prefix}{ws}");
        if map.get(&key).and_then(Value::as_str) != Some(em.as_str()) {
            map.insert(key, json!(em));
            if let Ok(bytes) = serde_json::to_vec(&Value::Object(map)) {
                let _ = host.write_own(IDENTITIES, &bytes);
            }
        }
    } else if !ws.is_empty() && em.is_empty() {
        if let Some(saved) = map
            .get(&format!("{prefix}{ws}"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            em = saved.to_owned();
        }
    } else if ws.is_empty() && !em.is_empty() {
        let matches = map
            .iter()
            .filter_map(|(k, v)| {
                let rest = k.strip_prefix(&prefix)?;
                v.as_str()
                    .filter(|s| s.eq_ignore_ascii_case(&em))
                    .map(|_| rest.to_owned())
            })
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            ws = matches[0].clone();
        }
    }
    (ws, em)
}
fn parse_reset_credits(limits: &Value) -> Option<Value> {
    let raw = limits
        .get("rateLimitResetCredits")
        .or_else(|| limits.get("rate_limit_reset_credits"))?;
    let count = raw
        .get("availableCount")
        .or_else(|| raw.get("available_count"))
        .and_then(Value::as_u64)?;
    let credits = raw.get("credits").and_then(Value::as_array).map(|items| {
        let mut parsed = items
            .iter()
            .filter_map(|item| {
                let id = item.get("id")?.as_str()?.to_owned();
                let raw_exp = item.get("expiresAt").or_else(|| item.get("expires_at"));
                let (sort_ts, exp_iso) =
                    if let Some(secs) = raw_exp.and_then(Value::as_f64).filter(|n| n.is_finite()) {
                        let iso = chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0)
                            .map(|d| d.to_rfc3339());
                        (Some((secs * 1000.0) as i64), iso)
                    } else if let Some(s) = raw_exp.and_then(Value::as_str) {
                        let ts = chrono::DateTime::parse_from_rfc3339(s)
                            .ok()
                            .map(|d| d.timestamp_millis());
                        (ts, Some(s.to_owned()))
                    } else {
                        (None, None)
                    };
                Some((
                    sort_ts.unwrap_or(i64::MAX),
                    id.clone(),
                    json!({"id": id, "expiresAt": exp_iso}),
                ))
            })
            .collect::<Vec<_>>();
        parsed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        parsed.into_iter().map(|(_, _, v)| v).collect::<Vec<_>>()
    });
    let mut out = json!({"availableCount": count});
    if let Some(credits) = credits {
        out["credits"] = json!(credits);
    }
    Some(out)
}
fn parse(
    host: &dyn Host,
    limits: &Value,
    account: &Value,
    home: &str,
    home_key: &str,
    native: bool,
) -> Result<Reading, String> {
    let mut reading = Reading::empty("codex");
    let raw_workspace = limits["accountId"].as_str().unwrap_or_default();
    let raw_email = account["email"].as_str().unwrap_or_default();
    let (workspace_str, email_str) = remember_identity(host, home, raw_workspace, raw_email);
    let workspace = workspace_str.as_str();
    let email = email_str.as_str();
    let identity = if workspace.is_empty() && email.is_empty() {
        account_id("codex", &["unresolved", home])
    } else {
        account_id("codex", &[&email.to_lowercase(), workspace])
    };
    let legacy = legacy_account("Codex", &email.to_lowercase(), workspace);
    let account_identity = legacy.unwrap_or_else(|| legacy_unresolved("Codex", home_key));
    // A present multi-bucket map is authoritative, an empty one included;
    // only without it does the single legacy bucket count (agent-hud-native-open
    // CodexRateLimits.buckets).
    let mut buckets = match limits["rateLimitsByLimitId"].as_object() {
        Some(map) => map
            .iter()
            .map(|(id, value)| (id.clone(), value))
            .collect::<Vec<_>>(),
        None => limits["rateLimits"]
            .is_object()
            .then(|| &limits["rateLimits"])
            .map(|value| {
                (
                    value["limitId"].as_str().unwrap_or("codex").to_owned(),
                    value,
                )
            })
            .into_iter()
            .collect(),
    };
    buckets.sort_by(|a, b| match (a.0.as_str(), b.0.as_str()) {
        ("codex", "codex") => std::cmp::Ordering::Equal,
        ("codex", _) => std::cmp::Ordering::Less,
        (_, "codex") => std::cmp::Ordering::Greater,
        _ => a.0.cmp(&b.0),
    });
    // The plan the first bucket names, else the signed-in account's.
    let plan = buckets
        .iter()
        .find_map(|(_, bucket)| bucket["planType"].as_str())
        .or_else(|| account["planType"].as_str());
    if account.is_object() || !workspace.is_empty() || !email.is_empty() {
        let mut acct = json!({"id":account_identity,"provider":"codex","label":if email.is_empty(){None}else{Some(email)},"plan":plan});
        if let Some(credits) = parse_reset_credits(limits) {
            acct["resetCredits"] = credits;
        }
        reading.account = Some(acct);
    }
    for (bucket_id, bucket) in buckets {
        // Named as Codex's own client names them; another bucket than `codex`
        // with its name first. Short names give the `codex` bucket's windows
        // their period alone and another bucket's its word, with the period
        // when the bucket has two windows (Spark 5h, Spark 7d, Reserve).
        let service = bucket["limitName"].as_str().unwrap_or(&bucket_id);
        let windows = ["primary", "secondary"]
            .iter()
            .filter(|slot| {
                bucket[**slot]["usedPercent"]
                    .as_f64()
                    .is_some_and(f64::is_finite)
            })
            .count();
        for slot in ["primary", "secondary"] {
            let window = &bucket[slot];
            let Some(percent) = window["usedPercent"]
                .as_f64()
                .filter(|percent| percent.is_finite())
            else {
                continue;
            };
            let secondary = slot == "secondary";
            let period = window_names::Period::from_seconds(
                window["windowDurationMins"]
                    .as_f64()
                    .map(|minutes| minutes * 60.0),
            );
            let limit = vendor_windows::codex_limit_name(period, secondary);
            let alone = period.map_or_else(
                || if secondary { "Secondary" } else { "Usage" }.to_owned(),
                |period| period.short_name(),
            );
            let (name, short) = if bucket_id == "codex" {
                (limit, alone)
            } else {
                let word = window_names::catalog_window_word("codex", service);
                let short = if windows > 1 {
                    format!(
                        "{word} {}",
                        period.map_or(alone, |period| period.after_word())
                    )
                } else {
                    word
                };
                (
                    format!(
                        "{} · {limit}",
                        window_names::catalog_window("codex", service)
                    ),
                    short,
                )
            };
            let reset = window["resetsAt"]
                .as_f64()
                .filter(|value| value.is_finite())
                .and_then(|value| chrono::DateTime::<chrono::Utc>::from_timestamp(value as i64, 0))
                .map(|date| date.to_rfc3339());
            let key = if bucket_id == "codex" && slot == "primary" {
                "codex".into()
            } else {
                format!("codex:{bucket_id}:{slot}")
            };
            reading.quotas.push(json!({"id":format!("{identity}:{key}"),"vendor":"codex","name":name,"shortLabel":short,"usedPercent":percent,"resetAt":reset,"status":status(percent),"account":if email.is_empty(){None}else{Some(email)},"accountId":account_identity,"legacyWindowId":format!("{account_identity}/{key}"),"consumerSource":if native{Some("codex")}else{None},"allModels":bucket_id=="codex","windowDurationSeconds":window["windowDurationMins"].as_f64().filter(|value|*value>0.0).map(|value|value*60.0)}));
        }
    }
    if reading.quotas.is_empty() {
        reading.notice = Some(l10n::text(
            "Codex 当前登录没有返回额度窗口",
            "Codex returned no quota windows for its current login",
        ));
    }
    Ok(reading)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    fn window(used: f64, minutes: u64) -> Value {
        json!({"usedPercent": used, "windowDurationMins": minutes, "resetsAt": 1_792_000_000})
    }

    #[test]
    fn a_present_bucket_map_is_authoritative_even_when_empty() {
        let host = FakeHost::new("/h");
        let legacy = json!({"limitId": "codex", "primary": window(10.0, 300), "planType": "plus"});
        let empty = json!({"accountId": "ws", "rateLimitsByLimitId": {}, "rateLimits": legacy});
        let account = json!({"email": "a@example.com", "planType": "plus"});
        let reading = parse(&host, &empty, &account, "/h", "", true).unwrap();
        assert!(reading.quotas.is_empty(), "the empty map leaves no window");
        let absent = json!({"accountId": "ws", "rateLimits": legacy});
        let reading = parse(&host, &absent, &account, "/h", "", true).unwrap();
        assert_eq!(
            reading.quotas.len(),
            1,
            "without the map the legacy bucket counts"
        );
    }

    #[test]
    fn the_plan_is_the_first_buckets_then_the_accounts() {
        let host = FakeHost::new("/h");
        let limits = json!({"accountId": "ws", "rateLimitsByLimitId": {
            "gpt-reserve": {"primary": window(1.0, 300), "planType": "team"},
            "codex": {"primary": window(10.0, 300), "secondary": window(20.0, 10080), "planType": "pro"}
        }});
        let account = json!({"email": "a@example.com", "planType": "plus"});
        let reading = parse(&host, &limits, &account, "/h", "", true).unwrap();
        assert_eq!(
            reading.account.unwrap()["plan"],
            "pro",
            "the codex bucket sorts first"
        );
        let unnamed = json!({"accountId": "ws", "rateLimitsByLimitId": {"codex": {"primary": window(10.0, 300)}}});
        let reading = parse(&host, &unnamed, &account, "/h", "", true).unwrap();
        assert_eq!(reading.account.unwrap()["plan"], "plus");
    }

    fn installed(limits: Value, account: Value) -> FakeHost {
        let mut host = FakeHost::new("/u");
        host.executables.insert("codex".into(), "/bin/codex".into());
        host.engine_replies
            .insert(1, json!({"id": 1, "result": {}}));
        host.engine_replies
            .insert(2, json!({"id": 2, "result": limits}));
        host.engine_replies
            .insert(3, json!({"id": 3, "result": {"account": account}}));
        host
    }

    fn limits() -> Value {
        json!({"accountId": "ws", "rateLimitsByLimitId": {
            "codex": {"primary": window(10.0, 300), "secondary": window(40.0, 10080), "planType": "pro"}
        }})
    }

    fn additional(host: &mut FakeHost, home: &Path, limits: Value, account: Value) {
        host.account_homes
            .entry("codex".into())
            .or_default()
            .push(home.to_path_buf());
        host.engine_replies_by_home.insert(
            home.to_path_buf(),
            installed(limits, account).engine_replies,
        );
    }

    #[test]
    fn each_account_home_uses_its_own_engine_and_keeps_its_windows() {
        let mut host = installed(limits(), json!({"email": "personal@example.com"}))
            .env("CODEX_HOME", "/accounts/personal");
        let work = host.home().join("codex-work");
        let mut work_limits = limits();
        work_limits["accountId"] = json!("work");
        work_limits["rateLimitsByLimitId"]["codex"]["primary"]["usedPercent"] = json!(75);
        additional(
            &mut host,
            &work,
            work_limits,
            json!({"email": "work@example.com"}),
        );

        let reading = read(&host).unwrap();
        let started = host.started.borrow();
        assert_eq!(started.len(), 2);
        assert_eq!(
            started[0].3,
            [("CODEX_HOME".to_owned(), "/accounts/personal".to_owned())]
        );
        assert_eq!(
            started[1].3,
            [("CODEX_HOME".to_owned(), work.to_string_lossy().into_owned())]
        );
        assert_eq!(reading.account.unwrap()["label"], "personal@example.com");
        assert_eq!(reading.additional_accounts.len(), 1);
        assert_eq!(reading.additional_accounts[0]["label"], "work@example.com");
        assert_eq!(reading.quotas.len(), 4);
        assert_eq!(reading.quotas[0]["usedPercent"], 10.0);
        assert_eq!(reading.quotas[2]["usedPercent"], 75.0);
        assert_ne!(
            reading.quotas[0]["accountId"],
            reading.quotas[2]["accountId"]
        );
        assert!(reading.notice.is_none());
    }

    #[test]
    fn repeated_homes_and_symlink_aliases_are_read_once() {
        let mut host = installed(limits(), json!({"email": "a@example.com"}));
        let default = host.home().join(".codex");
        let alias = host.home().join("codex-alias");
        host.canonical_paths.insert(alias.clone(), default.clone());
        host.account_homes
            .insert("codex".into(), vec![default, alias]);

        let reading = read(&host).unwrap();
        assert_eq!(host.started.borrow().len(), 1);
        assert_eq!(reading.quotas.len(), 2);
    }

    #[test]
    fn the_same_subscription_in_different_homes_is_not_duplicated() {
        let mut host = installed(limits(), json!({"email": "A@example.com"}));
        let work = host.home().join("codex-work");
        additional(
            &mut host,
            &work,
            limits(),
            json!({"email": "a@example.com"}),
        );

        let reading = read(&host).unwrap();
        assert_eq!(host.started.borrow().len(), 2);
        assert_eq!(reading.quotas.len(), 2);
        assert!(reading.additional_accounts.is_empty());
    }

    #[test]
    fn different_workspaces_for_the_same_email_keep_separate_subscriptions() {
        let mut host = installed(limits(), json!({"email": "a@example.com"}));
        let work = host.home().join("codex-work");
        let mut work_limits = limits();
        work_limits["accountId"] = json!("work");
        additional(
            &mut host,
            &work,
            work_limits,
            json!({"email": "a@example.com"}),
        );

        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 4);
        assert_eq!(reading.additional_accounts.len(), 1);
        assert_ne!(
            reading.account.unwrap()["id"],
            reading.additional_accounts[0]["id"]
        );
    }

    #[test]
    fn one_failed_home_does_not_hide_another_account_or_its_error() {
        let mut host = installed(limits(), json!({"email": "a@example.com"}));
        host.engine_replies
            .insert(2, json!({"id": 2, "error": {"message": "Sign-in expired"}}));
        let work = host.home().join("codex-work");
        additional(
            &mut host,
            &work,
            limits(),
            json!({"email": "work@example.com"}),
        );
        let host = pi_signed_in(host);

        let reading = read(&host).unwrap();
        assert_eq!(reading.account.unwrap()["label"], "work@example.com");
        assert_eq!(reading.quotas.len(), 3);
        assert_eq!(reading.additional_accounts.len(), 1);
        assert_eq!(host.asked.borrow().len(), 1);
        let notice = reading.notice.unwrap();
        assert!(notice.contains("Sign-in expired"));
        assert!(notice.contains(&*host.home().join(".codex").to_string_lossy()));
    }

    #[test]
    fn all_failed_homes_return_an_error_with_each_home() {
        let mut host = installed(limits(), Value::Null);
        host.engine_replies
            .insert(2, json!({"id": 2, "error": {"message": "Sign-in expired"}}));
        let work = host.home().join("codex-work");
        host.account_homes
            .insert("codex".into(), vec![work.clone()]);

        let error = read(&host).err().unwrap().reason().to_owned();
        assert_eq!(host.started.borrow().len(), 2);
        assert!(error.contains(&*host.home().join(".codex").to_string_lossy()));
        assert!(error.contains(&*work.to_string_lossy()));
        assert_eq!(error.matches("Sign-in expired").count(), 2);
    }

    #[test]
    fn the_app_server_reports_limits_and_the_account() {
        let host = installed(
            limits(),
            json!({"email": "A@example.com", "planType": "pro"}),
        );
        let reading = read(&host).unwrap();
        let started = host.started.borrow();
        assert_eq!(started[0].0, std::path::PathBuf::from("/bin/codex"));
        assert_eq!(started[0].1, ["app-server", "--listen", "stdio://"]);
        assert_eq!(started[0].2, "codex");
        let home = host.home().join(".codex").to_string_lossy().into_owned();
        assert_eq!(started[0].3, [("CODEX_HOME".to_owned(), home.clone())]);
        let methods: Vec<_> = host
            .engine_sent
            .borrow()
            .iter()
            .map(|m| m["method"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            methods,
            [
                "initialize",
                "initialized",
                "account/rateLimits/read",
                "account/read"
            ]
        );
        let identity = legacy_account("Codex", "a@example.com", "ws").unwrap();
        let account = reading.account.unwrap();
        assert_eq!(account["id"], identity.as_str());
        assert_eq!(account["label"], "A@example.com");
        assert_eq!(account["plan"], "pro");
        assert_eq!(reading.quotas.len(), 2);
        assert_eq!(reading.quotas[0]["consumerSource"], "codex");
        assert_eq!(reading.quotas[1]["usedPercent"], 40.0);
        let remembered: Value = serde_json::from_slice(&host.own.borrow()[IDENTITIES]).unwrap();
        assert_eq!(remembered[format!("{home}/ws")], "A@example.com");
    }

    #[test]
    fn a_remembered_email_names_a_workspace_reported_alone() {
        let host = installed(limits(), Value::Null);
        let home = host.home().join(".codex").to_string_lossy().into_owned();
        host.own.borrow_mut().insert(
            IDENTITIES.into(),
            serde_json::to_vec(&json!({format!("{home}/ws"): "a@example.com"})).unwrap(),
        );
        let account = read(&host).unwrap().account.unwrap();
        assert_eq!(account["label"], "a@example.com");
    }

    #[test]
    fn a_home_without_a_chatgpt_plan_says_why_instead_of_the_engine_error() {
        let refused = json!({"id": 2, "error": {"message": "chatgpt authentication required to read rate limits"}});
        let cases = [
            (
                json!({"account": {"type": "apiKey"}, "requiresOpenaiAuth": true}),
                "Codex is signed in with an API key, which has no ChatGPT plan limits",
            ),
            (
                json!({"account": {"type": "amazonBedrock"}, "requiresOpenaiAuth": false}),
                "Codex runs on Amazon Bedrock, which has no ChatGPT plan limits",
            ),
            (
                json!({"account": null, "requiresOpenaiAuth": false}),
                "Codex uses a custom model provider, which has no ChatGPT plan limits",
            ),
        ];
        for (answer, notice) in cases {
            let mut host = installed(limits(), Value::Null);
            host.engine_replies.insert(2, refused.clone());
            host.engine_replies
                .insert(3, json!({"id": 3, "result": answer}));
            let reading = read(&host).unwrap();
            assert!(reading.quotas.is_empty(), "{notice}");
            assert!(reading.account.is_none(), "{notice}");
            assert_eq!(reading.notice.as_deref(), Some(notice));
        }
        // No sign-in at all is Codex's own state, not a plan without limits:
        // a failed read, so the last windows stay.
        let mut host = installed(limits(), Value::Null);
        host.engine_replies.insert(2, refused.clone());
        host.engine_replies.insert(
            3,
            json!({"id": 3, "result": {"account": null, "requiresOpenaiAuth": true}}),
        );
        assert_eq!(
            read(&host).err(),
            Some(Unread::SignedOut(
                "Codex is not signed in. Run codex login with ChatGPT to read its quota".into()
            ))
        );

        // A ChatGPT sign-in whose limits fail still shows what Codex said.
        let mut host = installed(limits(), Value::Null);
        host.engine_replies.insert(2, refused);
        host.engine_replies.insert(
            3,
            json!({"id": 3, "result": {"account": {"type": "chatgpt", "email": "a@example.com", "planType": "pro"}, "requiresOpenaiAuth": true}}),
        );
        assert_eq!(
            read(&host).err().unwrap(),
            "Codex: chatgpt authentication required to read rate limits"
        );
    }

    #[test]
    fn an_engine_without_account_read_still_reports_limits() {
        let mut host = installed(limits(), Value::Null);
        host.engine_replies
            .insert(3, json!({"id": 3, "error": {"message": "unknown method"}}));
        assert_eq!(read(&host).unwrap().quotas.len(), 2);
    }

    #[test]
    fn no_limits_says_whether_codex_exited_or_timed_out() {
        let mut host = installed(limits(), Value::Null);
        host.engine_replies.remove(&2);
        host.engine_replies.remove(&3);
        assert_eq!(
            read(&host).err().unwrap(),
            "Codex exited before reporting limits; check local sign-in"
        );
        host.engine_ends = false;
        assert_eq!(read(&host).err().unwrap(), "Codex quota query timed out");
        let missing = FakeHost::new("/u");
        assert_eq!(
            read(&missing).err().unwrap(),
            "Install and sign in to read quota"
        );
    }

    fn pi_signed_in(host: FakeHost) -> FakeHost {
        let expires = (chrono::Utc::now().timestamp() + 3600) * 1000;
        host.file(
            "/u/.pi/agent/auth.json",
            &json!({"openai-codex": {"type": "oauth", "access": "tok", "accountId": "acc", "expires": expires}})
                .to_string(),
        )
        .answer(
            "https://chatgpt.com/backend-api/wham/usage",
            Ok(json!({"account_id": "acc", "email": "p@example.com", "plan_type": "plus",
                      "rate_limit": {"primary_window": {"used_percent": 30, "limit_window_seconds": 18000, "reset_at": 1_792_000_000}}})),
        )
    }

    #[test]
    fn pi_reads_the_same_plan_without_codex() {
        let host = pi_signed_in(FakeHost::new("/u"));
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 1);
        assert_eq!(reading.quotas[0]["usedPercent"], 30.0);
        assert!(reading.quotas[0]["consumerSource"].is_null());
        assert_eq!(
            reading.account.unwrap()["id"],
            legacy_account("Codex", "p@example.com", "acc")
                .unwrap()
                .as_str()
        );
        assert_eq!(host.asked.borrow()[0].1.as_deref(), Some("Bearer tok"));
        assert!(host.headers.borrow()[0].contains(&("ChatGPT-Account-Id".into(), "acc".into())));
        assert!(reading.notice.is_none());
    }

    #[test]
    fn a_configured_codex_home_needs_the_cli_even_when_pi_succeeds() {
        let mut host = pi_signed_in(FakeHost::new("/u"));
        let work = host.home().join("codex-work");
        host.account_homes.insert("codex".into(), vec![work]);

        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 1);
        assert_eq!(
            reading.notice.as_deref(),
            Some("Install and sign in to read quota")
        );
    }

    #[test]
    fn pis_second_account_joins_codexs() {
        let host = pi_signed_in(installed(limits(), json!({"email": "a@example.com"})));
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 3);
        assert_eq!(reading.additional_accounts.len(), 1);
        assert_eq!(reading.additional_accounts[0]["label"], "p@example.com");
    }

    #[test]
    fn pi_does_not_duplicate_an_account_in_an_additional_home() {
        let mut host = installed(limits(), json!({"email": "a@example.com"}));
        let work = host.home().join("codex-work");
        let mut work_limits = limits();
        work_limits["accountId"] = json!("acc");
        additional(
            &mut host,
            &work,
            work_limits,
            json!({"email": "p@example.com"}),
        );
        let host = pi_signed_in(host);

        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 4);
        assert_eq!(reading.additional_accounts.len(), 1);
        assert_eq!(host.asked.borrow().len(), 1);
        assert!(reading
            .quotas
            .iter()
            .all(|quota| quota["consumerSource"] == "codex"));
    }

    #[test]
    fn pi_success_preserves_a_native_account_error() {
        let mut host = installed(limits(), Value::Null);
        host.engine_replies
            .insert(2, json!({"id": 2, "error": {"message": "Sign-in expired"}}));
        let reading = read(&pi_signed_in(host)).unwrap();
        assert_eq!(reading.quotas.len(), 1);
        assert_eq!(reading.notice.as_deref(), Some("Codex: Sign-in expired"));
    }

    #[test]
    fn a_pi_error_preserves_an_additional_homes_error() {
        let mut host = installed(limits(), json!({"email": "a@example.com"})).file(
            "/u/.pi/agent/auth.json",
            r#"{"openai-codex": {"type": "oauth", "access": "tok", "accountId": "acc", "expires": 1}}"#,
        );
        let work = host.home().join("codex-work");
        additional(&mut host, &work, limits(), Value::Null);
        host.engine_replies_by_home
            .get_mut(&work)
            .unwrap()
            .insert(2, json!({"id": 2, "error": {"message": "Sign-in expired"}}));

        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 2);
        let notice = reading.notice.unwrap();
        assert!(notice.contains("Sign-in expired"));
        assert!(notice.contains("Sign in to ChatGPT again in Pi"));
    }

    #[test]
    fn an_expired_pi_sign_in_is_a_notice_beside_codex() {
        let host = installed(limits(), json!({"email": "a@example.com"})).file(
            "/u/.pi/agent/auth.json",
            r#"{"openai-codex": {"type": "oauth", "access": "tok", "accountId": "acc", "expires": 1}}"#,
        );
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 2);
        assert_eq!(
            reading.notice.as_deref(),
            Some("Sign in to ChatGPT again in Pi")
        );
    }
}
