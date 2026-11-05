//! The agents a resting HUD draws and what each one reads, projected from a
//! snapshot as src/packages/hud/src/model.ts `vendorsFromSnapshot` and
//! `configuredVendors` do, for what the host draws itself: the menu bar item,
//! the HUD windows' sizes and the Bots' directions.
mod format;

use crate::presentation::{mark, resting_agents};
pub use format::{
    meter_label, money_amount, money_short, quota_percent, recent_label, tokens_short,
};
use serde_json::Value;

pub struct Vendor<'a> {
    pub key: String,
    pub name: String,
    /// The tightest measured window's used percent.
    pub used: Option<f64>,
    /// The last reading's tightest window while every window waits on a
    /// failed read (`held`).
    pub held: Option<f64>,
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
    let balance = (!rows.is_empty()).then(|| {
        rows.iter()
            .map(|row| money_amount(&row["amount"], row["currency"].as_str().unwrap_or("USD")))
            .collect::<Vec<_>>()
            .join(" · ")
    });
    (balance, level)
}

/// The tightest window of an agent's last reading while every window waits
/// on a failed read ("stale"), so a Simple mark keeps showing it in the stale
/// grey. A window whose reset has passed holds nothing, since its share has
/// started over. None while any window reads live (held-reading.ts `heldUsed`).
fn held(quotas: &[&Value], now_ms: i64) -> Option<f64> {
    if quotas
        .iter()
        .any(|q| q["usedPercent"].is_number() && q["status"] != "stale")
    {
        return None;
    }
    quotas
        .iter()
        .filter(|q| {
            q["resetAt"]
                .as_str()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                .is_none_or(|at| at.timestamp_millis() > now_ms)
        })
        .filter_map(|q| q["usedPercent"].as_f64())
        .fold(None, |most: Option<f64>, used| {
            Some(most.map_or(used, |most| most.max(used)))
        })
}

/// Every agent a resting HUD in `mode` shows without a list, one per mark; `mode` is
/// "notch", "simple" or the bots' [`crate::presentation::QUEUE`].
pub fn vendors<'a>(snapshot: &'a Value, mode: &str, now_ms: i64) -> Vec<Vendor<'a>> {
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
                held: held(&quotas, now_ms),
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
                        money_amount(&b["amount"], b["currency"].as_str().unwrap_or("USD"))
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

/// The marks a screen's HUD shows, in Settings' agent order, the layout's `order`
/// (resting-agents.ts `layoutMarks`). Its `hud.agents` picks which marks show but
/// not their order; a list naming none of the available agents never empties a
/// HUD (HUDAgentOrder.swift).
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
    if available.iter().any(|vendor| marks.contains(&vendor.key)) {
        available.retain(|vendor| marks.contains(&vendor.key));
    }
    // The panel and the HUD share one order, Settings' agent order: the panel's blocks, a balance block standing
    // for its agent. An agent with no block keeps its place after them (Open #31).
    let order: Vec<String> = layout["order"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|key| mark(key.strip_prefix("api:").unwrap_or(key)))
        .collect();
    available.sort_by_key(|vendor| {
        order
            .iter()
            .position(|key| *key == vendor.key)
            .unwrap_or(order.len())
    });
    available
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
        "minimax" => "MiniMax",
        "ark" => "Volcengine Ark",
        "droid" => "Factory Droid",
        "cline" => "Cline",
        "roo-code" => "Roo Code",
        "goose" => "Goose",
        "trae" => "TRAE",
        // Agents with no mark of their own here, by their own names.
        _ => agent_hud_core::naming::model_names::vendor_name(source),
    };
    name.to_owned()
}

#[cfg(test)]
mod tests;
