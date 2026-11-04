//! Claude's plan limits through Claude Code's own engine, which answers get_usage.
use super::account::{profile, Folder};
use super::quota_windows::{identity, plan, plan_reading, windows};
use crate::account::{EngineStart, Host, Reading, Unread};
use crate::l10n;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

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
pub(super) enum EngineUnread {
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

/// The engine's reading of `folder`, whose account was `before` when it started.
pub(super) fn from_engine(
    host: &dyn Host,
    folder: &Folder,
    before: &Value,
) -> Result<Reading, EngineUnread> {
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
