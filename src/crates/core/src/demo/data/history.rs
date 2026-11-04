//! The demo's month of tokens, hour by hour, as the sample person works.
use chrono::{DateTime, Datelike, FixedOffset, Timelike};

/// How busy each Beijing hour is when the sample person works at it.
const HOUR_HEIGHTS: [u64; 24] = [
    24, 0, 0, 0, 0, 0, 0, 0, 0, 18, 44, 50, 0, 0, 62, 70, 58, 40, 0, 0, 26, 56, 64, 40,
];
/// Which models work each hour, in `history`'s spender order; the third model's
/// share is its lunchtime slot instead.
const HOUR_MIXES: [[u64; 6]; 24] = [
    [2, 1, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0],
    [1, 0, 0, 0, 0, 0],
    [1, 0, 0, 0, 0, 0],
    [1, 0, 0, 0, 0, 0],
    [1, 0, 0, 0, 0, 0],
    [1, 0, 0, 0, 0, 0],
    [1, 0, 0, 0, 0, 0],
    [1, 0, 0, 0, 0, 0],
    [3, 0, 0, 0, 0, 0],
    [3, 1, 0, 0, 0, 0],
    [2, 2, 0, 1, 0, 0],
    [1, 0, 0, 0, 0, 0],
    [1, 0, 0, 0, 0, 0],
    [1, 3, 0, 2, 0, 0],
    [1, 4, 0, 3, 1, 0],
    [0, 3, 0, 2, 2, 1],
    [0, 1, 0, 0, 3, 2],
    [1, 0, 0, 0, 0, 0],
    [1, 0, 0, 0, 0, 0],
    [1, 1, 0, 0, 1, 1],
    [2, 3, 0, 1, 0, 1],
    [3, 2, 0, 1, 1, 0],
    [2, 1, 0, 0, 0, 2],
];
/// The third model's tokens in each of its lunchtime quarter hours.
const LUNCH_TOKENS: u64 = 40_000;

fn beijing(ms: i64) -> DateTime<FixedOffset> {
    let zone = FixedOffset::east_opt(8 * 3600).expect("Beijing's offset");
    DateTime::from_timestamp_millis(ms)
        .expect("a demo time")
        .with_timezone(&zone)
}

/// A quarter hour's share of the month's work, by its Beijing time: the hours
/// the person works that day, scaled a little day by day, and by half at the
/// weekend. Weekdays count as the iPhone's sample does, Sunday 1 to Saturday 7.
fn work_weight(start: i64) -> (usize, f64) {
    let time = beijing(start);
    let weekday = time.weekday().num_days_from_sunday() + 1;
    let day = time.day();
    let hour = time.hour() as usize;
    let works = match weekday {
        7 => (14..=16).contains(&hour),
        1 => (20..=21).contains(&hour),
        _ => {
            [10, 11, 14, 15, 16, 21, 22].contains(&hour)
                || (hour == 9 && day % 3 == 1)
                || (hour == 17 && day.is_multiple_of(2))
                || (hour == 23 && day % 4 != 3)
                || (hour == 0 && day.is_multiple_of(3))
        }
    };
    let scale = if weekday == 7 || weekday == 1 {
        0.5
    } else {
        [1.0, 0.8, 1.2, 0.9, 1.1][(day % 5) as usize]
    };
    let weight = if works {
        HOUR_HEIGHTS[hour] as f64 * scale
    } else {
        0.0
    };
    (hour, weight)
}

/// 11:30 to 13:30 Beijing time, every day.
fn is_lunch(start: i64) -> bool {
    let time = beijing(start);
    (690..810).contains(&(time.hour() * 60 + time.minute()))
}

/// A month of tokens in quarter hours on the sample person's Beijing days:
/// weekday mornings, afternoons and evenings that shift a little from day to
/// day and now and then run past midnight, short weekends, quiet nights, and
/// each model at the hours it is used: (start, source, model, input, output,
/// cache read). deepseek-v4-flash takes the eight quarter hours from 11:30
/// every day at a fixed size, which its peak pricing is pinned to. The month
/// adds up to 15 million tokens a day before cache reads. The iPhone's
/// `DemoFleet.buckets` is the same.
pub(crate) fn history(now: i64) -> Vec<(i64, &'static str, &'static str, u64, u64, u64)> {
    const QUARTER: i64 = 900_000;
    let end = now.div_euclid(QUARTER) * QUARTER;
    let spenders: [(&str, &str); 6] = [
        ("claude", "claude-fable-5-1"),
        ("claude", "claude-opus-5-5"),
        ("deepseek", "deepseek-v4-flash"),
        ("codex", "codex-auto-review"),
        ("codex", "gpt-6.1-sol"),
        ("claude", "claude-sonnet-5-5"),
    ];
    let count = 30 * 96;
    let starts: Vec<i64> = (0..count)
        .map(|index| end - (count - index) as i64 * QUARTER)
        .collect();
    let work: Vec<(usize, f64)> = starts.iter().map(|start| work_weight(*start)).collect();
    let lunch: Vec<bool> = starts.iter().map(|start| is_lunch(*start)).collect();
    let weight: f64 = work.iter().map(|(_, weight)| weight).sum();
    let lunches = lunch.iter().filter(|lunch| **lunch).count() as u64;
    let multiplier = (15_000_000.0 * 30.0 - (lunches * LUNCH_TOKENS) as f64) / weight;
    let mut buckets = Vec::new();
    for ((start, (hour, weight)), lunch) in starts.iter().zip(&work).zip(&lunch) {
        let mix = HOUR_MIXES[*hour];
        let total: u64 = mix.iter().sum();
        for (position, (source, model)) in spenders.iter().enumerate() {
            let tokens = if position == 2 {
                if *lunch {
                    LUNCH_TOKENS
                } else {
                    0
                }
            } else {
                (weight * multiplier * mix[position] as f64 / total as f64) as u64
            };
            if tokens == 0 {
                continue;
            }
            let input = tokens * 4 / 5;
            buckets.push((
                *start,
                *source,
                *model,
                input,
                tokens - input,
                tokens * (position as u64 + 1),
            ));
        }
    }
    buckets
}
