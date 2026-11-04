//! Claude's plan limits: Claude Code's own engine answers get_usage (Open's
//! ClaudeEngineUsageClient), and `claude auth status` tells a signed-out
//! engine from an API-key login. The signed-in account is the one Claude
//! Code's .claude.json names, read before and after. Without a Claude Code
//! sign-in, the Claude desktop app's own claude.ai sign-in answers instead.
//! Folders added in Settings (`claudeHomes`) are read after the default one,
//! one at a time, each as its own account.
//!
//! Claude Code's own record of its last usage answer (`usage_record`) comes
//! first while it is under a minute old, which is what the engine would
//! answer then, so the engine is not started for it. When every way fails,
//! the record's windows stand in, as old as they are, and the failure stays
//! what it was: a signed-out Claude Code is still told to sign in again.
use super::usage_record;
use crate::account::{
    account_id, append_notice, client_home_key, legacy_account, legacy_unresolved, merge,
    read_json, status, variable, Answer, EngineStart, Host, HttpFailure, HttpRequest, Provider,
    Reading, StatusPage, Unread,
};
use crate::l10n;
use crate::naming::window_names;
use crate::Base;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// A Claude Code configuration folder to read. The default one is
/// `CLAUDE_CONFIG_DIR`, or `~/.claude`. An added one is a folder chosen in
/// Settings: its engine runs with `CLAUDE_CONFIG_DIR` set to it, and it
/// never falls back to the desktop app's sign-in, which is the default
/// folder's.
struct Folder {
    path: PathBuf,
    added: bool,
}

/// What Claude Code's engine gets on top of the user's environment.
fn engine_env(folder: &Folder) -> Vec<(&'static str, String)> {
    let mut env = vec![("CLAUDE_CODE_ENTRYPOINT", "agent-hud".into())];
    if folder.added {
        env.push((
            "CLAUDE_CONFIG_DIR",
            folder.path.to_string_lossy().into_owned(),
        ));
    }
    env
}
/// Claude Code's configuration folder: `CLAUDE_CONFIG_DIR`, or `~/.claude`.
fn config_dir(host: &dyn Host) -> PathBuf {
    variable(host, "CLAUDE_CONFIG_DIR").unwrap_or_else(|| host.home().join(".claude"))
}
/// A folder's `.claude.json`, Claude Code's global configuration: the
/// signed-in account (`oauthAccount`) and Claude Code's own usage record.
/// Claude Code keeps it in `CLAUDE_CONFIG_DIR` when that is set, and in the
/// home folder otherwise.
fn global_config(host: &dyn Host, folder: &Folder) -> Value {
    let path = if folder.added {
        folder.path.clone()
    } else {
        variable(host, "CLAUDE_CONFIG_DIR").unwrap_or_else(|| host.home().to_path_buf())
    }
    .join(".claude.json");
    read_json(host, &path).unwrap_or(Value::Null)
}
/// The account a folder's `.claude.json` names.
fn profile(host: &dyn Host, folder: &Folder) -> Value {
    global_config(host, folder)["oauthAccount"].clone()
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
enum EngineUnread {
    /// No Claude Code, or one that is signed out (`installed`).
    NoLogin {
        reason: String,
        installed: bool,
    },
    Failed(Unread),
}
impl From<String> for EngineUnread {
    fn from(reason: String) -> Self {
        EngineUnread::Failed(Unread::Failed(reason))
    }
}
impl From<Unread> for EngineUnread {
    fn from(unread: Unread) -> Self {
        EngineUnread::Failed(unread)
    }
}

/// Claude Code's quota, read through its engine; with no engine or a
/// signed-out one, through the Claude desktop app's sign-in.
pub static PROVIDER: Provider = Provider {
    // Claude reports only the windows in use; one it leaves out keeps its last reading.
    keeps_unreported_windows: true,
    status: Some(&STATUS),
    ..Provider::new("claude", read)
};
/// Claude's status page: Claude Code runs on the API.
static STATUS: StatusPage = StatusPage {
    url: "https://status.claude.com/api/v2/summary.json",
    components: &["Claude Code", "Claude API (api.anthropic.com)"],
};

/// The default folder, then each folder added in Settings that is another
/// folder, one at a time. With more than one, each notice and failure names
/// its folder, and the reading fails only when every folder does. A failed
/// folder's last record shows stale beside the folders that read; when every
/// folder fails, the records come with the failures.
pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let default = config_dir(host);
    let canonical = |path: &Path| host.canonical(path).unwrap_or_else(|| path.to_path_buf());
    let mut seen = HashSet::from([canonical(&default)]);
    let folders: Vec<_> = std::iter::once(Folder {
        path: default,
        added: false,
    })
    .chain(
        host.account_homes("claude")
            .iter()
            .map(|path| canonical(path))
            .filter(|path| seen.insert(path.clone()))
            .map(|path| Folder { path, added: true }),
    )
    .collect();
    if folders.len() == 1 {
        return read_folder(host, &folders[0]);
    }
    let mut reading = Reading::empty("claude");
    let mut succeeded = false;
    let mut failures = Vec::new();
    let mut records = Vec::new();
    for folder in &folders {
        if host.cancelled() {
            return Err(l10n::text("账户读取已停止", "Account reading was stopped").into());
        }
        let prefix = format!("{}: ", folder.path.display());
        let error = match read_folder(host, folder) {
            Ok(mut account) => match account.failure.take() {
                None => {
                    succeeded = true;
                    let notice = account.notice.take();
                    merge(&mut reading, account);
                    if let Some(notice) = notice {
                        append_notice(&mut reading, format!("{prefix}{notice}"));
                    }
                    continue;
                }
                Some(error) => {
                    records.push(account);
                    error
                }
            },
            Err(error) => error,
        };
        let error = error.prefixed(&prefix);
        append_notice(&mut reading, error.reason().to_owned());
        failures.push(error);
    }
    let failure = (!succeeded)
        .then(|| Unread::combined(failures).unwrap_or_else(|| Unread::Failed(String::new())));
    let recorded = !records.is_empty();
    for mut record in records {
        // Beside folders that read, a failed folder's windows show as a
        // failed reading's do.
        if failure.is_none() {
            stale(&mut record);
        }
        merge(&mut reading, record);
    }
    match failure {
        None => Ok(reading),
        Some(failure) if recorded => {
            reading.failure = Some(failure);
            Ok(reading)
        }
        Some(failure) => Err(failure),
    }
}

/// A reading's windows and accounts marked stale.
fn stale(reading: &mut Reading) {
    for row in &mut reading.quotas {
        row["status"] = json!("stale");
    }
    for account in reading
        .account
        .iter_mut()
        .chain(&mut reading.additional_accounts)
    {
        account["stale"] = json!(true);
    }
}

fn read_folder(host: &dyn Host, folder: &Folder) -> Result<Reading, Unread> {
    let config = global_config(host, folder);
    let now = chrono::Utc::now().timestamp_millis();
    if let Some(reading) = usage_record::read(&config, now)
        .filter(|record| record.fresh(now))
        .and_then(|record| recorded(host, folder, &config["oauthAccount"], &record))
    {
        return Ok(reading);
    }
    let read = match from_engine(host, folder, &config["oauthAccount"]) {
        Ok(reading) => Ok(reading),
        Err(EngineUnread::Failed(unread)) => Err(unread),
        // A Claude Code that is installed and signed out is the agent's own
        // state; no Claude Code at all is not a sign-out.
        Err(EngineUnread::NoLogin { reason, installed }) => {
            let desktop = if folder.added {
                None
            } else {
                from_desktop(host, folder, installed)
            };
            desktop.unwrap_or(Err(if installed {
                Unread::SignedOut(reason)
            } else {
                Unread::Failed(reason)
            }))
        }
    };
    read.or_else(|failure| kept(host, folder, failure))
}

/// `failure`, with the windows Claude Code last recorded for the folder's
/// account when it has any, as old as they are; the failure's reason says
/// how old. The file is read again: Claude Code may have written a newer
/// record while the reading ran.
fn kept(host: &dyn Host, folder: &Folder, failure: Unread) -> Result<Reading, Unread> {
    let config = global_config(host, folder);
    let now = chrono::Utc::now().timestamp_millis();
    let Some((record, mut reading)) = usage_record::read(&config, now).and_then(|record| {
        let reading = recorded(host, folder, &config["oauthAccount"], &record)?;
        Some((record, reading))
    }) else {
        return Err(failure);
    };
    let shown = record.shown(now);
    reading.failure = Some(failure.reworded(|reason| format!("{reason} · {shown}")));
    Ok(reading)
}

/// The windows of Claude Code's own `record` for the account `profile`
/// names, as of when Claude Code fetched them, under the same ids as the
/// engine's. None when it holds neither the session nor the weekly window.
fn recorded(
    host: &dyn Host,
    folder: &Folder,
    profile: &Value,
    record: &usage_record::Record,
) -> Option<Reading> {
    let windows = windows(record.utilization)?;
    let mut reading = plan_reading(host, windows, profile, profile_plan(profile), &folder.path);
    let fetched = crate::iso(record.fetched_ms);
    for row in &mut reading.quotas {
        row["observedAt"] = fetched.clone();
    }
    if let Some(account) = &mut reading.account {
        account["checkedAt"] = fetched;
    }
    Some(reading)
}

/// The engine's reading of `folder`, whose account was `before` when it started.
fn from_engine(host: &dyn Host, folder: &Folder, before: &Value) -> Result<Reading, EngineUnread> {
    let executable = host
        .executable("claude")
        .ok_or_else(|| EngineUnread::NoLogin {
            reason: l10n::text(
                "未找到 Claude Code 引擎（claude 命令）",
                "Claude Code engine (claude command) not found",
            ),
            installed: false,
        })?;
    let env = engine_env(folder);
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
        env: &env,
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
        let after = profile(host, folder);
        if identity(before) != identity(&after) {
            return Err(Unread::transient(l10n::text(
                "读取额度时 Claude 登录账户发生了变化，稍后自动重试",
                "The Claude account changed during the quota query; retrying shortly",
            ))
            .into());
        }
        // Plan limits apply but the engine came back without numbers: its own
        // usage lookup failed (refused for asking too often, or an error) and
        // it had nothing seen earlier to answer with. That is a failed read,
        // so the last reading stays, not a login without plan limits.
        if payload["rate_limits_available"] == true && !payload["rate_limits"].is_object() {
            return Err(Unread::transient(l10n::text(
                "这次没有读到 Claude 额度，稍后自动重试",
                "Couldn't read the Claude quota this time; retrying shortly",
            ))
            .into());
        }
        if payload["rate_limits_available"] != true {
            // get_usage reports no plan limits both for an API-key login and
            // for no login at all; only `auth status` tells the two apart.
            // An unknown login state is a failed read, so the last reading stays.
            let signed_in = host
                .start_engine(&EngineStart {
                    executable: &executable,
                    args: &["auth", "status", "--json"],
                    folder: "claude",
                    env: &env,
                })
                .ok()
                .and_then(|mut engine| engine.joined(Instant::now() + Duration::from_secs(10)))
                .and_then(|val| val["loggedIn"].as_bool())
                .ok_or_else(|| {
                    EngineUnread::Failed(Unread::transient(l10n::text(
                        "这次没有读到 Claude 额度，稍后自动重试",
                        "Couldn't read the Claude quota this time; retrying shortly",
                    )))
                })?;
            // Signed out is a failed read, so the last reading stays until
            // the person signs in again; an API-key login has no plan limits.
            if !signed_in {
                return Err(EngineUnread::NoLogin {
                    reason: l10n::text(
                        "Claude Code 已退出登录，运行 claude 重新登录后更新额度",
                        "Claude Code is signed out. Run claude to sign in again to update the quota",
                    ),
                    installed: true,
                });
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
        return Ok(plan_reading(host, windows, &after, plan, &folder.path));
    }
    let reason = process.stderr_preview(200).unwrap_or_else(|| {
        l10n::text(
            "引擎没有在 40 秒内返回用量",
            "The engine returned no usage within 40 s",
        )
    });
    // No answer in time is the engine's, not the account's: it passes.
    Err(Unread::transient(engine_failed(&reason)).into())
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

/// claude.ai answered with Cloudflare's browser check. The desktop app, a
/// browser, passes it; this request can't. A signed-out Claude Code reads
/// without it once signed in again, so that is the way back.
fn desktop_challenged(signed_out: bool) -> String {
    if signed_out {
        l10n::text(
            "Claude Code 已退出登录，claude.ai 也要求浏览器验证；运行 claude 重新登录后更新额度",
            "Claude Code is signed out and claude.ai asked for a browser check. Run claude to sign in again to update the quota",
        )
    } else {
        l10n::text(
            "claude.ai 要求浏览器验证，暂时无法通过 Claude 桌面版读取额度，稍后自动重试",
            "claude.ai asked for a browser check, so the Claude desktop app's sign-in can't read the quota; retrying shortly",
        )
    }
}

/// The plan limits through the Claude desktop app's claude.ai sign-in, as
/// claude.ai's own usage page reads them. None when the desktop app has no
/// sign-in to offer, or the user did not allow reading it: the engine's
/// reason then stands. `signed_out`: Claude Code is installed but signed out.
/// Only the default `folder` is read this way.
fn from_desktop(
    host: &dyn Host,
    folder: &Folder,
    signed_out: bool,
) -> Option<Result<Reading, Unread>> {
    let session = match host.app_cookie(DESKTOP, SITE, "sessionKey") {
        Answer::Found(value) if !value.trim().is_empty() => value,
        Answer::Failed => return Some(Err(Unread::transient(desktop_unreadable()))),
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
    // A sign-in claude.ai refuses has ended: as good as none. Its browser
    // check says nothing about the sign-in, so it is named as itself.
    let failed = |failure: HttpFailure| match failure {
        HttpFailure::Status(401 | 403) => None,
        HttpFailure::Challenged => Some(Err(if signed_out {
            Unread::SignedOut(desktop_challenged(true))
        } else {
            Unread::transient(desktop_challenged(false))
        })),
        failure => Some(Err(failure.unread())),
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
    let cli = profile(host, folder);
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
        return Some(Err(Unread::transient(desktop_unreadable())));
    };
    let limits = match get(&format!("{WEB_API}/organizations/{organization}/usage")) {
        Ok(limits) => limits,
        Err(failure) => return failed(failure),
    };
    let Some(windows) = windows(&limits) else {
        return Some(Err(l10n::text(
            "Claude 桌面版的登录返回了无法识别的额度数据",
            "The Claude desktop app's sign-in returned unrecognised usage data",
        )
        .into()));
    };
    let plan = profile_plan(&who);
    Some(Ok(plan_reading(host, windows, &who, plan, &folder.path)))
}

/// A reading of `windows` for the account `who` names in Claude Code's
/// profile fields (accountUuid, organizationUuid, emailAddress). Both
/// sign-ins name one account the same way, so its windows carry on from one
/// to the other. A login that names no account is the configuration
/// `folder`'s.
fn plan_reading(
    host: &dyn Host,
    windows: Vec<Window>,
    who: &Value,
    plan: Option<String>,
    folder: &Path,
) -> Reading {
    let id = identity(who).unwrap_or_else(|| {
        account_id("claude", &["unresolved", folder.to_string_lossy().as_ref()])
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
            &client_home_key(host, folder, &host.home().join(".claude")),
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
    use std::collections::HashMap;

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
            read(&host).err().as_ref().map(Unread::reason),
            Some("Claude Code is signed out. Run claude to sign in again to update the quota")
        );
        assert_eq!(host.started.borrow()[1].1, ["auth", "status", "--json"]);
        let mut host = installed(none);
        host.engine_output
            .insert("auth".into(), vec![json!({"loggedIn": true})]);
        let reading = read(&host).unwrap();
        assert!(reading.quotas.is_empty());
        assert!(reading.account.is_none());
        assert_eq!(
            reading.notice.as_deref(),
            Some("No plan limits for this login (API key or third-party platform)")
        );
        assert_eq!(host.started.borrow()[1].1, ["auth", "status", "--json"]);
    }

    #[test]
    fn auth_status_start_failure_is_a_failed_read() {
        let mut host = installed(json!({
            "subtype": "success", "response": {"rate_limits_available": false},
        }));
        host.engine_start_errors
            .insert("auth".into(), "could not start".into());
        assert_eq!(
            read(&host).err().unwrap(),
            "Couldn't read the Claude quota this time; retrying shortly"
        );
        assert_eq!(host.started.borrow()[1].1, ["auth", "status", "--json"]);
    }

    #[test]
    fn auth_status_without_a_parsed_response_is_a_failed_read() {
        // No parsed response covers empty or invalid JSON output and a timeout.
        for ends in [true, false] {
            let mut host = installed(json!({
                "subtype": "success", "response": {"rate_limits_available": false},
            }));
            host.engine_ends = ends;
            assert_eq!(
                read(&host).err().unwrap(),
                "Couldn't read the Claude quota this time; retrying shortly"
            );
            assert_eq!(host.started.borrow()[1].1, ["auth", "status", "--json"]);
        }
    }

    #[test]
    fn auth_status_without_a_boolean_login_state_is_a_failed_read() {
        for output in [
            Value::Null,
            json!({}),
            json!({"loggedIn": null}),
            json!({"loggedIn": "false"}),
            json!({"loggedIn": 0}),
        ] {
            let mut host = installed(json!({
                "subtype": "success", "response": {"rate_limits_available": false},
            }));
            host.engine_output.insert("auth".into(), vec![output]);
            assert_eq!(
                read(&host).err().unwrap(),
                "Couldn't read the Claude quota this time; retrying shortly"
            );
        }
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
        assert_eq!(
            read(&denied).err().as_ref().map(Unread::reason),
            Some(signed_out)
        );
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
        assert_eq!(
            read(&ended).err().as_ref().map(Unread::reason),
            Some(signed_out)
        );

        let mut unreadable = signed_out_with_desktop();
        unreadable
            .cookies
            .insert("Claude|claude.ai|sessionKey".into(), Answer::Failed);
        assert_eq!(
            read(&unreadable).err().as_ref().map(Unread::reason),
            Some("Couldn't read the Claude desktop app's sign-in; retrying shortly")
        );
    }

    #[test]
    fn a_browser_check_is_named_not_taken_for_an_ended_sign_in() {
        let challenged = signed_out_with_desktop()
            .file(
                "/u/AppData/Claude/config.json",
                &json!({"lastKnownAccountUuid": "user-1"}).to_string(),
            )
            .answer(
                "https://claude.ai/api/account",
                Err(HttpFailure::Challenged),
            );
        assert_eq!(
            read(&challenged).err().as_ref().map(Unread::reason),
            Some("Claude Code is signed out and claude.ai asked for a browser check. Run claude to sign in again to update the quota")
        );

        // Without Claude Code there is no sign-in to point to: it retries.
        let mut desktop_only = FakeHost::new("/u").answer(
            "https://claude.ai/api/account",
            Err(HttpFailure::Challenged),
        );
        desktop_only.cookies.insert(
            "Claude|claude.ai|sessionKey".into(),
            Answer::Found("sk-ant-sid01-secret".into()),
        );
        assert_eq!(
            read(&desktop_only).err().as_ref().map(Unread::reason),
            Some("claude.ai asked for a browser check, so the Claude desktop app's sign-in can't read the quota; retrying shortly")
        );
    }

    /// A folder added in Settings, signed in to `user`'s own account, whose
    /// engine answers `usage`.
    fn added(host: &mut FakeHost, folder: &Path, user: &str, usage: Value) {
        host.account_homes
            .entry("claude".into())
            .or_default()
            .push(folder.to_path_buf());
        let profile = json!({"oauthAccount": {
            "accountUuid": user, "organizationUuid": format!("{user}-org"),
            "emailAddress": format!("{user}@example.com"), "organizationType": "claude_pro",
        }});
        host.files.insert(
            folder.join(".claude.json"),
            Answer::Found(profile.to_string().into_bytes()),
        );
        host.engine_output_by_home.insert(
            folder.to_path_buf(),
            HashMap::from([(
                "-p".to_owned(),
                vec![
                    json!({"type": "system"}),
                    json!({"type": "control_response", "response": usage}),
                ],
            )]),
        );
    }

    #[test]
    fn each_added_folder_is_its_own_account_with_its_own_windows() {
        let mut host = installed(limits());
        let work = host.home().join("claude-work");
        let mut usage = limits();
        usage["response"]["rate_limits"]["five_hour"]["utilization"] = json!(75.0);
        added(&mut host, &work, "work", usage);

        let reading = read(&host).unwrap();
        let started = host.started.borrow();
        assert_eq!(started.len(), 2);
        let entrypoint = ("CLAUDE_CODE_ENTRYPOINT".to_owned(), "agent-hud".to_owned());
        // The default folder's engine keeps the user's own environment.
        assert_eq!(started[0].3, std::slice::from_ref(&entrypoint));
        assert_eq!(
            started[1].3,
            [
                entrypoint,
                (
                    "CLAUDE_CONFIG_DIR".to_owned(),
                    work.to_string_lossy().into_owned()
                )
            ]
        );
        assert_eq!(reading.account.unwrap()["label"], "me@example.com");
        assert_eq!(reading.additional_accounts.len(), 1);
        assert_eq!(reading.additional_accounts[0]["label"], "work@example.com");
        assert_eq!(reading.additional_accounts[0]["plan"], "pro");
        assert_eq!(reading.quotas.len(), 4);
        assert_eq!(reading.quotas[0]["usedPercent"], 12.0);
        assert_eq!(reading.quotas[2]["usedPercent"], 75.0);
        let id = account_id("claude", &["work", "work-org"]);
        assert_eq!(reading.quotas[2]["id"], format!("{id}:claude-session"));
        assert_ne!(reading.quotas[0]["id"], reading.quotas[2]["id"]);
        assert_ne!(
            reading.quotas[0]["accountId"],
            reading.quotas[2]["accountId"]
        );
        assert!(reading.notice.is_none());
    }

    #[test]
    fn an_added_folder_never_reads_the_desktop_apps_sign_in() {
        // The default folder is signed out and reads through the desktop
        // app's sign-in; a signed-out added folder stays signed out.
        let mut host = signed_out_with_desktop()
            .file(
                "/u/.claude.json",
                &json!({"oauthAccount": {
                    "accountUuid": "user-1", "organizationUuid": "0a1b-2c3d",
                    "emailAddress": "me@example.com", "organizationType": "claude_max",
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
        let work = host.home().join("claude-work");
        added(
            &mut host,
            &work,
            "work",
            json!({"subtype": "success", "response": {"rate_limits_available": false}}),
        );
        host.engine_output_by_home
            .get_mut(&work)
            .unwrap()
            .insert("auth".into(), vec![json!({"loggedIn": false})]);
        let signed_out =
            "Claude Code is signed out. Run claude to sign in again to update the quota";

        let reading = read(&host).unwrap();
        assert_eq!(reading.account.unwrap()["label"], "me@example.com");
        assert_eq!(reading.quotas.len(), 3);
        assert!(reading.additional_accounts.is_empty());
        assert_eq!(
            host.asked.borrow().len(),
            1,
            "only the default folder asks claude.ai"
        );
        assert_eq!(
            reading.notice,
            Some(format!("{}: {signed_out}", work.display()))
        );
        {
            let started = host.started.borrow();
            assert_eq!(started.len(), 4);
            assert_eq!(started[3].1, ["auth", "status", "--json"]);
            assert!(started[3].3.contains(&(
                "CLAUDE_CONFIG_DIR".to_owned(),
                work.to_string_lossy().into_owned()
            )));
        }

        // Without the desktop app's sign-in both folders are signed out.
        host.cookies.clear();
        let error = read(&host).err().unwrap();
        assert!(matches!(error, Unread::SignedOut(_)));
        assert_eq!(error.reason().matches(signed_out).count(), 2);
        assert!(error.reason().contains(&*work.to_string_lossy()));
        assert_eq!(host.asked.borrow().len(), 1);
    }

    #[test]
    fn the_default_folder_added_again_is_read_once() {
        let mut host = installed(limits());
        let default = host.home().join(".claude");
        let alias = host.home().join("claude-alias");
        host.canonical_paths.insert(alias.clone(), default.clone());
        host.account_homes
            .insert("claude".into(), vec![default, alias]);
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 2);
        assert!(reading.notice.is_none());
        assert_eq!(host.started.borrow().len(), 1);
        assert_eq!(
            host.started.borrow()[0].3.len(),
            1,
            "the default folder's engine gets no CLAUDE_CONFIG_DIR"
        );

        // A CLAUDE_CONFIG_DIR of the user's own is the default folder too.
        let mut host = installed(limits());
        let configured = host.home().join("claude-config");
        host.env.insert(
            "CLAUDE_CONFIG_DIR".into(),
            configured.to_string_lossy().into_owned(),
        );
        host.account_homes.insert("claude".into(), vec![configured]);
        assert_eq!(read(&host).unwrap().quotas.len(), 2);
        assert_eq!(host.started.borrow().len(), 1);
    }

    #[test]
    fn the_same_account_in_two_folders_is_not_duplicated() {
        let mut host = installed(limits());
        let work = host.home().join("claude-work");
        added(&mut host, &work, "work", limits());
        let profile = host.files[&host.home().join(".claude.json")].clone();
        host.files.insert(work.join(".claude.json"), profile);
        let reading = read(&host).unwrap();
        assert_eq!(host.started.borrow().len(), 2);
        assert_eq!(reading.quotas.len(), 2);
        assert!(reading.additional_accounts.is_empty());
    }

    #[test]
    fn an_added_folder_without_an_account_is_that_folders() {
        let mut host = installed(limits());
        let work = host.home().join("claude-work");
        added(&mut host, &work, "work", limits());
        host.files.remove(&work.join(".claude.json"));
        let reading = read(&host).unwrap();
        let key = client_home_key(&host, &work, &host.home().join(".claude"));
        assert!(!key.is_empty());
        assert_eq!(
            reading.additional_accounts[0]["id"],
            legacy_unresolved("Claude", &key).as_str()
        );
        let id = account_id("claude", &["unresolved", &work.to_string_lossy()]);
        assert_eq!(reading.quotas[2]["id"], format!("{id}:claude-session"));
    }

    /// A usage answer shaped as Claude Code records it: windows the plan
    /// has, the ones it does not as null, extra usage, and `limits[]` with a
    /// model-scoped week and one scoped to a surface.
    fn usage() -> Value {
        let window = |used: f64, reset: &str| {
            json!({"utilization": used, "resets_at": reset, "limit_dollars": null,
                   "used_dollars": null, "remaining_dollars": null, "locked_reason": null})
        };
        json!({
            "five_hour": window(21.0, "2026-11-04T15:00:00.000000+00:00"),
            "seven_day": window(63.0, "2026-11-08T12:00:00.000000+00:00"),
            "seven_day_opus": null, "seven_day_sonnet": null, "seven_day_cowork": null,
            "extra_usage": {"is_enabled": true, "monthly_limit": 5000, "used_credits": 1200,
                            "utilization": 24.0, "currency": "USD", "disabled_reason": null},
            "limits": [
                {"kind": "session", "group": "session", "percent": 21, "severity": "normal",
                 "resets_at": "2026-11-04T15:00:00.000000+00:00", "scope": null, "is_active": true},
                {"kind": "weekly_all", "group": "weekly", "percent": 63, "severity": "normal",
                 "resets_at": "2026-11-08T12:00:00.000000+00:00", "scope": null, "is_active": true},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 40, "severity": "normal",
                 "resets_at": "2026-11-08T12:00:00.000000+00:00", "is_active": true,
                 "scope": {"model": {"id": "claude-fable-5", "display_name": "Fable"}, "surface": null}},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 12, "severity": "normal",
                 "resets_at": null, "is_active": true,
                 "scope": {"model": null, "surface": {"display_name": "Cowork"}}}
            ],
            "spend": {"used": {"amount_minor": 0, "currency": "USD", "exponent": 2}, "percent": 0},
        })
    }

    /// Claude Code's record of `utilization`, fetched `age_ms` ago, written
    /// into the `.claude.json` at `file` for the account that file names (or
    /// for `account`).
    fn record(host: &mut FakeHost, file: &str, age_ms: i64, account: Option<&str>) -> i64 {
        let fetched = chrono::Utc::now().timestamp_millis() - age_ms;
        let mut config = read_json(host, Path::new(file)).unwrap();
        let account = account
            .map(Value::from)
            .unwrap_or_else(|| config["oauthAccount"]["accountUuid"].clone());
        config["cachedUsageUtilization"] =
            json!({"fetchedAtMs": fetched, "accountUuid": account, "utilization": usage()});
        host.files
            .insert(file.into(), Answer::Found(config.to_string().into_bytes()));
        fetched
    }

    fn signed_out() -> FakeHost {
        let mut host =
            installed(json!({"subtype": "success", "response": {"rate_limits_available": false}}));
        host.engine_output
            .insert("auth".into(), vec![json!({"loggedIn": false})]);
        host
    }

    const SIGNED_OUT: &str =
        "Claude Code is signed out. Run claude to sign in again to update the quota";

    #[test]
    fn a_fresh_record_stands_for_the_engine_without_starting_it() {
        let mut host = installed(limits());
        let fetched = record(&mut host, "/u/.claude.json", 10_000, None);
        let reading = read(&host).unwrap();
        assert!(host.started.borrow().is_empty(), "no engine was started");
        assert!(reading.failure.is_none());
        let id = account_id("claude", &["user-1", "org-1"]);
        let identity = legacy_account("Claude", "user-1", "org-1").unwrap();
        let ids = reading
            .quotas
            .iter()
            .map(|row| row["id"].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            [
                format!("{id}:claude-session"),
                format!("{id}:claude-weekly"),
                format!("{id}:claude-weekly-fable")
            ],
            "null windows, extra usage and a week scoped to a surface are left out"
        );
        let used = reading
            .quotas
            .iter()
            .map(|row| row["usedPercent"].clone())
            .collect::<Vec<_>>();
        assert_eq!(used, [21.0, 63.0, 40.0]);
        for row in &reading.quotas {
            assert_eq!(
                row["observedAt"],
                crate::iso(fetched),
                "read when Claude Code read it"
            );
            assert_eq!(row["accountId"], identity.as_str());
        }
        assert_eq!(
            reading.quotas[2]["legacyWindowId"],
            format!("{identity}/claude-weekly-fable")
        );
        let account = reading.account.unwrap();
        assert_eq!(account["id"], identity.as_str());
        assert_eq!(account["label"], "me@example.com");
        assert_eq!(account["plan"], "max_20x");
        assert_eq!(account["checkedAt"], crate::iso(fetched));
    }

    #[test]
    fn the_record_and_the_engine_name_each_window_alike() {
        let mut engine = limits();
        engine["response"]["rate_limits"] = usage();
        let engine = read(&installed(engine)).unwrap();
        let mut host = installed(limits());
        record(&mut host, "/u/.claude.json", 0, None);
        let recorded = read(&host).unwrap();
        let rows = |reading: &Reading| {
            reading
                .quotas
                .iter()
                .map(|row| {
                    let mut row = row.clone();
                    row.as_object_mut().unwrap().remove("observedAt");
                    row
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(rows(&recorded), rows(&engine));
        assert_eq!(
            recorded.account.unwrap()["id"],
            engine.account.unwrap()["id"]
        );
    }

    #[test]
    fn a_minute_old_record_or_another_accounts_is_not_taken_for_a_reading() {
        let mut old = installed(limits());
        record(&mut old, "/u/.claude.json", usage_record::FRESH_MS, None);
        assert_eq!(read(&old).unwrap().quotas[0]["usedPercent"], 12.0);
        assert_eq!(old.started.borrow().len(), 1, "the engine is asked");

        let mut other = installed(limits());
        record(&mut other, "/u/.claude.json", 0, Some("user-2"));
        let reading = read(&other).unwrap();
        assert_eq!(reading.quotas[0]["usedPercent"], 12.0);
        assert!(reading.quotas[0]["observedAt"].is_null());
        assert_eq!(other.started.borrow().len(), 1);
    }

    #[test]
    fn a_signed_out_claude_code_shows_its_last_record_and_stays_signed_out() {
        let mut host = signed_out();
        let fetched = record(&mut host, "/u/.claude.json", 2 * 3_600_000, None);
        let reading = read(&host).unwrap();
        let failure = reading.failure.as_ref().unwrap();
        assert!(
            matches!(failure, Unread::SignedOut(_)),
            "still told to sign in"
        );
        assert_eq!(
            failure.reason(),
            format!("{SIGNED_OUT} · Showing Claude Code's own reading from 2h ago")
        );
        assert_eq!(reading.quotas.len(), 3);
        assert_eq!(reading.quotas[0]["observedAt"], crate::iso(fetched));
        assert_eq!(
            reading.account.as_ref().unwrap()["checkedAt"],
            crate::iso(fetched)
        );
        assert_eq!(host.started.borrow()[1].1, ["auth", "status", "--json"]);

        // Another account's record is not shown for this one.
        let mut other = signed_out();
        record(&mut other, "/u/.claude.json", 60_000, Some("user-2"));
        let error = read(&other).err().unwrap();
        assert!(matches!(error, Unread::SignedOut(_)));
        assert_eq!(error.reason(), SIGNED_OUT);
    }

    #[test]
    fn a_failure_that_passes_keeps_its_kind_beside_the_record() {
        let mut silent = installed(limits());
        silent.engine_output.clear();
        record(&mut silent, "/u/.claude.json", 30 * 60_000, None);
        let failure = read(&silent).unwrap().failure.unwrap();
        assert!(matches!(failure, Unread::Transient { .. }));
        assert_eq!(
            failure.reason(),
            "Claude Code engine query failed: The engine returned no usage within 40 s · Showing Claude Code's own reading from 30m ago"
        );

        // Signed out, and claude.ai asked the desktop app's sign-in for a
        // browser check.
        let mut challenged = signed_out_with_desktop()
            .file(
                "/u/AppData/Claude/config.json",
                &json!({"lastKnownAccountUuid": "user-1"}).to_string(),
            )
            .answer(
                "https://claude.ai/api/account",
                Err(HttpFailure::Challenged),
            );
        record(&mut challenged, "/u/.claude.json", 5 * 86_400_000, None);
        let reading = read(&challenged).unwrap();
        let failure = reading.failure.unwrap();
        assert!(matches!(failure, Unread::SignedOut(_)));
        assert!(failure
            .reason()
            .starts_with("Claude Code is signed out and claude.ai asked for a browser check"));
        assert!(failure
            .reason()
            .ends_with(" · Showing Claude Code's own reading from 5d ago"));
        assert_eq!(reading.quotas.len(), 3);
    }

    #[test]
    fn a_failed_folder_shows_its_record_stale_beside_the_folders_that_read() {
        let mut host = installed(limits());
        let work = host.home().join("claude-work");
        added(
            &mut host,
            &work,
            "work",
            json!({"subtype": "success", "response": {"rate_limits_available": false}}),
        );
        host.engine_output_by_home
            .get_mut(&work)
            .unwrap()
            .insert("auth".into(), vec![json!({"loggedIn": false})]);
        let file = work.join(".claude.json");
        let fetched = record(&mut host, &file.to_string_lossy(), 3_600_000, None);
        let reading = read(&host).unwrap();
        assert!(reading.failure.is_none(), "the default folder read");
        assert_eq!(reading.quotas.len(), 5);
        assert_eq!(reading.quotas[0]["status"], "normal");
        let id = account_id("claude", &["work", "work-org"]);
        assert_eq!(reading.quotas[2]["id"], format!("{id}:claude-session"));
        assert_eq!(reading.quotas[2]["status"], "stale");
        assert_eq!(reading.quotas[2]["observedAt"], crate::iso(fetched));
        let work_account = &reading.additional_accounts[0];
        assert_eq!(work_account["stale"], true);
        assert_eq!(work_account["checkedAt"], crate::iso(fetched));
        assert_eq!(
            reading.notice,
            Some(format!(
                "{}: {SIGNED_OUT} · Showing Claude Code's own reading from 1h ago",
                work.display()
            ))
        );

        // When every folder fails, the records come with the failures.
        let mut both = signed_out();
        added(
            &mut both,
            &work,
            "work",
            json!({"subtype": "success", "response": {"rate_limits_available": false}}),
        );
        both.engine_output_by_home
            .get_mut(&work)
            .unwrap()
            .insert("auth".into(), vec![json!({"loggedIn": false})]);
        record(&mut both, &file.to_string_lossy(), 3_600_000, None);
        let reading = read(&both).unwrap();
        let failure = reading.failure.unwrap();
        assert!(matches!(failure, Unread::SignedOut(_)));
        assert_eq!(failure.reason().matches(SIGNED_OUT).count(), 2);
        assert_eq!(reading.quotas.len(), 3, "only the folder with a record");
        assert_eq!(
            reading.quotas[0]["status"], "normal",
            "stale once the engine files the failure"
        );
        assert_eq!(reading.account.unwrap()["label"], "work@example.com");
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
