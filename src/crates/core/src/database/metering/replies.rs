//! Which calls count toward the reply rate, and the rate they make.

use serde_json::{json, Value};

/// The fewest calls, and the least time they took in all, a reply rate is
/// worked out from (`reply_rate`): fewer is noise, so no rate.
const REPLY_RATE_CALLS: u64 = 3;
const REPLY_RATE_SPAN_MS: u64 = 5_000;

/// The calls the reply rate reads, as SQL over `usage_meter_event`: what
/// `reply_span` keeps, before its rate check.
pub(super) const REPLY_ROWS: &str = "output+reasoning>=10 AND (duration_ms BETWEEN 100 AND 600000
     OR (duration_ms IS NULL AND gap_ms BETWEEN 100 AND 120000))";

/// How a call counts toward the reply rate: its span and whether its log
/// timed it. A call the log timed counts its own time, up to 10 minutes; one
/// it did not counts the gap since its log's previous call, up to 120 s,
/// since a longer gap is the person's or a tool's. None for a call of under
/// 10 output tokens, a span under 100 ms, or over 10,000 tokens per second.
pub(crate) fn reply_span(
    tokens: u64,
    duration: Option<i64>,
    gap: Option<i64>,
) -> Option<(u64, bool)> {
    let (span, timed, cap) = match duration {
        Some(duration) => (duration, true, 600_000),
        None => (gap?, false, 120_000),
    };
    let span = u64::try_from(span)
        .ok()
        .filter(|span| (100..=cap).contains(span))?;
    (tokens >= 10 && tokens <= span.saturating_mul(10)).then_some((span, timed))
}

/// The reply rate of the calls in `sums` ([gap-timed, log-timed] output,
/// span and calls): the calls their logs timed when there are enough of
/// them, else the gap-timed ones, whose spans hold tool runs and the
/// person's time, so the rate is a lower bound. Enough is at least
/// `REPLY_RATE_CALLS` calls taking `REPLY_RATE_SPAN_MS` in all; with too few
/// either way there is no rate, which is unknown, not zero.
pub(crate) fn reply_rate(sums: [(u64, u64, u64); 2]) -> Option<Value> {
    let enough =
        |(_, span, calls): (u64, u64, u64)| calls >= REPLY_RATE_CALLS && span >= REPLY_RATE_SPAN_MS;
    let (lower_bound, (output, span, calls)) = if enough(sums[1]) {
        (false, sums[1])
    } else if enough(sums[0]) {
        (true, sums[0])
    } else {
        return None;
    };
    Some(
        json!({"tokensPerSecond":output as f64/(span as f64/1000.0),"calls":calls,"tokensOut":output,"lowerBound":lower_bound}),
    )
}
