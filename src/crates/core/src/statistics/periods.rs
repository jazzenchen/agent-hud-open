//! Calendar helpers: a query's periods, local days, and 15-minute bucket
//! bounds.
use super::query::{Query, QUARTER};
use chrono::{NaiveDate, TimeZone, Timelike};
use chrono_tz::Tz;

pub(super) fn periods(query: &Query) -> Result<Vec<(i64, i64)>, String> {
    let mut result = Vec::new();
    if query.bucket == 1440 {
        let mut start = day_start(query.zone, local_date(query.zone, query.from))?;
        while start < query.to {
            let date = local_date(query.zone, start).succ_opt().unwrap();
            let end = day_start(query.zone, date)?;
            result.push((start, end));
            start = end;
        }
    } else {
        let local = query
            .zone
            .timestamp_millis_opt(query.from)
            .single()
            .unwrap();
        let residual = (i64::from(local.minute()) % query.bucket) * 60_000
            + i64::from(local.second()) * 1000
            + i64::from(local.timestamp_subsec_millis());
        let mut start = query.from - residual;
        let step = query.bucket * 60_000;
        while start < query.to {
            result.push((start, start + step));
            start += step;
        }
    }
    Ok(result)
}

pub(crate) fn day_start(zone: Tz, mut date: NaiveDate) -> Result<i64, String> {
    for _ in 0..3 {
        for minute in 0..1440 {
            let local = date.and_hms_opt(minute / 60, minute % 60, 0).unwrap();
            if let Some(at) = zone.from_local_datetime(&local).earliest() {
                return Ok(at.timestamp_millis());
            }
        }
        date = date.succ_opt().unwrap();
    }
    Err("No valid instant for this calendar day".into())
}

pub(crate) fn local_date(zone: Tz, at: i64) -> NaiveDate {
    zone.timestamp_millis_opt(at).single().unwrap().date_naive()
}

pub(super) fn floor(at: i64) -> i64 {
    at.div_euclid(QUARTER) * QUARTER
}

pub(super) fn ceil(at: i64) -> i64 {
    floor(at)
        + (if at.rem_euclid(QUARTER) == 0 {
            0
        } else {
            QUARTER
        })
}

pub(super) fn overlaps(at: i64, from: i64, to: i64) -> bool {
    at + QUARTER > from && at < to
}
