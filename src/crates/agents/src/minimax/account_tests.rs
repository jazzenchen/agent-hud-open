use super::*;
use crate::account::fake::FakeHost;
use serde_json::Value;

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
