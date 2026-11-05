//! What an agent's proposal for the hover panel may name, and what it changes, as
//! agent-hud-native-open tells it (PanelLayout.swift, PanelLayoutRules.swift,
//! PanelLayoutMisreadings.swift, PanelLayoutFilters.swift, PanelHUD.swift).
//!
//! A layout is written one way (`normalize_layout`), so two layouts that say the
//! same are equal; a proposal is reduced to what it changes against the blocks
//! the panel has now (`reduced`), so one that draws what is saved is the saved
//! layout; and a proposal the panel cannot draw as asked is refused with every
//! reason (`misreadings`, `issues`, `filter_issues`, `hud_issues`).
mod block_issues;
mod blocks;
mod filter_issues;
mod layout;
mod marks;
mod misreadings;
mod projects;
mod reduced;

use super::*;
pub(super) use block_issues::issues;
pub(super) use blocks::{present_blocks, Block, Kind};
pub(super) use filter_issues::{filter_issues, hud_issues, session_agents};
pub(super) use layout::normalize_layout;
pub use marks::{mark, resting_agents};
pub(super) use misreadings::{hud_agent_misreadings, misreadings};
pub(super) use projects::{offered_projects, project_match, project_title};
pub(super) use reduced::reduced;

pub(super) const SECTIONS: &[&str] = &["blocks", "tokens", "sessions", "quotaHistory", "planValue"];
/// Sections a panel shows only once it is asked to: a layout whose `sections`
/// does not name one keeps it hidden, so a layout saved before them is drawn
/// as it was.
pub(super) const ADDED_SECTIONS: &[&str] = &["quotaHistory", "planValue"];
/// `rings` are concentric and `segments` split one ring, one per account; a layout
/// saved before them says `ring`, which `parse_block` reads as `rings`.
const WIDGETS: &[&str] = &["bar", "rings", "segments", "number", "chart"];
const RING_SIZES: &[&str] = &["small", "medium", "large"];
/// How much of a line a block takes; blocks that take less share it where each
/// keeps enough room (QuotaBlocks.tsx).
const SPANS: &[&str] = &["full", "half", "third", "quarter", "fifth", "sixth"];
const PERIODS: &[&str] = &["day", "week", "month"];
const SPACINGS: &[&str] = &["compact", "regular", "roomy"];
const SESSION_PERIODS: &[&str] = &["day", "week"];
/// What a layout saved by an earlier build may hold for the session period; read,
/// never proposed.
const STORED_SESSION_PERIODS: &[&str] = &[
    "day", "week", "active", "recent", "all", "1h", "6h", "12h", "24h", "48h", "7d",
];
const DEFAULT_COLUMNS: i64 = 4;
const DEFAULT_WIDTH: i64 = 540;
/// The panel's widths: two columns of sections side by side need about twice the narrowest.
pub(super) const WIDTHS: std::ops::RangeInclusive<i64> = 480..=960;

// MARK: Small helpers

/// The strings of a list, each once, in order; empty ones and non-strings are left out.
pub(super) fn strings(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for v in v.as_array().into_iter().flatten() {
        if let Some(s) = v.as_str().filter(|s| !s.is_empty()) {
            if !out.iter().any(|old| old == s) {
                out.push(s.to_owned());
            }
        }
    }
    out
}

/// Names in quotes, separated by commas.
pub(super) fn quoted<S: AsRef<str>>(names: &[S]) -> String {
    names
        .iter()
        .map(|n| format!("\"{}\"", n.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Names joined as a sentence lists them: "a, b and c".
pub(super) fn listed<S: AsRef<str>>(names: &[S]) -> String {
    match names {
        [] => String::new(),
        [one] => one.as_ref().to_owned(),
        [rest @ .., last] => format!(
            "{} and {}",
            rest.iter()
                .map(|n| n.as_ref())
                .collect::<Vec<_>>()
                .join(", "),
            last.as_ref()
        ),
    }
}

/// A whole number, written as an integer or as a float without a fraction.
pub(super) fn whole(v: &Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(n);
    }
    let n = v.as_f64()?;
    (n.fract() == 0.0 && n.abs() < 9.0e15).then_some(n as i64)
}

/// Whether two values say the same: numbers by value, so 20 and 20.0 are one.
pub(super) fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|other| same(v, other)))
        }
        _ => a == b,
    }
}

fn push_unique(list: &mut Vec<String>, value: &str) {
    if !value.is_empty() && !list.iter().any(|v| v == value) {
        list.push(value.to_owned());
    }
}

/// The string entries of a list with their place in it.
fn entries(v: &Value) -> impl Iterator<Item = (usize, &str)> {
    v.as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(i, v)| v.as_str().map(|s| (i, s)))
}
