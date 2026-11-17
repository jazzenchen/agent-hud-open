//! The rows the panel shows of an agent's windows and balances under a
//! screen's layout, which its HUD mark follows too.
use super::{list, mark, money_amount};
use serde_json::Value;

/// DeepSeek can return several currencies for the same account. The row ID,
/// rather than account ID, identifies the individual display preference.
pub(super) fn deepseek_balances<'a>(
    snapshot: &'a Value,
    layout: &Value,
) -> (Option<String>, &'a str) {
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
    let balance = (!rows.is_empty()).then(|| {
        rows.iter()
            .map(|row| money_amount(&row["amount"], row["currency"].as_str().unwrap_or("USD")))
            .collect::<Vec<_>>()
            .join(" · ")
    });
    (balance, level)
}

/// Whether a block's layout names a row by `key`: its id, its Swift window id,
/// or that window id without the account (quota-rows.ts `quotaRowMatches`).
fn row_matches(row: &Value, key: &str) -> bool {
    let legacy = row["legacyWindowId"].as_str();
    row["id"].as_str() == Some(key)
        || legacy == Some(key)
        || row["accountId"].as_str().is_some_and(|account| {
            legacy.and_then(|legacy| legacy.strip_prefix(account)?.strip_prefix('/')) == Some(key)
        })
}

/// Whether the panel shows a quota row under its block's layout: not hidden,
/// not of a hidden account, and of a named account when the block names some
/// (quota-rows.ts `blockRows`).
pub(super) fn row_shown(row: &Value, block: &Value) -> bool {
    let keys = |name: &str| {
        block[name]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
    };
    let account = row["accountId"]
        .as_str()
        .or_else(|| row["account"].as_str())
        .unwrap_or("unassigned");
    !keys("hiddenRows").any(|key| row_matches(row, key))
        && !keys("hiddenAccounts").any(|key| key == account)
        && (keys("accounts").next().is_none() || keys("accounts").any(|key| key == account))
}
