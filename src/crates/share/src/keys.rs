//! The keys and time buckets usage records are filed under.
pub const SHARED: &str = "default";
pub const BUCKET_MS: i64 = 900_000;
pub const DAY_MS: i64 = 86_400_000;
pub fn bucket_start(ms: i64) -> i64 {
    ms - ms.rem_euclid(BUCKET_MS)
}
pub fn day_start(ms: i64) -> i64 {
    ms - ms.rem_euclid(DAY_MS)
}
pub fn day(ms: i64) -> String {
    let z = day_start(ms) / DAY_MS + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    let date = doy - (153 * mp + 2) / 5 + 1;
    format!("{year:04}-{month:02}-{date:02}")
}
