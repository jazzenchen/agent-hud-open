//! Claude's plan limits: Claude Code's own engine answers get_usage (Open's
//! ClaudeEngineUsageClient), and `claude auth status` tells a signed-out
//! engine from an API-key login. The signed-in account is the one Claude
//! Code's .claude.json names, read before and after. Without a Claude Code
//! sign-in, the Claude desktop app's own claude.ai sign-in answers instead.
use crate::account::{
    account_id, client_home_key, legacy_account, legacy_unresolved, read_json, status, variable,
    Answer, EngineStart, Host, HttpFailure, HttpRequest, Reading,
};
use crate::l10n;
use crate::naming::window_names;
use crate::Base;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

/// What Claude Code's engine gets on top of the user's environment.
fn engine_env() -> [(&'static str, String); 1] {
    [("CLAUDE_CODE_ENTRYPOINT", "agent-hud".into())]
}
/// Claude Code's configuration folder: `CLAUDE_CONFIG_DIR`, or `~/.claude`.
fn config_dir(host: &dyn Host) -> PathBuf {
    variable(host, "CLAUDE_CONFIG_DIR").unwrap_or_else(|| host.home().join(".claude"))
}
fn profile(host: &dyn Host) -> Value {
    let path = variable(host, "CLAUDE_CONFIG_DIR")
        .unwrap_or_else(|| host.home().to_path_buf())
        .join(".claude.json");
    read_json(host, &path).unwrap_or(Value::Null)["oauthAccount"].clone()
}
fn identity(value: &Value) -> Option<String> {
    let user = value["accountUuid"].as_str().unwrap_or_default();
    let organization = value["organizationUuid"].as_str().unwrap_or_default();
    (!user.is_empty() || !organization.is_empty())
        .then(|| account_id("claude", &[user, organization]))
}
fn plan(profile: &Value, fallback: Option<&str>) -> Option<String> {
    let fallback = fallback?;
    let mapped = match profile["organizationType"].as_str() {
        Some("claude_max") => "max",
        Some("claude_pro") => "pro",
        Some("claude_team") => "team",
        Some("claude_enterprise") => "enterprise",
        _ => fallback,
    };
    let value = if mapped == "max" && profile["organizationType"] == "claude_max" {
        match profile["userRateLimitTier"]
            .as_str()
            .or_else(|| profile["organizationRateLimitTier"].as_str())
        {
            Some("default_claude_max_5x") => "max_5x",
            Some("default_claude_max_20x") => "max_20x",
            _ => mapped,
        }
    } else {
        mapped
    };
    Some(value.into())
}
/// A profile's plan without the engine's word for it: the organization's
/// type, or failing that its rate-limit tier.
fn profile_plan(profile: &Value) -> Option<String> {
    let tier = profile["userRateLimitTier"]
        .as_str()
        .or_else(|| profile["organizationRateLimitTier"].as_str());
    let guess = profile["organizationType"]
        .as_str()
        .and_then(|kind| kind.strip_prefix("claude_"))
        .or_else(|| {
            tier.and_then(|tier| {
                ["max", "pro", "team", "enterprise"]
                    .into_iter()
                    .find(|plan| tier.contains(plan))
            })
        })?;
    plan(profile, Some(guess))
}
/// Engine failures in Open's words (agent-hud-native-open ClaudeDataError).
fn engine_failed(reason: &str) -> String {
    if l10n::chinese() {
        format!("Claude Code 引擎查询失败：{reason}")
    } else {
        format!("Claude Code engine query failed: {reason}")
    }
}
fn malformed() -> String {
    l10n::text(
        "Claude Code 引擎返回了无法识别的额度数据",
        "The Claude Code engine returned unrecognised usage data",
    )
}

/// Why the engine gave no reading. Only an engine that is missing or signed
/// out hands the reading to the desktop app's sign-in.
enum Unread {
    NoLogin(String),
    Failed(String),
}
impl From<String> for Unread {
    fn from(reason: String) -> Self {
        Unread::Failed(reason)
    }
}

pub fn read(host: &dyn Host) -> Result<Reading, String> {
    match from_engine(host) {
        Ok(reading) => Ok(reading),
        Err(Unread::Failed(reason)) => Err(reason),
        Err(Unread::NoLogin(reason)) => from_desktop(host).unwrap_or(Err(reason)),
    }
}

fn from_engine(host: &dyn Host) -> Result<Reading, Unread> {
    let executable = host.executable("claude").ok_or_else(|| {
        Unread::NoLogin(l10n::text(
            "未找到 Claude Code 引擎（claude 命令）",
            "Claude Code engine (claude command) not found",
        ))
    })?;
    let before = profile(host);
    let mut process = host.start_engine(&EngineStart {
        executable: &executable,
        args: &[
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--settings",
            r#"{"disableAllHooks":true}"#,
        ],
        folder: "claude",
        env: &engine_env(),
    })?;
    process.send(&json!({"type":"control_request","request_id":"agent-hud-usage","request":{"subtype":"get_usage","skip_behaviors":true}}))?;
    let deadline = Instant::now() + Duration::from_secs(40);
    while let Some(message) = process.next(deadline) {
        if message["type"] != "control_response" {
            continue;
        }
        if message["response"]["subtype"] == "error" {
            return Err(engine_failed(
                message["response"]["error"]
                    .as_str()
                    .unwrap_or("unknown error"),
            )
            .into());
        }
        let payload = &message["response"]["response"];
        if !payload.is_object() {
            return Err(engine_failed("empty get_usage response").into());
        }
        let after = profile(host);
        if identity(&before) != identity(&after) {
            return Err(l10n::text(
                "读取额度时 Claude 登录账户发生了变化，稍后自动重试",
                "The Claude account changed during the quota query; retrying shortly",
            )
            .into());
        }
        // Plan limits apply but the engine came back without numbers: its own
        // usage lookup failed (refused for asking too often, or an error) and
        // it had nothing seen earlier to answer with. That is a failed read,
        // so the last reading stays, not a login without plan limits.
        if payload["rate_limits_available"] == true && !payload["rate_limits"].is_object() {
            return Err(l10n::text(
                "这次没有读到 Claude 额度，稍后自动重试",
                "Couldn't read the Claude quota this time; retrying shortly",
            )
            .into());
        }
        if payload["rate_limits_available"] != true {
            // get_usage reports no plan limits both for an API-key login and
            // for no login at all; only `auth status` tells the two apart.
            let signed_in = host
                .start_engine(&EngineStart {
                    executable: &executable,
                    args: &["auth", "status", "--json"],
                    folder: "claude",
                    env: &engine_env(),
                })
                .ok()
                .and_then(|mut engine| engine.joined(Instant::now() + Duration::from_secs(10)))
                .and_then(|val| val["loggedIn"].as_bool());
            // Signed out is a failed read, so the last reading stays until
            // the person signs in again; an API-key login has no plan limits.
            if signed_in == Some(false) {
                return Err(Unread::NoLogin(l10n::text(
                    "Claude Code 已退出登录，运行 claude 重新登录后更新额度",
                    "Claude Code is signed out. Run claude to sign in again to update the quota",
                )));
            }
            let mut reading = Reading::empty("claude");
            reading.notice = Some(l10n::text(
                "当前登录方式没有订阅额度（API key 或第三方平台）",
                "No plan limits for this login (API key or third-party platform)",
            ));
            return Ok(reading);
        }
        let windows = windows(&payload["rate_limits"]).ok_or_else(malformed)?;
        let plan = plan(&after, payload["subscription_type"].as_str());
        return Ok(plan_reading(host, windows, &after, plan));
    }
    let reason = process.stderr_preview(200).unwrap_or_else(|| {
        l10n::text(
            "引擎没有在 40 秒内返回用量",
            "The engine returned no usage within 40 s",
        )
    });
    Err(engine_failed(&reason).into())
}

/// The Claude desktop app's data folder, where it keeps its claude.ai sign-in.
const DESKTOP: &str = "Claude";
const SITE: &str = "claude.ai";
const WEB_API: &str = "https://claude.ai/api";

fn desktop_unreadable() -> String {
    l10n::text(
        "暂时无法读取 Claude 桌面版的登录，稍后自动重试",
        "Couldn't read the Claude desktop app's sign-in; retrying shortly",
    )
}

/// The plan limits through the Claude desktop app's claude.ai sign-in, as
/// claude.ai's own usage page reads them. None when the desktop app has no
/// sign-in to offer, or the user did not allow reading it: the engine's
/// reason then stands.
fn from_desktop(host: &dyn Host) -> Option<Result<Reading, String>> {
    let session = match host.app_cookie(DESKTOP, SITE, "sessionKey") {
        Answer::Found(value) if !value.trim().is_empty() => value,
        Answer::Failed => return Some(Err(desktop_unreadable())),
        _ => return None,
    };
    let cookie = format!("sessionKey={}", session.trim());
    let get = |url: &str| {
        host.http_json(&HttpRequest {
            url,
            secrets: &[("Cookie", cookie.as_str())],
            headers: &[("Accept", "application/json")],
            body: None,
            timeout: Duration::from_secs(15),
        })
    };
    // A sign-in claude.ai refuses has ended: as good as none.
    let failed = |failure: HttpFailure| match failure {
        HttpFailure::Status(401 | 403) => None,
        failure => Some(Err(failure.message())),
    };
    let saved = read_json(
        host,
        &host.folder(Base::AppData).join(DESKTOP).join("config.json"),
    )
    .unwrap_or(Value::Null);
    let user = saved["lastKnownAccountUuid"]
        .as_str()
        .filter(|user| !user.is_empty());
    let organization = host
        .app_cookie(DESKTOP, SITE, "lastActiveOrg")
        .found()
        .map(|organization| organization.trim().to_owned())
        .filter(|organization| !organization.is_empty());
    let cli = profile(host);
    let same = user.is_some()
        && cli["accountUuid"].as_str() == user
        && cli["organizationUuid"].is_string()
        && organization
            .as_deref()
            .is_none_or(|organization| cli["organizationUuid"] == organization);
    let who = if same {
        // The account Claude Code last signed in to: its name and plan.
        cli
    } else {
        let account = match get(&format!("{WEB_API}/account")) {
            Ok(account) => account,
            Err(failure) => return failed(failure),
        };
        let memberships = account["memberships"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let membership = memberships
            .iter()
            .find(|membership| {
                organization.is_some()
                    && membership["organization"]["uuid"].as_str() == organization.as_deref()
            })
            .or(memberships.first())
            .cloned()
            .unwrap_or(Value::Null);
        let tier = membership["organization"]["rate_limit_tier"].as_str();
        json!({
            "accountUuid": user.or(account["uuid"].as_str()),
            "organizationUuid": organization.as_deref().or(membership["organization"]["uuid"].as_str()),
            "emailAddress": account["email_address"],
            "organizationType": tier.filter(|tier| tier.contains("claude_max")).map(|_| "claude_max"),
            "organizationRateLimitTier": tier,
        })
    };
    let Some(organization) = who["organizationUuid"]
        .as_str()
        .filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
    else {
        return Some(Err(desktop_unreadable()));
    };
    let limits = match get(&format!("{WEB_API}/organizations/{organization}/usage")) {
        Ok(limits) => limits,
        Err(failure) => return failed(failure),
    };
    let Some(windows) = windows(&limits) else {
        return Some(Err(l10n::text(
            "Claude 桌面版的登录返回了无法识别的额度数据",
            "The Claude desktop app's sign-in returned unrecognised usage data",
        )));
    };
    let plan = profile_plan(&who);
    Some(Ok(plan_reading(host, windows, &who, plan)))
}

/// A reading of `windows` for the account `who` names in Claude Code's
/// profile fields (accountUuid, organizationUuid, emailAddress). Both
/// sign-ins name one account the same way, so its windows carry on from one
/// to the other.
fn plan_reading(
    host: &dyn Host,
    windows: Vec<Window>,
    who: &Value,
    plan: Option<String>,
) -> Reading {
    let id = identity(who).unwrap_or_else(|| {
        account_id(
            "claude",
            &["unresolved", config_dir(host).to_string_lossy().as_ref()],
        )
    });
    let label = who["emailAddress"].as_str();
    let legacy = legacy_account(
        "Claude",
        who["accountUuid"].as_str().unwrap_or(""),
        who["organizationUuid"].as_str().unwrap_or(""),
    );
    let account_identity = legacy.unwrap_or_else(|| {
        legacy_unresolved(
            "Claude",
            &client_home_key(host, &config_dir(host), &host.home().join(".claude")),
        )
    });
    let mut reading = Reading::empty("claude");
    reading.account =
        Some(json!({"id":account_identity,"provider":"claude","label":label,"plan":plan}));
    for window in windows {
        let Window {
            key,
            name,
            percent,
            reset,
            family,
        } = window;
        // "5h", "Weekly", and a family's weekly window by the family alone.
        let short = match family.as_deref() {
            Some(_) => window_names::leading(name.trim_start_matches("Weekly limit · ")),
            None if key == "claude-session" => window_names::Period::FiveHours.short_name(),
            None => window_names::Period::Week.short_name(),
        };
        reading.quotas.push(json!({"id":format!("{id}:{key}"),"vendor":"claude","name":name,"shortLabel":short,"usedPercent":percent,"resetAt":reset,"status":status(percent),"account":label,"accountId":account_identity,"legacyWindowId":format!("{account_identity}/{key}"),"consumerSource":"claude","windowDurationSeconds":if key=="claude-session"{5*3600}else{7*86400},"allModels":family.is_none(),"modelFamilies":family.map(|family|vec![family])}));
    }
    reading
}

/// One plan window as the engine reports it.
struct Window {
    key: String,
    name: String,
    percent: f64,
    reset: Option<String>,
    family: Option<String>,
}

/// The model families Claude names (agent-hud-native-open ClaudeModelInfo.families).
const FAMILIES: [&str; 5] = ["opus", "sonnet", "haiku", "fable", "mythos"];

/// A model id's or name's family, lowercased: the first of its words that
/// names one (agent-hud-native-open ClaudeModelInfo.parse).
fn family(model: &str) -> Option<&'static str> {
    model
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .find_map(|word| FAMILIES.into_iter().find(|family| *family == word))
}

/// The windows of a `rate_limits` object in the order Claude Code's own
/// /usage screen shows them: the session, the week across models, then each
/// model family's week, families in alphabetical order (agent-hud-native-open
/// ClaudeUsage.rows). A family's week comes from `seven_day_<family>` or a
/// `limits[]` entry of kind `weekly_scoped`, which wins. None when the
/// object has neither the session nor the weekly window, as Open reads it.
fn windows(limits: &Value) -> Option<Vec<Window>> {
    let window = |object: &Value, value_key: &str| {
        let percent = object[value_key]
            .as_f64()
            .filter(|percent| percent.is_finite())?;
        let reset = object["resets_at"]
            .as_str()
            .filter(|value| chrono::DateTime::parse_from_rfc3339(value).is_ok())
            .map(str::to_owned);
        Some((percent, reset))
    };
    let session = window(&limits["five_hour"], "utilization");
    let week = window(&limits["seven_day"], "utilization");
    if session.is_none() && week.is_none() {
        return None;
    }
    // Open reads `seven_day_opus` and `seven_day_sonnet`; the other families'
    // keys are read too, so a family window the service adds is not missed.
    let mut families = std::collections::BTreeMap::<String, (f64, Option<String>)>::new();
    for family in FAMILIES {
        if let Some(found) = window(
            &limits[format!("seven_day_{family}").as_str()],
            "utilization",
        ) {
            families.insert(family.into(), found);
        }
    }
    for entry in limits["limits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["kind"] == "weekly_scoped")
    {
        let model = &entry["scope"]["model"];
        let name = model["display_name"]
            .as_str()
            .or_else(|| model["id"].as_str());
        let Some(key) = name
            .and_then(family)
            .map(str::to_owned)
            .or_else(|| model["display_name"].as_str().map(str::to_lowercase))
        else {
            continue;
        };
        if let Some(found) = window(entry, "percent") {
            families.insert(key, found);
        }
    }
    let mut rows = Vec::new();
    for (key, name, found) in [
        ("claude-session", "Current session", session),
        ("claude-weekly", "Weekly limit · All models", week),
    ] {
        if let Some((percent, reset)) = found {
            rows.push(Window {
                key: key.into(),
                name: name.into(),
                percent,
                reset,
                family: None,
            });
        }
    }
    for (family, (percent, reset)) in families {
        let mut letters = family.chars();
        let capitalized = letters
            .next()
            .map(|first| first.to_uppercase().chain(letters).collect::<String>())
            .unwrap_or_default();
        rows.push(Window {
            key: format!("claude-weekly-{family}"),
            name: format!("Weekly limit · {capitalized}"),
            percent,
            reset,
            family: Some(family),
        });
    }
    Some(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    fn installed(usage: Value) -> FakeHost {
        let mut host = FakeHost::new("/u").file(
            "/u/.claude.json",
            &json!({"oauthAccount": {
                "accountUuid": "user-1", "organizationUuid": "org-1",
                "emailAddress": "me@example.com", "organizationType": "claude_max",
                "userRateLimitTier": "default_claude_max_20x",
            }})
            .to_string(),
        );
        host.executables
            .insert("claude".into(), "/bin/claude".into());
        host.engine_output.insert(
            "-p".into(),
            vec![
                json!({"type": "system"}),
                json!({"type": "control_response", "response": usage}),
            ],
        );
        host
    }

    fn limits() -> Value {
        json!({"subtype": "success", "response": {
            "rate_limits_available": true, "subscription_type": "max",
            "rate_limits": {"five_hour": {"utilization": 12.0}, "seven_day": {"utilization": 40.0}},
        }})
    }

    #[test]
    fn the_engine_reports_the_plans_windows() {
        let host = installed(limits());
        let reading = read(&host).unwrap();
        let identity = legacy_account("Claude", "user-1", "org-1").unwrap();
        let account = reading.account.unwrap();
        assert_eq!(account["id"], identity.as_str());
        assert_eq!(account["label"], "me@example.com");
        assert_eq!(account["plan"], "max_20x");
        assert_eq!(reading.quotas.len(), 2);
        let id = account_id("claude", &["user-1", "org-1"]);
        assert_eq!(reading.quotas[0]["id"], format!("{id}:claude-session"));
        assert_eq!(reading.quotas[0]["windowDurationSeconds"], 18000);
        let started = host.started.borrow();
        assert_eq!(started[0].0, PathBuf::from("/bin/claude"));
        assert_eq!(started[0].2, "claude");
        assert!(started[0]
            .1
            .contains(&r#"{"disableAllHooks":true}"#.to_owned()));
        assert_eq!(
            started[0].3,
            [("CLAUDE_CODE_ENTRYPOINT".to_owned(), "agent-hud".to_owned())]
        );
        assert_eq!(
            host.engine_sent.borrow()[0]["request"]["subtype"],
            "get_usage"
        );
    }

    #[test]
    fn without_plan_limits_auth_status_tells_signed_out_from_api_keys() {
        let none = json!({"subtype": "success", "response": {"rate_limits_available": false}});
        let mut host = installed(none.clone());
        host.engine_output
            .insert("auth".into(), vec![json!({"loggedIn": false})]);
        // Signed out fails the read, so the last reading stays.
        assert_eq!(
            read(&host).err().as_deref(),
            Some("Claude Code is signed out. Run claude to sign in again to update the quota")
        );
        let mut host = installed(none);
        host.engine_output
            .insert("auth".into(), vec![json!({"loggedIn": true})]);
        assert_eq!(
            read(&host).unwrap().notice.as_deref(),
            Some("No plan limits for this login (API key or third-party platform)")
        );
        assert_eq!(host.started.borrow()[1].1, ["auth", "status", "--json"]);
    }

    #[test]
    fn plan_limits_without_numbers_are_a_failed_read_not_a_login_without_limits() {
        // Claude Code 2.1.307 answers this way when its usage lookup fails and
        // it has no numbers seen earlier; `auth status` is not asked.
        let empty = json!({"subtype": "success", "response": {
            "rate_limits_available": true, "subscription_type": "max", "rate_limits": null,
        }});
        let host = installed(empty);
        assert_eq!(
            read(&host).err().unwrap(),
            "Couldn't read the Claude quota this time; retrying shortly"
        );
        assert_eq!(host.started.borrow().len(), 1);
    }

    #[test]
    fn engine_failures_are_worded_as_open_words_them() {
        let host = installed(json!({"subtype": "error", "error": "boom"}));
        assert_eq!(
            read(&host).err().unwrap(),
            "Claude Code engine query failed: boom"
        );
        let mut silent = installed(limits());
        silent.engine_output.clear();
        assert_eq!(
            read(&silent).err().unwrap(),
            "Claude Code engine query failed: The engine returned no usage within 40 s"
        );
        assert_eq!(
            read(&FakeHost::new("/u")).err().unwrap(),
            "Claude Code engine (claude command) not found"
        );
    }

    #[test]
    fn a_login_without_an_account_is_the_config_folders() {
        let mut host = installed(limits());
        host.files.clear();
        let reading = read(&host).unwrap();
        assert_eq!(
            reading.account.unwrap()["id"],
            legacy_unresolved("Claude", "").as_str()
        );
        let config = host.home().join(".claude");
        let id = account_id("claude", &["unresolved", &config.to_string_lossy()]);
        assert_eq!(reading.quotas[0]["id"], format!("{id}:claude-session"));
    }

    #[test]
    fn windows_follow_claude_codes_order_with_families_alphabetical() {
        let limits = json!({
            "five_hour": {"utilization": 12.0, "resets_at": "2026-10-21T15:00:00Z"},
            "seven_day": {"utilization": 40.0, "resets_at": "2026-10-25T00:00:00Z"},
            "seven_day_sonnet": {"utilization": 5.0},
            "seven_day_opus": {"utilization": 30.0},
            "limits": [
                {"kind": "weekly_scoped", "percent": 55.0, "scope": {"model": {"display_name": "Claude Fable 5"}}},
                {"kind": "weekly_scoped", "percent": 31.0, "scope": {"model": {"id": "claude-opus-5-1"}}},
                {"kind": "session", "percent": 99.0}
            ]
        });
        let rows = windows(&limits).unwrap();
        let keys = rows.iter().map(|row| row.key.as_str()).collect::<Vec<_>>();
        assert_eq!(
            keys,
            [
                "claude-session",
                "claude-weekly",
                "claude-weekly-fable",
                "claude-weekly-opus",
                "claude-weekly-sonnet"
            ]
        );
        assert_eq!(rows[2].name, "Weekly limit · Fable");
        assert_eq!(
            rows[3].percent, 31.0,
            "a weekly_scoped entry wins over seven_day_<family>"
        );
        assert_eq!(rows[0].reset.as_deref(), Some("2026-10-21T15:00:00Z"));
    }

    #[test]
    fn a_reading_without_the_session_or_weekly_window_is_unrecognised() {
        let only_family = json!({"seven_day_opus": {"utilization": 30.0}, "five_hour": null});
        assert!(windows(&only_family).is_none());
        let weekly_only = json!({"seven_day": {"utilization": 1}});
        assert_eq!(windows(&weekly_only).unwrap().len(), 1);
    }

    /// An engine that is installed but signed out, and the desktop app's
    /// claude.ai sign-in beside it.
    fn signed_out_with_desktop() -> FakeHost {
        let mut host =
            installed(json!({"subtype": "success", "response": {"rate_limits_available": false}}));
        host.engine_output
            .insert("auth".into(), vec![json!({"loggedIn": false})]);
        host.cookies.insert(
            "Claude|claude.ai|sessionKey".into(),
            Answer::Found("sk-ant-sid01-secret".into()),
        );
        host.cookies.insert(
            "Claude|claude.ai|lastActiveOrg".into(),
            Answer::Found("0a1b-2c3d".into()),
        );
        host
    }

    fn web_limits() -> Value {
        json!({
            "five_hour": {"utilization": 18.0, "resets_at": "2026-10-31T15:00:00.394+00:00"},
            "seven_day": {"utilization": 79.0, "resets_at": "2026-11-03T12:00:00.394+00:00"},
            "seven_day_fable": {"utilization": 0.0, "resets_at": "2026-11-03T12:00:00+00:00"},
        })
    }

    #[test]
    fn a_signed_out_engine_reads_through_the_desktop_apps_sign_in() {
        let host = signed_out_with_desktop()
            .file(
                "/u/.claude.json",
                &json!({"oauthAccount": {
                    "accountUuid": "user-1", "organizationUuid": "0a1b-2c3d",
                    "emailAddress": "me@example.com", "organizationType": "claude_max",
                    "organizationRateLimitTier": "default_claude_max_20x",
                }})
                .to_string(),
            )
            .file(
                "/u/AppData/Claude/config.json",
                &json!({"lastKnownAccountUuid": "user-1"}).to_string(),
            )
            .answer(
                "https://claude.ai/api/organizations/0a1b-2c3d/usage",
                Ok(web_limits()),
            );
        let reading = read(&host).unwrap();
        // The same account as Claude Code's last sign-in: its windows carry on.
        let id = account_id("claude", &["user-1", "0a1b-2c3d"]);
        assert_eq!(reading.quotas[0]["id"], format!("{id}:claude-session"));
        assert_eq!(reading.quotas[0]["usedPercent"], 18.0);
        assert_eq!(reading.quotas[1]["usedPercent"], 79.0);
        assert_eq!(reading.quotas[2]["id"], format!("{id}:claude-weekly-fable"));
        let account = reading.account.unwrap();
        assert_eq!(account["label"], "me@example.com");
        assert_eq!(account["plan"], "max_20x");
        // Claude Code's profile names the account, so only the usage is asked,
        // with the session as a cookie.
        let asked = host.asked.borrow();
        assert_eq!(asked.len(), 1);
        assert_eq!(
            host.sent.borrow()[0].0,
            vec![(
                "Cookie".to_owned(),
                "sessionKey=sk-ant-sid01-secret".to_owned()
            )]
        );
    }

    #[test]
    fn without_claude_code_the_desktop_apps_account_names_itself() {
        let mut host = FakeHost::new("/u")
            .file(
                "/u/AppData/Claude/config.json",
                &json!({"lastKnownAccountUuid": "user-9"}).to_string(),
            )
            .answer(
                "https://claude.ai/api/account",
                Ok(json!({"email_address": "desk@example.com", "memberships": [
                    {"organization": {"uuid": "ffff", "rate_limit_tier": "default_claude_pro"}},
                    {"organization": {"uuid": "0a1b-2c3d", "rate_limit_tier": "default_claude_max_5x"}},
                ]})),
            )
            .answer(
                "https://claude.ai/api/organizations/0a1b-2c3d/usage",
                Ok(web_limits()),
            );
        host.cookies.insert(
            "Claude|claude.ai|sessionKey".into(),
            Answer::Found("sk-ant-sid01-secret".into()),
        );
        host.cookies.insert(
            "Claude|claude.ai|lastActiveOrg".into(),
            Answer::Found("0a1b-2c3d".into()),
        );
        let reading = read(&host).unwrap();
        let id = account_id("claude", &["user-9", "0a1b-2c3d"]);
        assert_eq!(reading.quotas[0]["id"], format!("{id}:claude-session"));
        let account = reading.account.unwrap();
        assert_eq!(account["label"], "desk@example.com");
        assert_eq!(account["plan"], "max_5x");
    }

    #[test]
    fn a_desktop_sign_in_that_is_refused_or_ended_leaves_the_engines_reason() {
        let signed_out =
            "Claude Code is signed out. Run claude to sign in again to update the quota";
        let mut denied = signed_out_with_desktop();
        denied
            .cookies
            .insert("Claude|claude.ai|sessionKey".into(), Answer::Denied);
        assert_eq!(read(&denied).err().as_deref(), Some(signed_out));
        assert!(denied.asked.borrow().is_empty());

        let ended = signed_out_with_desktop()
            .file(
                "/u/AppData/Claude/config.json",
                &json!({"lastKnownAccountUuid": "user-1"}).to_string(),
            )
            .answer(
                "https://claude.ai/api/account",
                Err(HttpFailure::Status(401)),
            );
        assert_eq!(read(&ended).err().as_deref(), Some(signed_out));

        let mut unreadable = signed_out_with_desktop();
        unreadable
            .cookies
            .insert("Claude|claude.ai|sessionKey".into(), Answer::Failed);
        assert_eq!(
            read(&unreadable).err().as_deref(),
            Some("Couldn't read the Claude desktop app's sign-in; retrying shortly")
        );
    }

    #[test]
    fn a_signed_in_engine_never_reads_the_desktop_app() {
        let mut host = installed(limits());
        host.cookies.insert(
            "Claude|claude.ai|sessionKey".into(),
            Answer::Found("sk-ant-sid01-secret".into()),
        );
        assert_eq!(read(&host).unwrap().quotas.len(), 2);
        assert!(host.asked.borrow().is_empty());
    }
}
