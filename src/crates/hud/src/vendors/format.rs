//! What a HUD writes beside a mark: a window's share, a balance, or the last 24
//! hours, as src/packages/hud/src/model.ts and format.ts write them.
use super::*;
use serde_json::json;

/// A display percentage; stored readings and quota severity always use consumed usage.
pub fn quota_percent(used: f64, remaining: bool) -> f64 {
    let used = used.clamp(0.0, 100.0);
    if remaining {
        100.0 - used
    } else {
        used
    }
}

/// The number a Simple HUD writes beside a mark: its tightest window (held
/// from the last reading when it failed), else its balance, else its last 24
/// hours (model.ts `meterLabel`).
pub fn meter_label(vendor: &Vendor, remaining: bool) -> String {
    match vendor.used.or(vendor.held) {
        Some(used) => format!("{}%", quota_percent(used, remaining).round()),
        None => vendor
            .balance
            .clone()
            .or_else(|| vendor.usage.clone())
            .unwrap_or_else(|| "--".into()),
    }
}

/// What the clients behind mark `key` used in the last 24 hours, as the
/// snapshot's provider rows report it (`recent`), in a few characters
/// (model.ts `recentLabel`): "≈$1.20" at list prices, with "+" when some of
/// it has no list price or is in another currency, else "1.2M" tokens. None
/// when they used nothing.
pub fn recent_label(snapshot: &Value, key: &str) -> Option<String> {
    let mut tokens = 0.0;
    let mut unpriced = 0.0;
    let mut costs = std::collections::BTreeMap::<String, f64>::new();
    for recent in list(snapshot, "providers")
        .filter(|provider| mark(provider["id"].as_str().unwrap_or("")) == key)
        .map(|provider| &provider["recent"])
        .filter(|recent| recent.is_object())
    {
        tokens += recent["tokens"].as_f64().unwrap_or(0.0);
        unpriced += recent["unpricedTokens"].as_f64().unwrap_or(0.0);
        for (currency, amount) in recent["costs"].as_object().into_iter().flatten() {
            *costs.entry(currency.clone()).or_default() += amount.as_f64().unwrap_or(0.0);
        }
    }
    if tokens <= 0.0 {
        return None;
    }
    let shown = costs
        .get_key_value("USD")
        .or_else(|| costs.iter().next())
        .map(|(currency, amount)| (currency.clone(), *amount));
    Some(match shown {
        Some((_, amount)) if amount == 0.0 && unpriced <= 0.0 && costs.len() == 1 => {
            // Calls to a model on this computer cost nothing.
            if snapshot["language"] == "zh" {
                "免费"
            } else {
                "Free"
            }
            .to_owned()
        }
        Some((currency, amount)) => {
            let more = unpriced > 0.0 || costs.len() > 1;
            format!(
                "≈{}{}",
                money_short(amount, &currency),
                if more { "+" } else { "" }
            )
        }
        None => tokens_short(tokens),
    })
}

/// format.ts `tokensShort`: 48_000 → "48k", 1_260 → "1.3k", 950 → "950",
/// 2_400_000 → "2.4M"; a value that rounds up to the next unit reads in it.
pub fn tokens_short(tokens: f64) -> String {
    const UNITS: [(f64, &str); 3] = [(1e9, "B"), (1e6, "M"), (1e3, "k")];
    // One decimal below ten, none from ten up; ties to even, as printf.
    let trim = |value: f64| {
        let one = format!("{value:.1}");
        if value >= 10.0 || one.parse::<f64>().unwrap_or(0.0) >= 10.0 {
            format!("{value:.0}")
        } else {
            one
        }
    };
    for (index, (size, unit)) in UNITS.iter().enumerate() {
        if tokens < *size {
            continue;
        }
        let shown = trim(tokens / size);
        if index > 0 && shown.parse::<f64>().unwrap_or(0.0) >= 1000.0 {
            let (larger, larger_unit) = UNITS[index - 1];
            return trim(tokens / larger) + larger_unit;
        }
        return shown + unit;
    }
    format!("{}", tokens.trunc())
}

/// format.ts `moneyShort`: whole units from a hundred up ("$412"), two
/// decimals below ("$3.10"), "<$0.01" for a positive amount that rounds to none.
pub fn money_short(amount: f64, code: &str) -> String {
    let cents = format!("{amount:.2}");
    let rounded = cents.parse::<f64>().unwrap_or(0.0);
    if rounded == 0.0 && amount > 0.0 {
        return format!("<{}", money_amount(&json!("0.01"), code));
    }
    if rounded.abs() >= 100.0 {
        let text = money_amount(&json!(format!("{rounded:.0}")), code);
        return text.strip_suffix(".00").map(str::to_owned).unwrap_or(text);
    }
    money_amount(&json!(cents), code)
}

/// A balance with its currency's narrow symbol and two decimals, rounded half
/// to even, as format.ts `moneyAmount` writes it: "$12.99", "¥1,234.50",
/// "-$3.00". Both languages write it the same way.
pub fn money_amount(amount: &Value, code: &str) -> String {
    let decimal = match amount {
        Value::String(text) => text.trim().to_owned(),
        // The shortest form that reads back as the same number, as Intl reads it.
        Value::Number(number) => number.as_f64().map(|v| v.to_string()).unwrap_or_default(),
        _ => String::new(),
    };
    let (negative, digits) = match decimal.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, decimal.strip_prefix('+').unwrap_or(&decimal)),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty() && fraction.is_empty()
        || !whole
            .chars()
            .chain(fraction.chars())
            .all(|c| c.is_ascii_digit())
    {
        return format!("{code} {decimal}");
    }
    // Hundredths, rounded half to even on the decimal digits.
    let mut cents: u128 = format!("{whole}{:0<2}", fraction.get(..2).unwrap_or(fraction))
        .trim_start_matches('0')
        .parse()
        .unwrap_or(0);
    let rest = fraction.get(2..).unwrap_or("");
    let first = rest.bytes().next().unwrap_or(b'0');
    let beyond = rest.bytes().skip(1).any(|b| b != b'0');
    if first > b'5' || first == b'5' && (beyond || cents % 2 == 1) {
        cents += 1;
    }
    let units = (cents / 100).to_string();
    let mut grouped = String::new();
    for (index, digit) in units.chars().enumerate() {
        if index > 0 && (units.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    let symbol = match code {
        "USD" | "AUD" | "CAD" | "HKD" | "NZD" | "SGD" | "TWD" | "MXN" => "$",
        "CNY" | "JPY" => "¥",
        "EUR" => "€",
        "GBP" => "£",
        "KRW" => "₩",
        "INR" => "₹",
        _ => "",
    };
    let sign = if negative && cents > 0 { "-" } else { "" };
    let amount = format!("{grouped}.{:02}", cents % 100);
    if symbol.is_empty() {
        format!("{sign}{code}\u{a0}{amount}")
    } else {
        format!("{sign}{symbol}{amount}")
    }
}
