//! The demo's year of tokens, hour by hour, as the sample person works.
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

/// The days of tokens the demo holds: a year.
pub(crate) const HISTORY_DAYS: usize = 365;
/// The last days, whose tokens add up to 15 million a day before cache reads.
const MONTH_DAYS: usize = 30;

/// A number from 0 to 1 that stays the same for the same `seed`.
pub(super) fn chance(seed: u64) -> f64 {
    // SplitMix64.
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// The Beijing day a moment falls on, counted from 1970-01-01.
pub(super) fn day_number(ms: i64) -> i64 {
    (ms + 8 * 3_600_000).div_euclid(86_400_000)
}

/// How busy a Beijing day is against an ordinary working day. The person took
/// to agents over the first months; after that the months rise and fall (a
/// crunch in June and September, quiet in August and around the Spring
/// Festival, with a week off in each), weeks run hot or cool, single days
/// swing widely, a weekday is sometimes off, and most weekends are rest with
/// the odd one worked through.
pub(super) fn day_pace(start: i64, end: i64) -> f64 {
    const MONTHS: [f64; 12] = [
        0.6, 0.45, 0.8, 0.65, 0.95, 1.25, 0.85, 0.5, 1.15, 0.9, 1.0, 0.55,
    ];
    const WEEKS: [f64; 8] = [0.45, 0.7, 0.85, 1.0, 1.0, 1.1, 1.3, 1.65];
    let time = beijing(start);
    let (month, date) = (time.month(), time.day());
    if (month == 2 && (14..=22).contains(&date)) || (month == 8 && (3..=9).contains(&date)) {
        return 0.0;
    }
    let day = day_number(start) as u64;
    let roll = |salt: u64| chance(day.wrapping_mul(0x1000_0000_01B3) ^ salt);
    let age = (end - start) as f64 / 86_400_000.0;
    let adoption = 0.3 + 0.7 * ((HISTORY_DAYS as f64 - age) / 130.0).clamp(0.0, 1.0);
    let week = WEEKS[(chance(day / 7 + 0x5EED) * WEEKS.len() as f64) as usize];
    // Roughly a log-normal swing: most days within half to twice the usual.
    let swing = ((roll(1) + roll(2) + roll(3) - 1.5) * 0.8).exp();
    let weekday = time.weekday().num_days_from_sunday();
    let rest = if weekday == 0 || weekday == 6 {
        match roll(4) {
            r if r < 0.5 => 0.0,
            r if r < 0.82 => 0.5,
            _ => 1.8,
        }
    } else if roll(5) < 0.06 {
        0.0
    } else {
        1.0
    };
    MONTHS[month as usize - 1] * adoption * week * swing * rest
}

/// How hard an hour of a day is worked, around the day's own pace.
fn hour_swing(start: i64) -> f64 {
    let hour = (start + 8 * 3_600_000).div_euclid(3_600_000) as u64;
    0.45 + chance(hour ^ 0xA11CE) * 1.1
}

/// deepseek-v4-flash at lunch: most days, in a size of its own.
fn lunch_tokens(start: i64, pace: f64) -> u64 {
    let day = day_number(start) as u64;
    if pace <= 0.0 || !is_lunch(start) || chance(day ^ 0x10C4) > 0.7 {
        return 0;
    }
    (LUNCH_TOKENS as f64 * (0.4 + chance(day ^ 0x51CE) * 1.2)) as u64
}

/// A year of tokens in quarter hours on the sample person's Beijing days:
/// working hours that shift a little from day to day and now and then run
/// past midnight, quiet nights, the day's pace from `day_pace`, and each model
/// at the hours it is used: (start, source, model, input, output, cache read).
/// deepseek-v4-flash takes quarter hours from 11:30 on most days. The last
/// thirty days add up to 15 million tokens a day before cache reads.
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
    let count = HISTORY_DAYS * 96;
    let month = count - MONTH_DAYS * 96;
    let starts: Vec<i64> = (0..count)
        .map(|index| end - (count - index) as i64 * QUARTER)
        .collect();
    let paces: Vec<f64> = starts.iter().map(|start| day_pace(*start, end)).collect();
    let work: Vec<(usize, f64)> = starts
        .iter()
        .zip(&paces)
        .map(|(start, pace)| {
            let (hour, weight) = work_weight(*start);
            (hour, weight * pace * hour_swing(*start))
        })
        .collect();
    let lunch: Vec<u64> = starts
        .iter()
        .zip(&paces)
        .map(|(start, pace)| lunch_tokens(*start, *pace))
        .collect();
    // The last thirty days set the scale.
    let weight: f64 = work[month..].iter().map(|(_, weight)| weight).sum();
    let lunches: u64 = lunch[month..].iter().sum();
    let multiplier = (15_000_000.0 * 30.0 - lunches as f64) / weight;
    let mut buckets = Vec::new();
    for ((start, (hour, weight)), lunch) in starts.iter().zip(&work).zip(&lunch) {
        let mix = HOUR_MIXES[*hour];
        let total: u64 = mix.iter().sum();
        for (position, (source, model)) in spenders.iter().enumerate() {
            let tokens = if position == 2 {
                *lunch
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
