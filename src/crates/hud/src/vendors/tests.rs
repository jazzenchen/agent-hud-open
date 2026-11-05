use super::*;
use serde_json::json;

#[test]
fn a_balance_reads_as_the_money_format_contract_writes_it() {
    let contract: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/parity/contracts/money-format.json"
    ))
    .unwrap();
    let mut checked = 0;
    for case in contract["cases"].as_array().unwrap() {
        let args = &case["args"];
        if case["fn"] != "MoneyFormat.amount" || args["estimated"] != false {
            continue;
        }
        for language in ["en", "zh"] {
            assert_eq!(
                money_amount(&args["amount"], args["currency"].as_str().unwrap()),
                case["expected"][language],
                "{}",
                case["name"]
            );
        }
        checked += 1;
    }
    assert!(checked >= 7);
    assert_eq!(money_amount(&json!(8.85), "CNY"), "¥8.85");
    assert_eq!(money_amount(&json!(0.125), "USD"), "$0.12");
    assert_eq!(money_amount(&json!(0.135), "USD"), "$0.14");
    assert_eq!(money_amount(&json!(1e-7), "USD"), "$0.00");
}

#[test]
fn an_agent_with_no_window_or_balance_shows_its_last_24_hours() {
    let snapshot = json!({
        "quotas": [{"vendor": "kimi", "usedPercent": 30.0, "status": "normal"}],
        "billing": [], "sessions": [], "approvals": [],
        "providers": [
            {"id": "hermes", "detected": true, "enabled": true,
             "recent": {"tokens": 1_260_000, "costs": {}, "unpricedTokens": 1_260_000}},
            {"id": "claude", "detected": true, "enabled": true,
             "recent": {"tokens": 5_000, "costs": {"USD": 1.2}, "unpricedTokens": 0}},
            {"id": "codex", "detected": true, "enabled": true,
             "recent": {"tokens": 9_000, "costs": {"USD": 412.5}, "unpricedTokens": 300}},
            {"id": "kimi", "detected": true, "enabled": true,
             "recent": {"tokens": 9_000, "costs": {"CNY": 2.0}, "unpricedTokens": 0}},
            {"id": "pi", "detected": true, "enabled": true, "recent": null},
            {"id": "opencode", "detected": true, "enabled": true,
             "recent": {"tokens": 700, "costs": {"USD": 0.0}, "unpricedTokens": 0}}
        ]
    });
    let rows = vendors(&snapshot, "simple", 0);
    let label = |key: &str| {
        let row = rows.iter().find(|row| row.key == key).unwrap();
        meter_label(row, false)
    };
    assert_eq!(label("hermes"), "1.3M", "no price: its tokens");
    assert_eq!(label("claude"), "≈$1.20", "priced: its cost");
    assert_eq!(
        label("openai"),
        "≈$412+",
        "partly priced: the priced part, then +"
    );
    assert_eq!(label("kimi"), "30%", "a window comes first");
    assert_eq!(label("pi"), "--", "nothing used: nothing to show");
    assert_eq!(label("opencode"), "Free", "a local model costs nothing");
}

#[test]
fn short_tokens_and_money_read_as_format_ts_writes_them() {
    for (tokens, text) in [
        (950.0, "950"),
        (1_260.0, "1.3k"),
        (9_999.0, "10k"),
        (48_000.0, "48k"),
        (999_950.0, "1.0M"),
        (2_400_000.0, "2.4M"),
        (3_900_000_000.0, "3.9B"),
    ] {
        assert_eq!(tokens_short(tokens), text, "{tokens}");
    }
    for (amount, code, text) in [
        (3.1, "USD", "$3.10"),
        (412.5, "USD", "$412"),
        (1_234.4, "USD", "$1,234"),
        (0.004, "USD", "<$0.01"),
        (8.85, "CNY", "¥8.85"),
    ] {
        assert_eq!(money_short(amount, code), text, "{amount}");
    }
}

#[test]
fn deepseek_balances_follow_row_visibility_and_order() {
    let snapshot = json!({
        "language": "en", "quotas": [], "sessions": [], "approvals": [], "providers": [],
        "billing": [
            {"id":"ds:USD", "accountId":"ds", "vendor":"deepseek", "amount":0.0, "currency":"USD", "level":"critical"},
            {"id":"ds:CNY", "accountId":"ds", "vendor":"deepseek", "amount":9.92, "currency":"CNY", "level":"warning"}
        ]
    });
    let view = |layout: Value| {
        let list = configured(&snapshot, "simple", &layout, 0);
        let vendor = list
            .into_iter()
            .find(|vendor| vendor.key == "deepseek")
            .unwrap();
        (meter_label(&vendor, false), vendor.level.to_owned())
    };
    assert_eq!(view(json!({})), ("$0.00 · ¥9.92".into(), "critical".into()));
    assert_eq!(
        view(json!({"blocks":{"api:deepseek":{"rows":["ds:CNY", "ds:USD"]}}})).0,
        "¥9.92 · $0.00"
    );
    assert_eq!(
        view(json!({"blocks":{"api:deepseek":{"hiddenRows":["ds:USD"]}}})),
        ("¥9.92".into(), "warning".into())
    );
    assert_eq!(
        view(json!({"blocks":{"api:deepseek":{"hiddenRows":["ds:USD", "ds:CNY"]}}})),
        ("--".into(), "stale".into())
    );
}

#[test]
fn vendors_follow_the_resting_order_and_a_layout_list() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-21T12:00:00Z")
        .unwrap()
        .timestamp_millis();
    let snapshot = json!({
        "language": "en",
        "quotas": [
            {"vendor": "claude", "name": "5h", "usedPercent": 40.0, "status": "normal", "resetAt": null},
            {"vendor": "claude", "name": "Week", "usedPercent": 72.4, "status": "warning", "resetAt": null},
            {"vendor": "codex", "name": "5h", "usedPercent": 90.0, "status": "stale", "resetAt": null}
        ],
        "billing": [{"vendor": "deepseek", "amount": 8.85, "currency": "CNY", "level": "normal"}],
        "providers": [],
        "approvals": [],
        "sessions": [
            {"id": "a", "source": "claude", "state": "running", "updatedAt": "2026-10-21T11:00:00Z"},
            {"id": "b", "source": "codex", "state": "idle", "updatedAt": "2026-10-21T10:00:00Z"}
        ]
    });
    let simple = vendors(&snapshot, "simple", now);
    let keys: Vec<&str> = simple.iter().map(|v| v.key.as_str()).collect();
    assert_eq!(keys, ["claude", "openai", "deepseek"]);
    assert_eq!(simple[0].used, Some(72.4));
    assert_eq!(simple[0].level, "warning");
    assert_eq!(simple[0].working, 1);
    assert_eq!(simple[1].name, "OpenAI");
    // A stale reading gives no level; its mark keeps the share in grey.
    assert_eq!(simple[1].used, None);
    assert_eq!(simple[1].level, "stale");
    assert_eq!(simple[1].held, Some(90.0));
    assert_eq!(meter_label(&simple[1], false), "90%");
    assert_eq!(simple[0].held, None);
    assert_eq!(simple[2].balance.as_deref(), Some("¥8.85"));
    assert_eq!(meter_label(&simple[0], false), "72%");
    assert_eq!(meter_label(&simple[0], true), "28%");
    assert_eq!(simple[0].used, Some(72.4));
    assert_eq!(meter_label(&simple[2], false), "¥8.85");
    let named = configured(
        &snapshot,
        "simple",
        &json!({"hud": {"agents": ["DeepSeek", "Claude Code"]}}),
        now,
    );
    let keys: Vec<&str> = named.iter().map(|v| v.key.as_str()).collect();
    // The list picks the marks; without an agent order they keep the resting order.
    assert_eq!(keys, ["claude", "deepseek"]);
    let unknown = configured(
        &snapshot,
        "simple",
        &json!({"hud": {"agents": ["kimi"]}}),
        now,
    );
    assert_eq!(unknown.len(), 3);
    // The HUD follows Settings' agent order, the panel's blocks (Open #31); a balance block stands for its
    // agent, and an agent with no block keeps its place after them.
    let keys = |layout: Value| -> Vec<String> {
        configured(&snapshot, "simple", &layout, now)
            .iter()
            .map(|v| v.key.clone())
            .collect()
    };
    assert_eq!(
        keys(json!({"order": ["api:deepseek", "codex"]})),
        ["deepseek", "openai", "claude"]
    );
    assert_eq!(
        keys(
            json!({"order": ["api:deepseek", "codex"], "hud": {"agents": ["Claude Code", "Codex"]}})
        ),
        ["openai", "claude"],
        "the HUD's own list picks the marks, Settings' order sorts them"
    );
    assert_eq!(
        keys(json!({"order": ["codex"], "hud": {"agents": ["kimi"]}})),
        ["openai", "claude", "deepseek"]
    );
}
#[test]
fn a_failed_read_holds_its_last_share_until_the_window_resets() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-11-01T12:00:00Z")
        .unwrap()
        .timestamp_millis();
    let window = |used: f64, status: &str, reset: &str| json!({"vendor": "claude", "usedPercent": used, "status": status, "resetAt": reset});
    let held_of = |quotas: Vec<Value>| {
        let snapshot = json!({"quotas": quotas, "billing": [], "providers": [], "sessions": []});
        vendors(&snapshot, "simple", now)[0].held
    };
    // Every window waits on a failed read: the tightest one still running holds.
    assert_eq!(
        held_of(vec![
            window(12.0, "stale", "2026-11-01T13:00:00Z"),
            window(97.0, "stale", "2026-11-03T12:00:00Z"),
            window(100.0, "stale", "2026-11-01T11:00:00Z"),
        ]),
        Some(97.0)
    );
    // A live window stands for the mark; nothing is held beside it.
    assert_eq!(
        held_of(vec![
            window(12.0, "normal", "2026-11-01T13:00:00Z"),
            window(97.0, "stale", "2026-11-03T12:00:00Z"),
        ]),
        None
    );
    // Every window has reset since: nothing is left to hold.
    assert_eq!(
        held_of(vec![window(97.0, "stale", "2026-11-01T11:00:00Z")]),
        None
    );
}

#[test]
fn detected_clients_without_readings_keep_their_marks_and_unknown_meter() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-23T12:00:00Z")
        .unwrap()
        .timestamp_millis();
    let snapshot = json!({
        "quotas": [{"vendor":"claude", "usedPercent":0, "status":"normal"}],
        "billing": [{"vendor":"deepseek", "amount":8.85, "currency":"CNY"}],
        "sessions": [{"source":"opencode", "updatedAt":"2026-10-23T11:00:00Z"}],
        "providers": [
            {"id":"codex", "detected":true, "enabled":true},
            {"id":"chatgpt", "detected":true, "enabled":true},
            {"id":"claude", "detected":true, "enabled":true},
            {"id":"kimi", "detected":true, "enabled":false},
            {"id":"cursor", "detected":false, "enabled":true}
        ]
    });
    for (mode, expected) in [
        ("notch", vec!["claude", "deepseek", "openai"]),
        ("simple", vec!["claude", "deepseek", "openai"]),
        (
            crate::presentation::QUEUE,
            vec!["claude", "opencode", "openai"],
        ),
    ] {
        let rows = vendors(&snapshot, mode, now);
        assert_eq!(
            rows.iter().map(|row| row.key.as_str()).collect::<Vec<_>>(),
            expected
        );
        let codex = rows.iter().find(|row| row.key == "openai").unwrap();
        assert_eq!(codex.used, None);
        assert!(codex.quotas.is_empty());
        assert_eq!(meter_label(codex, false), "--");
        let selected = configured(&snapshot, mode, &json!({"hud":{"agents":["codex"]}}), now);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].key, "openai");
    }
}
