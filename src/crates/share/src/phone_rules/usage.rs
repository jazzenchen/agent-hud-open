//! A day of usage: `SyncPayload.Usage` and its rules.
use super::{decimal, ensure, payload, Check};
use crate::{
    keys::{BUCKET_MS, DAY_MS},
    record::SyncRecord,
};

pub(super) fn check(record: &SyncRecord) -> Check {
    let value = payload(record)?;
    let day = value.text("day")?;
    let mut starts = Vec::new();
    for tokens in value.objects("tokens", "tokens")? {
        let start = tokens.int("startMs")?;
        let agent = tokens.text("agentId")?;
        tokens.text_opt("account")?;
        let counts = [
            tokens.int("in")?,
            tokens.int("out")?,
            tokens.int("cacheRead")?,
        ];
        tokens.int_opt("cacheWrite")?;
        tokens.int_opt("reasoning")?;
        if let Some(cost) = tokens.object_opt("listCost", "listCost")? {
            cost.text("currency")?;
            for part in ["input", "cacheWrite", "cacheRead", "output", "reasoning"] {
                cost.text_opt(part)?;
            }
        }
        ensure(!agent.is_empty(), || "tokens without a model".into())?;
        ensure(counts.iter().all(|n| *n >= 0), || {
            format!("negative tokens for {agent}")
        })?;
        starts.push(start);
    }
    for cost in value.objects("costs", "cost")? {
        let start = cost.int("startMs")?;
        let billing = cost.text("billingID")?;
        let amounts = cost.text_map("amounts")?;
        ensure(!billing.is_empty(), || "a cost without a billing id".into())?;
        ensure(!amounts.is_empty(), || format!("no amounts for {billing}"))?;
        for (currency, amount) in amounts {
            ensure(!currency.is_empty(), || {
                format!("an amount without a currency for {billing}")
            })?;
            ensure(decimal(amount), || {
                format!("{billing}'s {currency} amount is not a number")
            })?;
        }
        starts.push(start);
    }
    ensure(record.key == day, || "key is not its day".into())?;
    let day_start = day_start(day).ok_or_else(|| format!("{day} is not a day"))?;
    ensure(
        starts
            .iter()
            .all(|s| s % BUCKET_MS == 0 && *s >= day_start && *s < day_start + DAY_MS),
        || "a bucket outside its day".into(),
    )
}

/// UTC midnight of a `yyyy-MM-dd` day, as the phone's `SyncBucket.dayStart(of:)`
/// reads it: four, two and two ASCII digits naming a real date.
fn day_start(day: &str) -> Option<i64> {
    let parts = day.split('-').collect::<Vec<_>>();
    let shaped = parts.iter().map(|p| p.len()).eq([4, 2, 2])
        && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()));
    if !shaped {
        return None;
    }
    let [year, month, date] = [0, 1, 2].map(|i| parts[i].parse::<i64>().unwrap_or(0));
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=12).contains(&month) || !(1..=days[month as usize - 1]).contains(&date) {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * if month > 2 { month - 3 } else { month + 9 } + 2) / 5 + date - 1;
    Some((era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468) * DAY_MS)
}

#[cfg(test)]
pub(super) fn day_start_for_tests(day: &str) -> Option<i64> {
    day_start(day)
}
