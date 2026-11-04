//! JSON as the phone writes it back, for the content revision both sides hash.
//!
//! The phone decodes a snapshot's records into its JSON value type and hashes
//! them encoded again by Swift's `JSONEncoder` with sorted keys and unescaped
//! slashes (`LocalNetworkSnapshot.contentRevision`). That type reads a number
//! as a 64-bit integer whenever it is a whole number in range, so `3.0` comes
//! back as `3`, and Swift writes other numbers in its own notation (`1e-05`).
//! serde_json writes `3.0` and `0.00001`, so the Mac hashes this form instead.
//! Keys sort by their UTF-8 bytes and strings escape the same on both sides.
use serde_json::{Number, Value};

/// `value` in the phone's form.
pub fn to_vec(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    write(value, &mut out);
    out
}

fn write(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(flag) => out.extend_from_slice(if *flag { b"true" } else { b"false" }),
        Value::Number(number) => out.extend_from_slice(number_text(number).as_bytes()),
        Value::String(text) => {
            // serde_json escapes exactly what Swift escapes: quote, backslash and
            // control characters, never a slash, DEL or U+2028.
            out.extend_from_slice(&serde_json::to_vec(text).expect("a string always encodes"));
        }
        Value::Array(items) => {
            out.push(b'[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                write(item, out);
            }
            out.push(b']');
        }
        Value::Object(map) => {
            let mut entries = map.iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            out.push(b'{');
            for (index, (key, item)) in entries.into_iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                out.extend_from_slice(&serde_json::to_vec(key).expect("a string always encodes"));
                out.push(b':');
                write(item, out);
            }
            out.push(b'}');
        }
    }
}

fn number_text(number: &Number) -> String {
    if let Some(whole) = number.as_i64() {
        return whole.to_string();
    }
    // A u64 above i64::MAX does not fit the phone's integer, so it is a double there.
    let value = number.as_f64().unwrap_or(0.0);
    double_text(value)
}

/// A double as the phone writes it: a whole number in 64-bit range as an
/// integer, others as Swift's `description` does.
pub fn double_text(value: f64) -> String {
    let sign = if value < 0.0 { "-" } else { "" };
    let (digits, exponent) = shortest(value.abs());
    if value.fract() == 0.0 {
        // The phone reads the decimal text, not the double, as its integer:
        // above 2^53 that is the shortest digits followed by zeros.
        let zeros = exponent - (digits.len() as i32 - 1);
        if (0..20).contains(&zeros) {
            let text = format!("{sign}{digits}{}", "0".repeat(zeros as usize));
            if let Ok(whole) = text.parse::<i64>() {
                return whole.to_string();
            }
        }
    }
    if (-4..16).contains(&exponent) {
        let body = if exponent >= 0 {
            let point = exponent as usize + 1;
            if digits.len() > point {
                format!("{}.{}", &digits[..point], &digits[point..])
            } else {
                format!("{digits}{}.0", "0".repeat(point - digits.len()))
            }
        } else {
            format!("0.{}{digits}", "0".repeat((-exponent - 1) as usize))
        };
        return format!("{sign}{body}");
    }
    let mantissa = match digits.len() {
        1 => digits.clone(),
        _ => format!("{}.{}", &digits[..1], &digits[1..]),
    };
    let direction = if exponent < 0 { '-' } else { '+' };
    format!("{sign}{mantissa}e{direction}{:02}", exponent.unsigned_abs())
}

/// The shortest significant digits that read back as `value` (not negative)
/// and the decimal exponent of the first. Rust and Swift agree on these except
/// when two candidates are equally close: Swift takes the even one.
fn shortest(value: f64) -> (String, i32) {
    if value == 0.0 {
        return ("0".into(), 0);
    }
    let (digits, exponent) = split(&format!("{value:e}"));
    let length = digits.len();
    // Enough exact digits to see a tie: a 5 right after the shortest digits
    // with nothing but zeros behind it.
    let (exact, exact_exponent) = split(&format!("{:.*e}", length + 40, value));
    let tie = exact_exponent == exponent
        && exact.as_bytes()[length] == b'5'
        && exact.as_bytes()[length + 1..]
            .iter()
            .all(|digit| *digit == b'0');
    if !tie {
        return (digits, exponent);
    }
    let lower = exact[..length].to_owned();
    let upper = (lower.parse::<u128>().unwrap_or(0) + 1).to_string();
    let reads_back = |candidate: &str| {
        candidate.len() == length
            && format!("{}.{}e{exponent}", &candidate[..1], &candidate[1..]).parse::<f64>()
                == Ok(value)
    };
    let even = |candidate: &str| candidate.as_bytes()[length - 1].is_multiple_of(2);
    let chosen = [lower, upper]
        .into_iter()
        .filter(|candidate| reads_back(candidate))
        .min_by_key(|candidate| !even(candidate));
    (chosen.unwrap_or(digits), exponent)
}

/// `d.ddde±x` as its digits and exponent.
fn split(scientific: &str) -> (String, i32) {
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("{:e} always has an exponent");
    let digits = mantissa.chars().filter(char::is_ascii_digit).collect();
    (
        digits,
        exponent.parse().expect("{:e} exponent is an integer"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the phone's `SyncJSON` writes back for this input, captured from
    /// Swift on 2026-10-21.
    #[test]
    fn writes_what_the_phone_writes_back() {
        let input = r#"{"b":1,"B":2,"a10":3,"a9":4,"_x":5,"Z":6,"aB":7,"ab":8,"é":9,"e":10,"a-b":11,"a_b":12,"aa":13,"a":14,"f":[0.0,3.0,-2.0,1.5,0.1,0.30000000000000004,0.0001,0.00001,0.000123456,1e-7,123456789.125,1e15,1e16,1.5e16,1e20,9.2233720368547758e18,1.7976931348623157e308,5e-324,-0.0,12.345678901234567,100.25,33.333333333333336],"s":"a/b\"c\\d\u0001\u001f\u007f\n\t\r\b\f é😀"}"#;
        let swift = "{\"B\":2,\"Z\":6,\"_x\":5,\"a\":14,\"a-b\":11,\"a10\":3,\"a9\":4,\"aB\":7,\"a_b\":12,\"aa\":13,\"ab\":8,\"b\":1,\"e\":10,\"f\":[0,3,-2,1.5,0.1,0.30000000000000004,0.0001,1e-05,0.000123456,1e-07,123456789.125,1000000000000000,10000000000000000,15000000000000000,1e+20,9.223372036854776e+18,1.7976931348623157e+308,5e-324,0,12.345678901234567,100.25,33.333333333333336],\"s\":\"a/b\\\"c\\\\d\\u0001\\u001f\u{7f}\\n\\t\\r\\b\\f\u{2028}é😀\",\"é\":9}";
        let value: Value = serde_json::from_str(input).unwrap();
        assert_eq!(String::from_utf8(to_vec(&value)).unwrap(), swift);
    }

    #[test]
    fn large_unsigned_numbers_are_doubles_on_the_phone() {
        let value: Value = serde_json::from_str("18446744073709551615").unwrap();
        assert_eq!(to_vec(&value), b"1.8446744073709552e+19");
    }

    /// Cases a 200,000-value comparison with Swift turned up (2026-10-21).
    #[test]
    // The literals are the exact doubles of each tie, written in full.
    #[allow(clippy::excessive_precision, clippy::inconsistent_digit_grouping)]
    fn ties_and_large_whole_numbers_as_swift_writes_them() {
        // Two shortest candidates equally close: Swift takes the even digit.
        assert_eq!(double_text(1_315_951_194_971_797.25), "1315951194971797.2");
        assert_eq!(
            double_text(-1_052_730_259_603_333.25),
            "-1052730259603333.2"
        );
        assert_eq!(double_text(9_688_531_951_154.0625), "9688531951154.062");
        // Above 2^53 the phone's integer is the decimal text, not the double.
        assert_eq!(double_text(690_799_285_879_927_296.0), "690799285879927300");
        assert_eq!(
            double_text(-9_223_372_036_854_775_808.0),
            "-9.223372036854776e+18"
        );
        assert_eq!(double_text(0.000_012_3), "1.23e-05");
    }
}
