use super::projection::{reset_countdown, Level};
use super::*;
use serde_json::json;

fn titles(state: &Presentation) -> Vec<String> {
    state
        .entries
        .iter()
        .map(|entry| match entry {
            Entry::Separator => "—".into(),
            Entry::Item { title, value, .. } if value.is_empty() => title.clone(),
            Entry::Item { title, value, .. } => format!("{title} {value}"),
        })
        .collect()
}

#[test]
fn the_menu_lists_windows_then_balances_as_use_tray_did() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-21T12:00:00Z")
        .unwrap()
        .timestamp_millis();
    let snapshot = json!({
        "language": "en",
        "quotas": [
            {"vendor": "claude", "name": "5h", "usedPercent": 40.4, "status": "normal", "account": "me@work", "resetAt": "2026-10-21T14:05:00Z"},
            {"vendor": "claude", "name": "Week", "usedPercent": 72.5, "status": "warning", "account": "me@work", "resetAt": null},
            {"vendor": "codex", "name": "5h", "usedPercent": 95.0, "status": "stale", "account": null, "resetAt": null}
        ],
        "billing": [{"vendor": "deepseek", "amount": "8.85", "currency": "CNY", "level": "normal"}],
        "providers": [], "approvals": [], "sessions": []
    });
    let agents = crate::vendors::vendors(&snapshot, "simple", now);
    let state = project(&snapshot, &agents, true, false, false, None, now);
    assert_eq!(
        titles(&state),
        [
            "Claude",
            "me@work",
            "5h 40% · 2h 05m",
            "Week 73%",
            "—",
            "OpenAI",
            "5h 95%",
            "—",
            "DeepSeek · Balance ¥8.85"
        ]
    );
    // The highest reading that is not stale; one level per agent.
    assert_eq!(state.used_percent, Some(72.5));
    assert!(state.levels == [Level::Warning, Level::Stale, Level::Normal]);
    let paused = json!({"language": "zh", "status": {"paused": true}, "quotas": [], "billing": [], "approvals": [],
        "providers": [{"id": "kimi", "detected": true, "enabled": true}],
        "sessions": [{"id": "a", "source": "kimi", "state": "idle", "updatedAt": "2026-10-21T11:00:00Z"}]});
    let agents = crate::vendors::vendors(&paused, "simple", now);
    let state = project(&paused, &agents, true, true, false, None, now);
    assert_eq!(titles(&state), ["Kimi", "暂无额度读数 —"]);
    assert!(state.levels.is_empty() && state.glow_hidden && state.used_percent.is_none());
}

#[test]
fn remaining_display_changes_menu_figures_but_keeps_the_tightest_window_and_risk() {
    let snapshot = json!({
        "language": "en", "settings": {},
        "quotas": [
            {"vendor":"claude", "name":"5h", "usedPercent":0.0, "status":"normal"},
            {"vendor":"claude", "name":"Week", "usedPercent":92.0, "status":"critical"},
            {"vendor":"codex", "name":"5h", "usedPercent":null, "status":"stale"}
        ],
        "billing":[], "providers":[], "approvals":[], "sessions":[]
    });
    let agents = crate::vendors::vendors(&snapshot, "simple", 0);
    let consumed = project(&snapshot, &agents, true, false, false, None, 0);
    let mut remaining_snapshot = snapshot.clone();
    remaining_snapshot["settings"]["quotaDisplay"] = json!("remaining");
    let remaining = project(&remaining_snapshot, &agents, true, false, false, None, 0);
    assert_eq!(
        titles(&remaining),
        ["Claude", "5h 100%", "Week 8%", "—", "OpenAI", "5h —"]
    );
    assert!(remaining.remaining);
    assert_eq!(remaining.used_percent, Some(92.0));
    assert_eq!(remaining.used_percent, consumed.used_percent);
    assert!(remaining.levels == consumed.levels);
    assert_eq!(
        crate::vendors::quota_percent(remaining.used_percent.unwrap(), remaining.remaining),
        8.0
    );
}

#[test]
fn a_reset_within_a_day_counts_down() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-21T12:00:00Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(
        reset_countdown("2026-10-21T12:09:40Z", now).as_deref(),
        Some("9m")
    );
    assert_eq!(
        reset_countdown("2026-10-21T11:00:00Z", now).as_deref(),
        Some("0m")
    );
    assert_eq!(
        reset_countdown("2026-10-22T11:59:59Z", now).as_deref(),
        Some("23h 59m")
    );
    assert!(reset_countdown("2026-10-23T12:00:00Z", now).is_some_and(|text| text.len() == 9));
    assert_eq!(reset_countdown("soon", now), None);
}

#[test]
fn an_update_notice_redraws_the_menu() {
    let snapshot =
        json!({"language": "en", "quotas": [], "billing": [], "providers": [], "sessions": []});
    let none = project(&snapshot, &[], true, false, false, None, 0);
    let offered = project(&snapshot, &[], true, false, false, Some("0.6.1"), 0);
    assert_eq!(offered.update.as_deref(), Some("0.6.1"));
    // `apply` skips an unchanged presentation, so the notice must change it.
    assert!(none != offered);
}
