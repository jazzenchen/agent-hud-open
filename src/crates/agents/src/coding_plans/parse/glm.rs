//! The GLM Coding Plan's answer (ZCode's plan's too): its quota limits, the
//! token, credit and MCP call windows, each told apart by kind, unit and length.
use crate::coding_plans::numeric;
use crate::l10n::tr;
use crate::naming::{vendor_windows::glm_name, window_names::Period};
use crate::transcript::metering::date_ms;
use serde_json::{json, Value};

/// Each of the plan's quota limits in `root`, as a window handed to `add`.
pub(super) fn windows(
    root: &Value,
    add: &mut impl FnMut(
        String,
        (String, String),
        f64,
        Value,
        Option<f64>,
        bool,
        Option<(f64, Option<f64>, &str)>,
    ),
) -> Result<(), String> {
    if root["success"] != true || numeric(&root["code"]) != Some(200.0) {
        return Err(tr!("GLM returned an unsupported quota format"));
    }
    let mut found = Vec::new();
    for value in root["data"]["limits"]
        .as_array()
        .ok_or_else(|| tr!("GLM quota windows are unavailable"))?
    {
        let Some(kind) = value["type"]
            .as_str()
            .filter(|s| matches!(*s, "TOKENS_LIMIT" | "CREDIT_LIMIT" | "TIME_LIMIT"))
        else {
            continue;
        };
        let unit = value["unit"]
            .as_i64()
            .ok_or_else(|| tr!("GLM quota window unit is unavailable"))?;
        let count = numeric(&value["number"])
            .ok_or_else(|| tr!("GLM quota window duration is unavailable"))?;
        let mut used = numeric(&value["percentage"])
            .ok_or_else(|| tr!("GLM quota percentage is unavailable"))?;
        let capacity = numeric(&value["usage"]);
        let current = numeric(&value["currentValue"]);
        if let Some(capacity) = capacity.filter(|n| *n > 0.0) {
            if let Some(count) = numeric(&value["remaining"])
                .map(|n| (capacity - n).max(current.unwrap_or(0.0)))
                .or(current)
            {
                used = count / capacity * 100.0;
            }
        }
        let duration = match unit {
            1 => Some(86400.0 * count),
            3 => Some(3600.0 * count),
            5 => Some(60.0 * count),
            6 => Some(604800.0 * count),
            _ => None,
        };
        let monthly = kind == "TIME_LIMIT" && unit == 5 && count == 1.0;
        let duration = if monthly { None } else { duration };
        let mut reset = numeric(&value["nextResetTime"])
            .and_then(|n| chrono::DateTime::<chrono::Utc>::from_timestamp_millis(n as i64))
            .map(|d| json!(d.to_rfc3339()))
            .unwrap_or(Value::Null);
        if kind != "TIME_LIMIT"
            && duration == Some(18000.0)
            && reset
                .as_str()
                .and_then(date_ms)
                .is_some_and(|at| at > chrono::Utc::now().timestamp_millis() + 18_060_000)
        {
            reset = Value::Null;
        }
        let native_unit = match kind {
            "CREDIT_LIMIT" => "credits",
            "TIME_LIMIT" => "calls",
            _ => "tokens",
        };
        found.push((
            format!("{kind}:{unit}:{count}"),
            kind,
            glm_name(kind, Period::from_seconds(duration), monthly),
            used,
            reset,
            duration,
            current.map(|used| (used.round(), capacity.map(f64::round), native_unit)),
        ));
    }
    // Token limits and credits of the same length, which an account
    // does not normally have both of, are told apart by their unit.
    let shorts = found
        .iter()
        .map(|window| window.2 .1.clone())
        .collect::<Vec<_>>();
    for (key, kind, mut name, used, reset, duration, native) in found {
        if shorts.iter().filter(|short| **short == name.1).count() > 1 && kind != "TIME_LIMIT" {
            if let Some(period) = Period::from_seconds(duration) {
                let unit = if kind == "CREDIT_LIMIT" {
                    "Credits"
                } else {
                    "Tokens"
                };
                name.1 = format!("{unit} {}", period.after_word());
            }
        }
        // The MCP window limits tool calls, not the plan's models.
        add(
            key,
            name,
            used,
            reset,
            duration,
            kind != "TIME_LIMIT",
            native,
        );
    }
    Ok(())
}
