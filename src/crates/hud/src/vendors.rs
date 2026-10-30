//! The agents a resting HUD draws and what each one reads, projected from a
//! snapshot as src/packages/hud/src/model.ts `vendorsFromSnapshot` and
//! `configuredVendors` do, for what the host draws itself: the menu bar item,
//! the HUD windows' sizes and the Bots' directions.
use crate::presentation::{mark, resting_agents};
use serde_json::{json, Value};

pub struct Vendor<'a> {
    pub key: String,
    pub name: String,
    /// The tightest measured window's used percent.
    pub used: Option<f64>,
    pub level: &'a str,
    /// Sessions in flight: running, or waiting on a request.
    pub working: usize,
    pub balance: Option<String>,
    /// What an agent with no window and no balance shows instead: its last
    /// 24 hours at list prices, else their tokens (`recent_label`).
    pub usage: Option<String>,
    pub quotas: Vec<&'a Value>,
    pub sessions: Vec<&'a Value>,
}

fn list<'a>(snapshot: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    snapshot[key].as_array().into_iter().flatten()
}

/// DeepSeek can return several currencies for the same account. The row ID,
/// rather than account ID, identifies the individual display preference.
fn deepseek_balances<'a>(snapshot: &'a Value, layout: &Value) -> (Option<String>, &'a str) {
    let mut rows: Vec<&Value> = list(snapshot, "billing")
        .filter(|row| mark(row["vendor"].as_str().unwrap_or("")) == "deepseek")
        .filter(|row| {
            let id = row["id"].as_str().unwrap_or("");
            let account = row["accountId"].as_str().unwrap_or(id);
            let has = |key: &str, value: &str| {
                layout[key]
                    .as_array()
                    .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(value)))
            };
            !has("hiddenRows", id)
                && !has("hiddenAccounts", account)
                && (layout["accounts"]
                    .as_array()
                    .is_none_or(|items| items.is_empty())
                    || has("accounts", account))
        })
        .collect();
    let order = layout["rows"].as_array();
    rows.sort_by_key(|row| {
        order
            .and_then(|items| items.iter().position(|item| item == &row["id"]))
            .unwrap_or(usize::MAX)
    });
    let level = if rows.iter().any(|row| row["level"] == "critical") {
        "critical"
    } else if rows.iter().any(|row| row["level"] == "warning") {
        "warning"
    } else if rows.iter().any(|row| row["level"] == "normal") {
        "normal"
    } else {
        "stale"
    };
    let zh = snapshot["language"] == "zh";
    let balance = (!rows.is_empty()).then(|| {
        rows.iter()
            .map(|row| {
                money_amount(
                    &row["amount"],
                    row["currency"].as_str().unwrap_or("USD"),
                    zh,
                )
            })
            .collect::<Vec<_>>()
            .join(" · ")
    });
    (balance, level)
}

/// Every agent a resting HUD in `mode` shows without a list, one per mark.
pub fn vendors<'a>(snapshot: &'a Value, mode: &str, now_ms: i64) -> Vec<Vendor<'a>> {
    let zh = snapshot["language"] == "zh";
    resting_agents(snapshot, mode, now_ms)
        .into_iter()
        .map(|source| {
            let key = mark(&source);
            let quotas: Vec<&Value> = list(snapshot, "quotas")
                .filter(|q| mark(q["vendor"].as_str().unwrap_or("")) == key)
                .collect();
            // The first of equal readings stays the tightest.
            let tightest = quotas
                .iter()
                .filter(|q| q["status"] != "stale")
                .filter_map(|q| Some((q["usedPercent"].as_f64()?, *q)))
                .fold(None::<(f64, &Value)>, |best, (used, q)| match best {
                    Some((most, _)) if used <= most => best,
                    _ => Some((used, q)),
                });
            let sessions: Vec<&Value> = list(snapshot, "sessions")
                .filter(|s| mark(s["source"].as_str().unwrap_or("")) == key)
                .collect();
            let balance = quotas
                .is_empty()
                .then(|| {
                    list(snapshot, "billing")
                        .find(|b| mark(b["vendor"].as_str().unwrap_or("")) == key)
                })
                .flatten();
            let deepseek = (key == "deepseek" && quotas.is_empty() && balance.is_some())
                .then(|| deepseek_balances(snapshot, &Value::Null));
            Vendor {
                name: vendor_name(&source),
                used: tightest.map(|(used, _)| used),
                // An agent billed by balance alone takes its balance's level.
                level: tightest
                    .and_then(|(_, q)| q["status"].as_str())
                    .or_else(|| deepseek.as_ref().map(|(_, level)| *level))
                    .or_else(|| balance.and_then(|b| b["level"].as_str()))
                    .unwrap_or("stale"),
                working: sessions
                    .iter()
                    .filter(|s| matches!(s["state"].as_str(), Some("running" | "waiting")))
                    .count(),
                balance: deepseek.and_then(|(label, _)| label).or_else(|| {
                    balance.map(|b| {
                        money_amount(&b["amount"], b["currency"].as_str().unwrap_or("USD"), zh)
                    })
                }),
                usage: (quotas.is_empty() && balance.is_none())
                    .then(|| recent_label(snapshot, &key))
                    .flatten(),
                key,
                quotas,
                sessions,
            }
        })
        .collect()
}

/// The marks a screen's HUD names in its layout's `hud.agents`, in that order;
/// a list naming none of the available agents never empties a HUD
/// (HUDAgentOrder.swift).
pub fn configured<'a>(
    snapshot: &'a Value,
    mode: &str,
    layout: &Value,
    now_ms: i64,
) -> Vec<Vendor<'a>> {
    let mut available = vendors(snapshot, mode, now_ms);
    for vendor in &mut available {
        if vendor.key == "deepseek" && vendor.quotas.is_empty() && vendor.balance.is_some() {
            let block = list(snapshot, "billing")
                .find(|b| mark(b["vendor"].as_str().unwrap_or("")) == vendor.key)
                .and_then(|b| b["vendor"].as_str())
                .map(|name| &layout["blocks"][format!("api:{name}")])
                .unwrap_or(&Value::Null);
            let (balance, level) = deepseek_balances(snapshot, block);
            vendor.balance = balance;
            vendor.level = level;
            vendor.usage = None;
        }
    }
    let marks: Vec<String> = layout["hud"]["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(mark)
        .collect();
    if !available.iter().any(|vendor| marks.contains(&vendor.key)) {
        return available;
    }
    let mut named: Vec<Vendor> = available
        .into_iter()
        .filter(|vendor| marks.contains(&vendor.key))
        .collect();
    named.sort_by_key(|vendor| marks.iter().position(|key| *key == vendor.key));
    named
}

/// A display percentage; stored readings and quota severity always use consumed usage.
pub fn quota_percent(used: f64, remaining: bool) -> f64 {
    let used = used.clamp(0.0, 100.0);
    if remaining {
        100.0 - used
    } else {
        used
    }
}

/// The number a Simple HUD writes beside a mark: its tightest window, else
/// its balance, else its last 24 hours (model.ts `meterLabel`).
pub fn meter_label(vendor: &Vendor, remaining: bool) -> String {
    match vendor.used {
        Some(used) => format!("{}%", quota_percent(used, remaining).round()),
        None => vendor
            .balance
            .clone()
            .or_else(|| vendor.usage.clone())
            .unwrap_or_else(|| "--".into()),
    }
}

/// What the clients behind mark `key` used in the last 24 hours, as the
/// snapshot's provider rows report it (`recent`), in a few characters
/// (model.ts `recentLabel`): "≈$1.20" at list prices, with "+" when some of
/// it has no list price or is in another currency, else "1.2M" tokens. None
/// when they used nothing.
pub fn recent_label(snapshot: &Value, key: &str) -> Option<String> {
    let mut tokens = 0.0;
    let mut unpriced = 0.0;
    let mut costs = std::collections::BTreeMap::<String, f64>::new();
    for recent in list(snapshot, "providers")
        .filter(|provider| mark(provider["id"].as_str().unwrap_or("")) == key)
        .map(|provider| &provider["recent"])
        .filter(|recent| recent.is_object())
    {
        tokens += recent["tokens"].as_f64().unwrap_or(0.0);
        unpriced += recent["unpricedTokens"].as_f64().unwrap_or(0.0);
        for (currency, amount) in recent["costs"].as_object().into_iter().flatten() {
            *costs.entry(currency.clone()).or_default() += amount.as_f64().unwrap_or(0.0);
        }
    }
    if tokens <= 0.0 {
        return None;
    }
    let shown = costs
        .get_key_value("USD")
        .or_else(|| costs.iter().next())
        .map(|(currency, amount)| (currency.clone(), *amount));
    Some(match shown {
        Some((_, amount)) if amount == 0.0 && unpriced <= 0.0 && costs.len() == 1 => {
            // Calls to a model on this computer cost nothing.
            if snapshot["language"] == "zh" {
                "免费"
            } else {
                "Free"
            }
            .to_owned()
        }
        Some((currency, amount)) => {
            let more = unpriced > 0.0 || costs.len() > 1;
            format!(
                "≈{}{}",
                money_short(amount, &currency),
                if more { "+" } else { "" }
            )
        }
        None => tokens_short(tokens),
    })
}

/// format.ts `tokensShort`: 48_000 → "48k", 1_260 → "1.3k", 950 → "950",
/// 2_400_000 → "2.4M"; a value that rounds up to the next unit reads in it.
pub fn tokens_short(tokens: f64) -> String {
    const UNITS: [(f64, &str); 3] = [(1e9, "B"), (1e6, "M"), (1e3, "k")];
    // One decimal below ten, none from ten up; ties to even, as printf.
    let trim = |value: f64| {
        let one = format!("{value:.1}");
        if value >= 10.0 || one.parse::<f64>().unwrap_or(0.0) >= 10.0 {
            format!("{value:.0}")
        } else {
            one
        }
    };
    for (index, (size, unit)) in UNITS.iter().enumerate() {
        if tokens < *size {
            continue;
        }
        let shown = trim(tokens / size);
        if index > 0 && shown.parse::<f64>().unwrap_or(0.0) >= 1000.0 {
            let (larger, larger_unit) = UNITS[index - 1];
            return trim(tokens / larger) + larger_unit;
        }
        return shown + unit;
    }
    format!("{}", tokens.trunc())
}

/// format.ts `moneyShort`: whole units from a hundred up ("$412"), two
/// decimals below ("$3.10"), "<$0.01" for a positive amount that rounds to none.
pub fn money_short(amount: f64, code: &str) -> String {
    let cents = format!("{amount:.2}");
    let rounded = cents.parse::<f64>().unwrap_or(0.0);
    if rounded == 0.0 && amount > 0.0 {
        return format!("<{}", money_amount(&json!("0.01"), code, false));
    }
    if rounded.abs() >= 100.0 {
        let text = money_amount(&json!(format!("{rounded:.0}")), code, false);
        return text.strip_suffix(".00").map(str::to_owned).unwrap_or(text);
    }
    money_amount(&json!(cents), code, false)
}

/// agent-artwork.ts `agentVendorName`.
fn vendor_name(source: &str) -> String {
    let name = match mark(source).as_str() {
        "claude" => "Claude",
        "openai" => "OpenAI",
        "antigravity" => "Antigravity",
        "deepseek" => "DeepSeek",
        "grok" => "Grok",
        "grok-bot" => "Grok Bot",
        "cursor" => "Cursor",
        "opencode" => "OpenCode",
        "kimi" => "Kimi",
        "glm" => "GLM",
        "pi" => "Pi",
        "copilot" => "GitHub Copilot",
        "openclaw" => "OpenClaw",
        "hermes" => "Hermes",
        "zcode" => "ZCode",
        "codebuddy" => "CodeBuddy",
        "workbuddy" => "WorkBuddy",
        "qwen" => "Qwen",
        "qoder" => "Qoder",
        "devin" => "Devin",
        _ => source,
    };
    name.to_owned()
}

/// A balance with its currency's narrow symbol and two decimals, rounded half
/// to even, as format.ts `moneyAmount` writes it: "$12.99", "¥1,234.50",
/// "-$3.00". Both languages write it the same way.
pub fn money_amount(amount: &Value, code: &str, _zh: bool) -> String {
    let decimal = match amount {
        Value::String(text) => text.trim().to_owned(),
        // The shortest form that reads back as the same number, as Intl reads it.
        Value::Number(number) => number.as_f64().map(|v| v.to_string()).unwrap_or_default(),
        _ => String::new(),
    };
    let (negative, digits) = match decimal.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, decimal.strip_prefix('+').unwrap_or(&decimal)),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty() && fraction.is_empty()
        || !whole
            .chars()
            .chain(fraction.chars())
            .all(|c| c.is_ascii_digit())
    {
        return format!("{code} {decimal}");
    }
    // Hundredths, rounded half to even on the decimal digits.
    let mut cents: u128 = format!("{whole}{:0<2}", fraction.get(..2).unwrap_or(fraction))
        .trim_start_matches('0')
        .parse()
        .unwrap_or(0);
    let rest = fraction.get(2..).unwrap_or("");
    let first = rest.bytes().next().unwrap_or(b'0');
    let beyond = rest.bytes().skip(1).any(|b| b != b'0');
    if first > b'5' || first == b'5' && (beyond || cents % 2 == 1) {
        cents += 1;
    }
    let units = (cents / 100).to_string();
    let mut grouped = String::new();
    for (index, digit) in units.chars().enumerate() {
        if index > 0 && (units.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    let symbol = match code {
        "USD" | "AUD" | "CAD" | "HKD" | "NZD" | "SGD" | "TWD" | "MXN" => "$",
        "CNY" | "JPY" => "¥",
        "EUR" => "€",
        "GBP" => "£",
        "KRW" => "₩",
        "INR" => "₹",
        _ => "",
    };
    let sign = if negative && cents > 0 { "-" } else { "" };
    let amount = format!("{grouped}.{:02}", cents % 100);
    if symbol.is_empty() {
        format!("{sign}{code}\u{a0}{amount}")
    } else {
        format!("{sign}{symbol}{amount}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_balance_reads_as_the_money_format_contract_writes_it() {
        let contract: Value = serde_json::from_str(include_str!(
            "../../../fixtures/parity/contracts/money-format.json"
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
                    money_amount(
                        &args["amount"],
                        args["currency"].as_str().unwrap(),
                        language == "zh"
                    ),
                    case["expected"][language],
                    "{}",
                    case["name"]
                );
            }
            checked += 1;
        }
        assert!(checked >= 7);
        assert_eq!(money_amount(&json!(8.85), "CNY", false), "¥8.85");
        assert_eq!(money_amount(&json!(0.125), "USD", false), "$0.12");
        assert_eq!(money_amount(&json!(0.135), "USD", false), "$0.14");
        assert_eq!(money_amount(&json!(1e-7), "USD", false), "$0.00");
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
        // A stale reading is no reading.
        assert_eq!(simple[1].used, None);
        assert_eq!(simple[1].level, "stale");
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
        assert_eq!(keys, ["deepseek", "claude"]);
        let unknown = configured(
            &snapshot,
            "simple",
            &json!({"hud": {"agents": ["kimi"]}}),
            now,
        );
        assert_eq!(unknown.len(), 3);
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
            ("logos", vec!["claude", "opencode", "openai"]),
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
}
