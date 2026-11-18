//! The agents a resting HUD draws and what each one reads, projected from a
//! snapshot as src/packages/hud/src/model.ts `vendorsFromSnapshot` and
//! `configuredVendors` do, for what the host draws itself: the menu bar item,
//! the HUD windows' sizes and the Bots' directions.
mod format;
mod panel_rows;

use crate::presentation::{mark, resting_agents};
pub use format::{
    meter_label, money_amount, money_short, quota_percent, recent_label, tokens_short,
};
use panel_rows::{deepseek_balances, row_shown};
use serde_json::Value;
use std::cmp::Ordering;

pub struct Vendor<'a> {
    pub key: String,
    pub name: String,
    /// The measured window's used percent: the shortest, when the screen asks
    /// for it, else the tightest.
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

/// Whether a window's reset has passed: its share has started over, so its
/// reading no longer says how much is used until the next read (Open #90;
/// held-reading.ts `resetPassed`).
fn reset_passed(q: &Value, now_ms: i64) -> bool {
    q["resetAt"]
        .as_str()
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .is_some_and(|at| at.timestamp_millis() <= now_ms)
}

/// A window that reads live now: a number from the last read, and a reset
/// still ahead (held-reading.ts `liveWindow`).
fn live(q: &Value, now_ms: i64) -> bool {
    q["usedPercent"].is_number() && q["status"] != "stale" && !reset_passed(q, now_ms)
}

/// The tightest window of an agent's last reading while every window waits
/// on a failed read ("stale"), so a Simple mark keeps showing it in the stale
/// grey. A window whose reset has passed holds nothing, since its share has
/// started over. None while any window reads live (held-reading.ts `heldUsed`).
fn held(quotas: &[&Value], now_ms: i64) -> Option<f64> {
    if quotas.iter().any(|q| live(q, now_ms)) {
        return None;
    }
    quotas
        .iter()
        .filter(|q| !reset_passed(q, now_ms))
        .filter_map(|q| q["usedPercent"].as_f64())
        .fold(None, |most: Option<f64>, used| {
            Some(most.map_or(used, |most| most.max(used)))
        })
}

/// The live window with the highest used percent, with that percent; the
/// first of equal readings stays the tightest (model.ts `tightestWindow`).
fn tightest<'a>(quotas: &[&'a Value], now_ms: i64) -> Option<(f64, &'a Value)> {
    quotas
        .iter()
        .filter(|q| live(q, now_ms))
        .filter_map(|q| Some((q["usedPercent"].as_f64()?, *q)))
        .fold(None, |best, (used, q)| match best {
            Some((most, _)) if used <= most => best,
            _ => Some((used, q)),
        })
}

/// The live window with the shortest reported window, with its percent; ties
/// go to the highest used percent. When no live window reports a window
/// length, the tightest window instead, so a screen without duration data
/// reads as it did (held-reading.ts `shortestWindow`).
fn shortest<'a>(quotas: &[&'a Value], now_ms: i64) -> Option<(f64, &'a Value)> {
    quotas
        .iter()
        .filter(|q| live(q, now_ms))
        .filter(|q| {
            q["windowDurationSeconds"]
                .as_f64()
                .is_some_and(|duration| duration.is_finite() && duration > 0.0)
        })
        .min_by(|a, b| {
            let duration = |q: &&Value| q["windowDurationSeconds"].as_f64().unwrap();
            duration(a)
                .partial_cmp(&duration(b))
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    // Ties: the highest used percent sorts first.
                    b["usedPercent"]
                        .as_f64()
                        .unwrap()
                        .partial_cmp(&a["usedPercent"].as_f64().unwrap())
                        .unwrap_or(Ordering::Equal)
                })
        })
        .map(|q| (q["usedPercent"].as_f64().unwrap(), *q))
        .or_else(|| tightest(quotas, now_ms))
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
            let tightest = tightest(&quotas, now_ms);
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
/// HUD (HUDAgentOrder.swift). `shortest_first` is the screen's own switch
/// (Settings → Display → HUD): its numbers follow the shortest live window,
/// not the most used one.
pub fn configured<'a>(
    snapshot: &'a Value,
    mode: &str,
    layout: &Value,
    now_ms: i64,
    shortest_first: bool,
) -> Vec<Vendor<'a>> {
    let mut available = vendors(snapshot, mode, now_ms);
    for vendor in &mut available {
        // The number and level follow the windows the panel shows: a row the
        // person hid there no longer sets them (Open #91). The rings already
        // draw only those (mark-windows.ts).
        let shown: Vec<&Value> = vendor
            .quotas
            .iter()
            .copied()
            .filter(|q| row_shown(q, &layout["blocks"][q["vendor"].as_str().unwrap_or("")]))
            .collect();
        if shown.len() < vendor.quotas.len() || shortest_first {
            let picked = if shortest_first {
                shortest(&shown, now_ms)
            } else {
                tightest(&shown, now_ms)
            };
            vendor.used = picked.map(|(used, _)| used);
            vendor.held = held(&shown, now_ms);
            vendor.level = picked
                .and_then(|(_, q)| q["status"].as_str())
                .unwrap_or("stale");
        }
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
        "codebuddy-cn" => "CodeBuddy CN",
        "workbuddy" => "WorkBuddy",
        "qwen" => "Qwen",
        "qoder" => "Qoder",
        "qoder-cn" => "Qoder CN",
        "qoder-work" => "QoderWork",
        "devin" => "Devin",
        "minimax" => "MiniMax",
        "minimax-code" => "MiniMax Code",
        "ark" => "Volcengine Ark",
        "droid" => "Factory Droid",
        "cline" => "Cline",
        "roo-code" => "Roo Code",
        "goose" => "Goose",
        "trae" => "TRAE",
        "trae-cn" => "TRAE CN",
        // Agents with no mark of their own here, by their own names.
        _ => agent_hud_core::naming::model_names::vendor_name(source),
    };
    name.to_owned()
}

#[cfg(test)]
mod tests;
