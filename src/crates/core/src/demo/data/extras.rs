//! The other clients in the demo's year: Qwen Code, Grok, Pi, CodeBuddy and
//! OpenClaw, each taken up at its own time, used on some days and at its own
//! hours, and far smaller than Claude Code and Codex. Their days follow the
//! same pace as the rest (`day_pace`): off in the weeks off, mostly quiet at
//! weekends.
use super::history::{chance, day_number, day_pace, HISTORY_DAYS};

/// A client: its source, model, the days since it was taken up, the share of
/// days it is used, its Beijing hours, its tokens in a quarter hour on an
/// ordinary day and its cache reads as a multiple of them.
pub(crate) struct Client {
    pub source: &'static str,
    pub model: &'static str,
    since: i64,
    days: f64,
    hours: &'static [i64],
    quarter: f64,
    read: u64,
}

pub(crate) const CLIENTS: [Client; 5] = [
    Client {
        source: "qwen",
        model: "qwen3.7-plus",
        since: 300,
        days: 0.45,
        hours: &[14, 15, 16],
        quarter: 90_000.0,
        read: 3,
    },
    Client {
        source: "codebuddy",
        model: "glm-5.3",
        since: 240,
        days: 0.4,
        hours: &[10, 11],
        quarter: 80_000.0,
        read: 4,
    },
    Client {
        source: "pi",
        model: "kimi-k3",
        since: 200,
        days: 0.3,
        hours: &[20, 21, 22],
        quarter: 60_000.0,
        read: 3,
    },
    Client {
        source: "openclaw",
        model: "minimax-m3",
        since: 180,
        days: 0.3,
        hours: &[9, 13, 19],
        quarter: 45_000.0,
        read: 2,
    },
    Client {
        source: "grok-cli",
        model: "grok-4.7",
        since: 120,
        days: 0.35,
        hours: &[21, 22],
        quarter: 110_000.0,
        read: 2,
    },
];

/// The other clients' tokens in quarter hours, in `history`'s shape:
/// (start, source, model, input, output, cache read).
pub(crate) fn extra_history(now: i64) -> Vec<(i64, &'static str, &'static str, u64, u64, u64)> {
    const QUARTER: i64 = 900_000;
    const DAY: i64 = 86_400_000;
    let end = now.div_euclid(QUARTER) * QUARTER;
    let count = HISTORY_DAYS as i64 * 96;
    let mut rows = Vec::new();
    for index in 0..count {
        let start = end - (count - index) * QUARTER;
        let hour = (start + 8 * 3_600_000).div_euclid(3_600_000).rem_euclid(24);
        let day = day_number(start) as u64;
        let pace = day_pace(start, end).min(2.0);
        if pace <= 0.0 {
            continue;
        }
        for (salt, extra) in CLIENTS.iter().enumerate() {
            let salt = salt as u64 + 1;
            if (end - start) / DAY > extra.since
                || !extra.hours.contains(&hour)
                || chance(day.wrapping_mul(31) ^ salt) > extra.days
            {
                continue;
            }
            let swing = 0.4 + chance((start as u64).wrapping_mul(salt) ^ 0xE7) * 1.2;
            let tokens = (extra.quarter * pace * swing) as u64;
            if tokens == 0 {
                continue;
            }
            let input = tokens * 4 / 5;
            rows.push((
                start,
                extra.source,
                extra.model,
                input,
                tokens - input,
                tokens * extra.read,
            ));
        }
    }
    rows
}
