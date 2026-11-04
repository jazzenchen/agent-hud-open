//! What each plan's usage service answers: Kimi Code's usages, OpenCode Go's
//! usage and the GLM Coding Plan's quota limits, as quota windows.
use super::{numeric, Credential};
use crate::account::status;
use crate::l10n;
use crate::naming::{
    vendor_windows::{glm_name, kimi_name},
    window_names::Period,
};
use crate::transcript::metering::date_ms;
use serde_json::{json, Value};

fn date(value: &Value) -> Value {
    value
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| json!(d.to_rfc3339()))
        .unwrap_or(Value::Null)
}
pub(super) fn parse(
    root: &Value,
    entry: &Credential,
    id: &str,
) -> Result<(Vec<Value>, Option<String>), String> {
    let mut windows = Vec::new();
    let plan = if entry.provider == "kimi" {
        root["membership"]["level"].as_str()
    } else {
        root["data"]["planName"]
            .as_str()
            .or_else(|| root["data"]["level"].as_str())
    }
    .map(str::to_owned);
    let mut add = |key: String,
                   (name, short): (String, String),
                   used: f64,
                   reset: Value,
                   duration: Option<f64>,
                   all_models: bool,
                   native: Option<(f64, Option<f64>, &str)>| {
        let used = used.min(100.0);
        let mut row = json!({"id":format!("{id}:{key}"),"vendor":entry.provider,"name":name,"shortLabel":short,"usedPercent":used,"resetAt":reset,"status":status(used),"account":plan,"accountId":id,"allModels":all_models});
        if let Some(duration) = duration {
            row["windowDurationSeconds"] = json!(duration);
        }
        if let Some((used, capacity, unit)) = native {
            row["nativeUsage"] = json!({"used":used,"capacity":capacity,"unit":unit});
        }
        windows.push(row);
    };
    match entry.provider {
        "kimi" => {
            let mut current = false;
            for (field, key, name, duration) in [
                (
                    "limit_5h",
                    "limit:TIME_UNIT_MINUTE:300.0",
                    kimi_name(Some(Period::FiveHours)),
                    Some(18000.0),
                ),
                (
                    "limit_7d",
                    "weekly",
                    kimi_name(Some(Period::Week)),
                    Some(604800.0),
                ),
                (
                    "limit_month_total",
                    "monthly",
                    ("Monthly total quota".into(), Period::Month.short_name()),
                    None,
                ),
            ] {
                let value = &root["usages"][field];
                if let Some(ratio) = numeric(&value["used_ratio"]) {
                    current = true;
                    add(
                        key.into(),
                        name,
                        ratio * 100.0,
                        date(&value["reset_time"]),
                        duration,
                        true,
                        None,
                    );
                }
            }
            if !current {
                let mut old = Vec::new();
                if root["usage"].is_object() {
                    old.push(("weekly".into(), 604800.0, &root["usage"]));
                }
                for value in root["limits"].as_array().into_iter().flatten() {
                    let spec = &value["window"];
                    let count =
                        numeric(&spec["duration"])
                            .filter(|n| *n > 0.0)
                            .ok_or_else(|| {
                                l10n::text(
                                    "Kimi 额度窗口缺少时长",
                                    "Kimi quota window has no duration",
                                )
                            })?;
                    let unit = spec["timeUnit"].as_str().ok_or_else(|| {
                        l10n::text("Kimi 额度窗口缺少单位", "Kimi quota window has no unit")
                    })?;
                    let multiplier = match unit {
                        "TIME_UNIT_MINUTE" => 60.0,
                        "TIME_UNIT_HOUR" => 3600.0,
                        "TIME_UNIT_DAY" => 86400.0,
                        "TIME_UNIT_WEEK" => 604800.0,
                        _ => {
                            return Err(l10n::text(
                                "Kimi 额度窗口的单位无法识别",
                                "Kimi quota window unit is unsupported",
                            ))
                        }
                    };
                    old.push((
                        format!("limit:{unit}:{count:.1}"),
                        count * multiplier,
                        &value["detail"],
                    ));
                }
                for (key, duration, value) in old {
                    let limit = numeric(&value["limit"])
                        .filter(|n| *n > 0.0)
                        .ok_or_else(|| {
                            l10n::text("Kimi 未提供额度总量", "Kimi quota capacity is unavailable")
                        })?;
                    let used = numeric(&value["used"])
                        .or_else(|| numeric(&value["remaining"]).map(|n| (limit - n).max(0.0)))
                        .ok_or_else(|| {
                            l10n::text("Kimi 未提供已用额度", "Kimi quota usage is unavailable")
                        })?;
                    add(
                        key,
                        kimi_name(Period::from_seconds(Some(duration))),
                        used / limit * 100.0,
                        date(&value["resetTime"]),
                        Some(duration),
                        true,
                        Some((used, Some(limit), "units")),
                    );
                }
            }
        }
        "opencode" => {
            if !root["usage"].is_object() {
                return Err(l10n::text(
                    "OpenCode Go 返回的额度格式无法识别",
                    "OpenCode Go returned an unsupported quota format",
                ));
            }
            for key in ["rolling", "weekly", "monthly"] {
                let value = &root["usage"][key];
                if value.is_null() {
                    continue;
                }
                let duration = match key {
                    "rolling" => Some(18000.0),
                    "weekly" => Some(604800.0),
                    _ => None,
                };
                // OpenCode's names for Go's limits; the service calls the 5
                // hours rolling.
                let name = match key {
                    "rolling" => ("5-hour limit".into(), Period::FiveHours.short_name()),
                    "weekly" => ("Weekly limit".into(), Period::Week.short_name()),
                    _ => ("Monthly limit".into(), Period::Month.short_name()),
                };
                add(
                    key.into(),
                    name,
                    numeric(&value["percent"]).ok_or_else(|| {
                        l10n::text(
                            "OpenCode Go 未提供额度比例",
                            "OpenCode Go quota percentage is unavailable",
                        )
                    })?,
                    date(&value["resetsAt"]),
                    duration,
                    true,
                    None,
                );
            }
        }
        "glm" => {
            if root["success"] != true || numeric(&root["code"]) != Some(200.0) {
                return Err(l10n::text(
                    "GLM 返回的额度格式无法识别",
                    "GLM returned an unsupported quota format",
                ));
            }
            let mut found = Vec::new();
            for value in root["data"]["limits"].as_array().ok_or_else(|| {
                l10n::text("GLM 未提供额度窗口", "GLM quota windows are unavailable")
            })? {
                let Some(kind) = value["type"]
                    .as_str()
                    .filter(|s| matches!(*s, "TOKENS_LIMIT" | "CREDIT_LIMIT" | "TIME_LIMIT"))
                else {
                    continue;
                };
                let unit = value["unit"].as_i64().ok_or_else(|| {
                    l10n::text(
                        "GLM 额度窗口缺少单位",
                        "GLM quota window unit is unavailable",
                    )
                })?;
                let count = numeric(&value["number"]).ok_or_else(|| {
                    l10n::text(
                        "GLM 额度窗口缺少时长",
                        "GLM quota window duration is unavailable",
                    )
                })?;
                let mut used = numeric(&value["percentage"]).ok_or_else(|| {
                    l10n::text("GLM 未提供额度比例", "GLM quota percentage is unavailable")
                })?;
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
                if shorts.iter().filter(|short| **short == name.1).count() > 1
                    && kind != "TIME_LIMIT"
                {
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
        }
        _ => unreachable!(),
    }
    if windows.is_empty() {
        return Err(l10n::text(
            "Coding Plan 没有返回可核实的额度窗口",
            "Coding plan did not return a verifiable quota window",
        ));
    }
    for row in &mut windows {
        link(row, entry);
    }
    Ok((windows, plan))
}

/// The logged calls that spend a plan: those through its own provider ids,
/// which no pay-as-you-go key uses ("pi#zai" is Pi's alone: OpenCode's `zai`
/// is pay-as-you-go), and, when Claude Code's address is the plan's,
/// Claude Code's calls to the vendor's models.
pub(super) fn link(row: &mut Value, entry: &Credential) {
    let china = entry.base.contains("bigmodel.cn");
    let (routes, family): (&[&str], Option<&str>) = match entry.provider {
        "kimi" => (
            &["kimi-code", "kimi-for-coding", "kimi-coding"],
            Some("kimi"),
        ),
        "glm" if china => (&["zhipuai-coding-plan", "pi#zai-coding-cn"], Some("glm")),
        "glm" => (&["zai-coding-plan", "pi#zai"], Some("glm")),
        "opencode" => (&["opencode-go"], None),
        _ => return,
    };
    row["consumerRoutes"] = json!(routes);
    if let Some(family) = family.filter(|_| entry.clients.contains("Claude")) {
        row["consumerSource"] = json!("claude");
        if row["allModels"] == true {
            row["modelFamilies"] = json!([family]);
        }
    }
}
