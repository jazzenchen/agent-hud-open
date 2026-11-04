//! A plan's meters for one billing period, from the dashboard's summary or
//! from the period Cursor itself shows.
use super::account::percentage;
use chrono::{DateTime, FixedOffset};
use serde_json::Value;

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .filter(|number| number.is_finite() && *number >= 0.0)
}

/// The plan's meters for one billing period: included, first-party and
/// third-party usage, the period's end and its length in seconds.
pub(super) struct Period {
    pub(super) used: [Option<f64>; 3],
    pub(super) reset: Option<String>,
    pub(super) seconds: Option<f64>,
}

fn seconds(
    start: Option<DateTime<FixedOffset>>,
    end: Option<DateTime<FixedOffset>>,
) -> Option<f64> {
    let (start, end) = start.zip(end).filter(|(start, end)| end > start)?;
    Some((end - start).num_milliseconds() as f64 / 1000.0)
}

/// The period in the dashboard's usage summary.
pub(super) fn summary_period(body: &Value) -> Period {
    let at = |key: &str| {
        let text = body[key].as_str()?;
        Some((text, DateTime::parse_from_rfc3339(text).ok()?))
    };
    let (start, end) = (at("billingCycleStart"), at("billingCycleEnd"));
    let plan = &body["individualUsage"]["plan"];
    let main = (plan["enabled"] != false)
        .then(|| number(&plan["totalPercentUsed"]).or_else(|| percentage(plan)))
        .flatten();
    Period {
        used: [
            main,
            number(&plan["autoPercentUsed"]),
            number(&plan["apiPercentUsed"]),
        ],
        reset: end.map(|(text, _)| text.to_owned()),
        seconds: seconds(start.map(|(_, at)| at), end.map(|(_, at)| at)),
    }
}

/// The period Cursor shows (`GetCurrentPeriodUsage`), whose bounds are
/// milliseconds. A plan metered in percentages (one merged into SuperGrok)
/// reports its meters, leaving out one at 0; a plan metered in dollars
/// reports cents spent of each limit, which give the share as Cursor's usage
/// bar works it out. A meter with neither is not shown.
pub(super) fn current_period(reply: &Value) -> Option<Period> {
    let plan = reply.get("planUsage").filter(|plan| plan.is_object())?;
    let at = |key: &str| {
        let ms = reply[key]
            .as_str()
            .and_then(|text| text.parse().ok())
            .or_else(|| reply[key].as_i64())?;
        Some(DateTime::from_timestamp_millis(ms)?.fixed_offset())
    };
    let (start, end) = (at("billingCycleStart"), at("billingCycleEnd"));
    let percents = ["totalPercentUsed", "autoPercentUsed", "apiPercentUsed"];
    let used = if percents.iter().any(|key| !plan[key].is_null()) {
        percents.map(|key| match &plan[key] {
            Value::Null => Some(0.0),
            value => number(value),
        })
    } else {
        // Cents, which the servers leave out at 0.
        let share = |spend: &str, limit: &str| {
            let limit = number(&plan[limit]).filter(|limit| *limit > 0.0)?;
            let spent = number(&plan[spend]).unwrap_or(0.0);
            Some((spent / limit * 100.0).min(100.0))
        };
        [
            share("includedSpend", "limit"),
            share("autoSpend", "autoLimit"),
            share("apiSpend", "apiLimit"),
        ]
    };
    Some(Period {
        used,
        reset: end.map(|end| end.to_rfc3339()),
        seconds: seconds(start, end),
    })
}
