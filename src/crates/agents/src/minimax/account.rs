//! MiniMax's plan quota, read with the plan keys other clients already hold
//! (`routes`). MiniMax answers `GET /v1/token_plan/remains` on each platform
//! (its M Plan FAQ, checked 2026-11-04); older Coding Plan keys are answered
//! at `/v1/api/openplatform/coding_plan/remains`, tried when the first is
//! not there. The answer's `model_remains` lists each model family's 5-hour
//! and weekly windows. Endpoint order, the field meanings (a count named
//! `usage_count` is what remains) and the service names follow CodexBar's
//! MiniMax reader (MIT, 897bff3).
use super::routes::{self, Key, Region};
use crate::account::{
    account_id, status, Host, HttpFailure, HttpRequest, Provider, Reading, Unread,
};
use crate::l10n;
use crate::naming::window_names::Period;
use serde_json::{json, Value};
use std::time::Duration;

/// Work in Claude Code, Codex, OpenCode and Pi spends the plan, as the
/// clients its keys are found in.
pub static PROVIDER: Provider = Provider {
    // The plan is also spent from clients whose sessions are not read here
    // (Cursor, TRAE, the vendor's own app) and from other devices, so it is
    // read on the account interval whatever this computer does.
    local_work: false,
    work_sources: &["claude", "codex", "opencode", "pi"],
    ..Provider::new("minimax", read)
};

/// The usage endpoints of a platform, in the order they are tried.
fn endpoints(region: Region) -> [&'static str; 2] {
    match region {
        Region::Global => [
            "https://www.minimax.io/v1/token_plan/remains",
            "https://api.minimax.io/v1/api/openplatform/coding_plan/remains",
        ],
        Region::China => [
            "https://www.minimax.cn/v1/token_plan/remains",
            "https://api.minimaxi.com/v1/api/openplatform/coding_plan/remains",
        ],
    }
}

fn unreachable() -> String {
    l10n::text(
        "无法连接 MiniMax 额度服务",
        "MiniMax plan quota could not be reached",
    )
}
fn refused() -> String {
    l10n::text(
        "MiniMax 拒绝了现有的套餐 Key，请检查来源客户端里的 M Plan API Key",
        "MiniMax refused the existing plan key; check the M Plan API key in the source client",
    )
}
fn no_plan() -> String {
    l10n::text(
        "这个 MiniMax Key 没有生效中的套餐",
        "This MiniMax key has no active plan",
    )
}
fn no_usage() -> String {
    l10n::text(
        "MiniMax 没有返回可核实的套餐额度",
        "MiniMax did not return a verifiable plan quota",
    )
}

/// The plan's quota, one account per key.
pub fn read(host: &dyn Host) -> Result<Reading, Unread> {
    let keys = routes::keys(host);
    let mut reading = Reading::empty("minimax");
    if keys.is_empty() {
        reading.notice = Some(l10n::text(
            "现有客户端配置中没有可用的 MiniMax 套餐 Key",
            "No usable MiniMax plan key was found in the existing client configuration",
        ));
        return Ok(reading);
    }
    let mut failures = Vec::new();
    for key in &keys {
        if host.cancelled() {
            return Err(l10n::text("账户读取已停止", "Account reading was stopped").into());
        }
        let result = read_key(host, key);
        // Every client configured with the key reaches the plan on its
        // platform, read or not; the platform picks the models' price list.
        let region = match &result {
            Ok(Some((_, region))) => Some(*region),
            _ => key.region,
        };
        let id = region.map(|region| account_id("minimax", &[region.name(), &key.key]));
        if let (Some(region), Some(id)) = (region, &id) {
            let mut clients = key.clients.clone();
            clients.sort();
            for client in clients {
                reading.services.push(json!({"client":client,"provider":"MiniMax","product":"plan","accountId":id,"region":region.name()}));
            }
        }
        match result {
            Ok(Some((parsed, _))) => {
                let id = id.unwrap_or_default();
                let account = json!({"id":id,"provider":"minimax","label":parsed.plan,"plan":parsed.plan,"clients":key.clients,"identityEvidence":"credential-scope"});
                for window in parsed.windows {
                    reading.quotas.push(window.row(&id, parsed.plan.as_deref()));
                }
                if reading.account.is_none() {
                    reading.account = Some(account);
                } else {
                    reading.additional_accounts.push(account);
                }
            }
            // A key MiniMax bills by use has no plan to read: nothing failed.
            Ok(None) => {}
            Err(unread) => failures.push(unread),
        }
    }
    if reading.account.is_none() {
        return match Unread::combined(failures) {
            Some(why) => Err(why),
            None => {
                reading.notice = Some(no_plan());
                Ok(reading)
            }
        };
    }
    if !failures.is_empty() {
        reading.notice = Some(if l10n::chinese() {
            format!("有 {} 个已配置的 MiniMax 套餐读取失败", failures.len())
        } else {
            format!(
                "{} configured MiniMax plan(s) could not be read",
                failures.len()
            )
        });
    } else if reading.quotas.is_empty() {
        reading.notice = Some(l10n::text(
            "MiniMax 套餐目前没有限额窗口",
            "The MiniMax plan reports no limited window right now",
        ));
    }
    Ok(reading)
}

/// One key's answer and the platform that gave it, or None when MiniMax says
/// the key has no active plan. A key is tried on its own platform, or on both
/// in turn when that is not known; the next platform is asked only when this
/// one refused the key.
fn read_key(host: &dyn Host, key: &Key) -> Result<Option<(Parsed, Region)>, Unread> {
    let regions = match key.region {
        Some(region) => vec![region],
        None => vec![Region::Global, Region::China],
    };
    let authorization = format!("Bearer {}", key.key);
    let now = chrono::Utc::now().timestamp_millis();
    let mut status = None;
    let mut refused_any = false;
    for region in regions {
        let mut refused_here = false;
        let mut missed = None;
        for url in endpoints(region) {
            let answer = host.http_json(&HttpRequest {
                url,
                secrets: &[("Authorization", &authorization)],
                headers: &[("Content-Type", "application/json")],
                body: None,
                timeout: Duration::from_secs(15),
            });
            match answer {
                Ok(body) => match parse(&body, now) {
                    Ok(parsed) => return Ok(Some((parsed, region))),
                    Err(Answer::Refused) => refused_here = true,
                    Err(Answer::Status(code)) => {
                        status.get_or_insert(code);
                    }
                    Err(Answer::Unrecognized) => {}
                },
                Err(HttpFailure::Status(401 | 403)) => refused_here = true,
                // Not this endpoint: the next one may be.
                Err(HttpFailure::Status(302 | 404 | 405) | HttpFailure::NotJson) => {}
                Err(failure @ HttpFailure::Unreachable) => missed = Some(failure),
                Err(failure) => return Err(failure.unread()),
            }
        }
        if !refused_here {
            if let Some(failure) = missed {
                return Err(failure.unread_as(unreachable()));
            }
            break;
        }
        refused_any = true;
    }
    if status == Some(2062) {
        return Ok(None);
    }
    Err(match status {
        Some(code) => if l10n::chinese() {
            format!("MiniMax 额度服务返回状态 {code}")
        } else {
            format!("MiniMax plan quota returned status {code}")
        }
        .into(),
        None if refused_any => refused().into(),
        None => no_usage().into(),
    })
}

/// Why an answer is not a quota.
#[derive(Debug, PartialEq)]
enum Answer {
    /// The key is not accepted here.
    Refused,
    /// A `base_resp` status other than success.
    Status(i64),
    /// No `model_remains`.
    Unrecognized,
}

/// What a plan's answer says: its name and its windows.
#[derive(Debug)]
struct Parsed {
    plan: Option<String>,
    windows: Vec<Window>,
}

#[derive(Debug)]
struct Window {
    key: String,
    name: String,
    short: String,
    used: f64,
    reset: Option<i64>,
    duration: f64,
    all_models: bool,
    /// Prompts used and allowed, where the plan counts them.
    counts: Option<(f64, f64)>,
}
impl Window {
    fn row(&self, account: &str, plan: Option<&str>) -> Value {
        let mut row = json!({"id":format!("{account}:{}",self.key),"vendor":"minimax","name":self.name,"shortLabel":self.short,"usedPercent":self.used,"resetAt":self.reset.and_then(iso),"status":status(self.used),"account":plan,"accountId":account,"allModels":self.all_models,"windowDurationSeconds":self.duration});
        if let Some((used, capacity)) = self.counts {
            row["nativeUsage"] = json!({"used":used,"capacity":capacity,"unit":"prompts"});
        }
        row
    }
}

fn iso(ms: i64) -> Option<String> {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms).map(|at| at.to_rfc3339())
}

/// A number, or a number written as a string ("96", "12%").
fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| {
            let text = value.as_str()?.trim();
            text.strip_suffix('%').unwrap_or(text).trim().parse().ok()
        })
        .filter(|n: &f64| n.is_finite())
}

/// An instant in epoch seconds or milliseconds, as milliseconds.
fn instant(value: &Value) -> Option<i64> {
    let raw = number(value)?;
    if raw > 1e12 {
        Some(raw as i64)
    } else if raw > 1e9 {
        Some((raw * 1000.0) as i64)
    } else {
        None
    }
}

/// A model family's name in MiniMax's words and the word a short name
/// keeps; None for the plan's text models.
fn service(model: &str) -> Option<(String, String)> {
    let lower = model.trim().to_lowercase();
    let named = |name: &str, word: &str| Some((name.to_owned(), word.to_owned()));
    if text_model(&lower) {
        None
    } else if lower == "video" {
        named("Video", "Video")
    } else if lower.contains("speech") {
        named("Text to Speech", "Speech")
    } else if lower.contains("hailuo") && lower.contains("fast") {
        named("Image to Video", "Video")
    } else if lower.contains("hailuo") {
        named("Text to Video", "Video")
    } else if lower.starts_with("image-") {
        named("Image Generation", "Image")
    } else if lower.contains("music") {
        named("Music Generation", "Music")
    } else {
        let word = model.split_whitespace().next().unwrap_or(model);
        named(model.trim(), word)
    }
}

/// The plan's text models: `general`, and the M series by name in older
/// answers.
fn text_model(lower: &str) -> bool {
    lower == "general" || lower.contains("minimax-m") || lower.starts_with("m2.")
}

/// The 5-hour (`current_interval_*`) or weekly (`current_weekly_*`) window of
/// one `model_remains` entry.
struct Lane {
    prefix: &'static str,
    start: &'static str,
    end: &'static str,
    remains: &'static str,
    key: &'static str,
    default_seconds: f64,
}
const LANES: [Lane; 2] = [
    Lane {
        prefix: "current_interval",
        start: "start_time",
        end: "end_time",
        remains: "remains_time",
        key: "interval",
        default_seconds: 18_000.0,
    },
    Lane {
        prefix: "current_weekly",
        start: "weekly_start_time",
        end: "weekly_end_time",
        remains: "weekly_remains_time",
        key: "weekly",
        default_seconds: 604_800.0,
    },
];

fn parse(root: &Value, now: i64) -> Result<Parsed, Answer> {
    let data = if root["data"].is_object() {
        &root["data"]
    } else {
        root
    };
    let base = if data["base_resp"].is_object() {
        &data["base_resp"]
    } else {
        &root["base_resp"]
    };
    if let Some(code) = number(&base["status_code"]).filter(|code| *code != 0.0) {
        let message = base["status_msg"].as_str().unwrap_or("").to_lowercase();
        if code == 1004.0
            || ["invalid api key", "login", "log in", "cookie"]
                .iter()
                .any(|word| message.contains(word))
        {
            return Err(Answer::Refused);
        }
        return Err(Answer::Status(code as i64));
    }
    let remains = data["model_remains"]
        .as_array()
        .filter(|list| !list.is_empty())
        .ok_or(Answer::Unrecognized)?;
    let plan = [
        &data["current_subscribe_title"],
        &data["plan_name"],
        &data["combo_title"],
        &data["current_plan_title"],
        &data["current_combo_card"]["title"],
    ]
    .iter()
    .filter_map(|value| value.as_str().map(str::trim))
    .find(|name| !name.is_empty())
    .map(str::to_owned);
    let texts = remains
        .iter()
        .filter(|item| {
            item["model_name"]
                .as_str()
                .is_some_and(|model| text_model(&model.trim().to_lowercase()))
        })
        .count();
    let mut windows = Vec::new();
    for item in remains {
        let Some(model) = item["model_name"].as_str().filter(|m| !m.trim().is_empty()) else {
            continue;
        };
        let lower = model.trim().to_lowercase();
        // A second text family is named by its model, as any other.
        let named = service(model).or_else(|| {
            (texts > 1 && lower != "general")
                .then(|| (model.trim().to_owned(), model.trim().to_owned()))
        });
        for lane in &LANES {
            let field = |name: &str| &item[format!("{}_{name}", lane.prefix)];
            let total = number(field("total_count"));
            // CodexBar: the "usage" count is what is left.
            let left = number(field("usage_count"));
            let remaining_percent = number(field("remaining_percent")).or_else(|| {
                let used = number(field("used_percent")).filter(|used| *used >= 0.0)?;
                let limit = number(field("total_percent"))
                    .filter(|n| *n > 0.0)
                    .unwrap_or(100.0);
                Some((100.0 - used / limit * 100.0).clamp(0.0, 100.0))
            });
            let counted = total.filter(|total| *total > 0.0).zip(left);
            // Status 3 with nothing to count is a lane the plan leaves out
            // or does not limit.
            if number(field("status")) == Some(3.0)
                && counted.is_none()
                && remaining_percent.is_none_or(|left| left >= 100.0)
            {
                continue;
            }
            let (used, counts) = match (remaining_percent, counted) {
                (Some(left), _) => ((100.0 - left).clamp(0.0, 100.0), None),
                (None, Some((total, left))) => {
                    let used = (total - left).max(0.0);
                    (
                        (used / total * 100.0).clamp(0.0, 100.0),
                        Some((used, total)),
                    )
                }
                (None, None) => continue,
            };
            let start = instant(&item[lane.start]);
            let end = instant(&item[lane.end]);
            let duration = start
                .zip(end)
                .map(|(start, end)| (end - start) as f64 / 1000.0)
                .filter(|seconds| *seconds >= 60.0)
                .unwrap_or(lane.default_seconds);
            let reset = end.filter(|end| *end > now).or_else(|| {
                let left = number(&item[lane.remains]).filter(|n| *n > 0.0)?;
                // Milliseconds, as MiniMax writes it; a value that fits in
                // the window as seconds is seconds.
                let seconds = if left > duration { left / 1000.0 } else { left };
                Some(now + (seconds * 1000.0) as i64)
            });
            let period = Period::from_seconds(Some(duration));
            let window = match period {
                Some(Period::FiveHours) => "5-hour window".to_owned(),
                Some(Period::Week) => "Weekly window".to_owned(),
                Some(Period::Day) => "Daily window".to_owned(),
                Some(Period::Hours(hours)) => format!("{hours}-hour window"),
                Some(Period::Days(days)) => format!("{days}-day window"),
                _ => "Usage window".to_owned(),
            };
            let short = period.map_or_else(|| "Usage".to_owned(), Period::short_name);
            let (name, short) = match &named {
                None => (window, short),
                Some((name, word)) => (
                    format!("{name} · {window}"),
                    period.map_or_else(|| word.clone(), |p| format!("{word} {}", p.after_word())),
                ),
            };
            windows.push(Window {
                key: format!("{lower}:{}", lane.key),
                name,
                short,
                used,
                reset,
                duration,
                all_models: named.is_none(),
                counts,
            });
        }
    }
    Ok(Parsed { plan, windows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::fake::FakeHost;

    const GLOBAL: &str = "https://www.minimax.io/v1/token_plan/remains";
    const GLOBAL_OLD: &str = "https://api.minimax.io/v1/api/openplatform/coding_plan/remains";
    const CHINA: &str = "https://www.minimax.cn/v1/token_plan/remains";
    const CLAUDE_CHINA: &str = r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.minimax.cn/anthropic","ANTHROPIC_AUTH_TOKEN":"sk-cp-cn"}}"#;

    /// A Token Plan answer in percentages, as CodexBar's fixture
    /// `token-plan-normal.json` records one, with a video family beside it.
    fn percentages() -> Value {
        let now = chrono::Utc::now().timestamp_millis();
        json!({"base_resp": {"status_code": 0, "status_msg": "success"}, "data": {
        "current_subscribe_title": "Token Plan Plus",
        "model_remains": [
            {"model_name": "general",
             "current_interval_total_count": 0, "current_interval_usage_count": 0,
             "current_interval_remaining_percent": "96",
             "start_time": now - 3_600_000, "end_time": now + 14_400_000,
             "current_weekly_total_count": 0, "current_weekly_usage_count": 0,
             "current_weekly_remaining_percent": "99",
             "weekly_start_time": now - 86_400_000, "weekly_end_time": now + 518_400_000},
            {"model_name": "video",
             "current_interval_status": 3, "current_interval_remaining_percent": 100,
             "current_weekly_remaining_percent": 70}
        ]}})
    }

    #[test]
    fn a_plan_in_percentages_reads_its_windows() {
        let host = FakeHost::new("/u")
            .file("/u/.claude/settings.json", CLAUDE_CHINA)
            .answer(CHINA, Ok(percentages()));
        let reading = read(&host).unwrap();
        let id = account_id("minimax", &["china", "sk-cp-cn"]);
        let account = reading.account.unwrap();
        assert_eq!(account["id"], id.as_str());
        assert_eq!(account["plan"], "Token Plan Plus");
        assert_eq!(reading.quotas.len(), 3, "video has no 5-hour window");
        let five = &reading.quotas[0];
        assert_eq!(five["id"], format!("{id}:general:interval"));
        assert_eq!(five["name"], "5-hour window");
        assert_eq!(five["shortLabel"], "5h");
        assert_eq!(five["usedPercent"], 4.0);
        assert_eq!(five["windowDurationSeconds"], 18000.0);
        assert_eq!(five["allModels"], true);
        assert!(five["resetAt"].is_string());
        assert_eq!(reading.quotas[1]["name"], "Weekly window");
        assert_eq!(reading.quotas[1]["usedPercent"], 1.0);
        let video = &reading.quotas[2];
        assert_eq!(video["name"], "Video · Weekly window");
        assert_eq!(video["shortLabel"], "Video 7d");
        assert_eq!(video["usedPercent"], 30.0);
        assert_eq!(video["allModels"], false);
        assert_eq!(reading.services[0]["provider"], "MiniMax");
        assert_eq!(reading.services[0]["client"], "Claude");
        assert_eq!(reading.services[0]["region"], "china");
        let asked = host.asked.borrow();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0], (CHINA.into(), Some("Bearer sk-cp-cn".into())));
    }

    #[test]
    fn counts_left_are_read_as_prompts_used() {
        let root = json!({"model_remains": [{"model_name": "MiniMax-M2",
            "current_interval_total_count": 100, "current_interval_usage_count": 25,
            "remains_time": 7_200_000}]});
        let parsed = parse(&root, 1_000_000_000_000).unwrap();
        assert_eq!(parsed.windows.len(), 1);
        let window = &parsed.windows[0];
        assert_eq!(window.used, 75.0, "25 of 100 prompts are left");
        assert_eq!(window.counts, Some((75.0, 100.0)));
        assert_eq!(
            window.reset,
            Some(1_000_007_200_000),
            "remains_time is in ms"
        );
        assert_eq!(window.name, "5-hour window");
    }

    #[test]
    fn used_and_total_percentages_and_a_boost_read_against_the_boost() {
        let root = json!({"data": {"model_remains": [{"model_name": "general",
            "current_weekly_used_percent": "75%", "current_weekly_total_percent": "150%"}]}});
        let parsed = parse(&root, 0).unwrap();
        assert_eq!(parsed.windows.len(), 1);
        assert_eq!(parsed.windows[0].used, 50.0);
        assert_eq!(parsed.windows[0].duration, 604800.0);
    }

    #[test]
    fn refusals_and_statuses_are_told_apart() {
        let refused = json!({"base_resp": {"status_code": 1004, "status_msg": "login fail"}});
        assert_eq!(parse(&refused, 0).err(), Some(Answer::Refused));
        let none = json!({"base_resp": {"status_code": 2062, "status_msg": "no active token plan subscription"}});
        assert_eq!(parse(&none, 0).err(), Some(Answer::Status(2062)));
        assert_eq!(
            parse(&json!({"data": {}}), 0).err(),
            Some(Answer::Unrecognized)
        );
    }

    #[test]
    fn a_key_without_a_platform_is_tried_on_the_other_when_refused() {
        let host = FakeHost::new("/u")
            .env("MINIMAX_API_KEY", "sk-cp-k")
            .answer(GLOBAL, Err(HttpFailure::Status(401)))
            .answer(GLOBAL_OLD, Err(HttpFailure::Status(401)))
            .answer(CHINA, Ok(percentages()));
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 3);
        assert_eq!(
            reading.account.unwrap()["id"],
            account_id("minimax", &["china", "sk-cp-k"]).as_str()
        );
        let asked: Vec<_> = host
            .asked
            .borrow()
            .iter()
            .map(|(url, _)| url.clone())
            .collect();
        assert_eq!(asked, [GLOBAL, GLOBAL_OLD, CHINA]);
        assert_eq!(reading.services[0]["client"], "MiniMax");
    }

    #[test]
    fn an_older_plan_is_read_where_the_token_plan_is_not() {
        let host = FakeHost::new("/u")
            .file(
                "/u/.claude/settings.json",
                r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.minimax.io/anthropic","ANTHROPIC_AUTH_TOKEN":"sk-cp-g"}}"#,
            )
            .answer(GLOBAL, Err(HttpFailure::Status(404)))
            .answer(
                GLOBAL_OLD,
                Ok(json!({"model_remains": [{"model_name": "MiniMax-M2",
                    "current_interval_total_count": 300, "current_interval_usage_count": 300}]})),
            );
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas[0]["usedPercent"], 0.0);
        assert_eq!(reading.quotas[0]["nativeUsage"]["capacity"], 300.0);
        assert_eq!(reading.services[0]["region"], "international");
    }

    #[test]
    fn failures_keep_their_kind() {
        let key = |host: FakeHost| {
            host.file(
                "/u/.claude/settings.json",
                r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.minimax.io/anthropic","ANTHROPIC_AUTH_TOKEN":"sk-cp-g"}}"#,
            )
        };
        // No answer at all passes by itself.
        assert!(matches!(
            read(&key(FakeHost::new("/u"))).err(),
            Some(Unread::Transient { .. })
        ));
        let limited = key(FakeHost::new("/u")).answer(
            GLOBAL,
            Err(HttpFailure::Limited {
                retry_after: Some(90),
            }),
        );
        assert_eq!(
            read(&limited).err(),
            Some(Unread::Transient {
                reason: HttpFailure::Limited { retry_after: None }.message(),
                retry_after: Some(Duration::from_secs(90))
            })
        );
        let refusing = key(FakeHost::new("/u"))
            .answer(GLOBAL, Err(HttpFailure::Status(401)))
            .answer(GLOBAL_OLD, Err(HttpFailure::Status(403)));
        assert_eq!(read(&refusing).err().unwrap(), refused());
        let ended = key(FakeHost::new("/u"))
            .answer(
                GLOBAL,
                Ok(json!({"base_resp": {"status_code": 2062, "status_msg": "no active token plan subscription"}})),
            )
            .answer(GLOBAL_OLD, Err(HttpFailure::Status(404)));
        // A key with no plan is not a failed reading: nothing to show, and why.
        let reading = read(&ended).unwrap();
        assert!(reading.quotas.is_empty() && reading.account.is_none());
        assert_eq!(reading.notice, Some(no_plan()));
    }

    #[test]
    fn nothing_configured_leaves_a_notice_and_asks_nothing() {
        let host = FakeHost::new("/u");
        let reading = read(&host).unwrap();
        assert!(reading.notice.is_some());
        assert!(host.asked.borrow().is_empty());
    }

    #[test]
    fn one_unreadable_key_is_a_notice_beside_another() {
        let host = FakeHost::new("/u")
            .env("MINIMAX_CN_API_KEY", "sk-cp-cn")
            .file(
                "/u/.claude/settings.json",
                r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.minimax.io/anthropic","ANTHROPIC_AUTH_TOKEN":"sk-cp-g"}}"#,
            )
            .answer(CHINA, Ok(percentages()))
            .answer(GLOBAL, Err(HttpFailure::Status(401)))
            .answer(GLOBAL_OLD, Err(HttpFailure::Status(401)));
        let reading = read(&host).unwrap();
        assert_eq!(reading.quotas.len(), 3);
        assert_eq!(
            reading.notice.as_deref(),
            Some("1 configured MiniMax plan(s) could not be read")
        );
        assert_eq!(reading.services.len(), 2);
    }

    #[test]
    fn a_stopped_reading_asks_no_further() {
        let mut host = FakeHost::new("/u").env("MINIMAX_API_KEY", "sk-cp-k");
        host.cancelled = true;
        assert_eq!(read(&host).err().unwrap(), "Account reading was stopped");
        assert!(host.asked.borrow().is_empty());
    }
}
