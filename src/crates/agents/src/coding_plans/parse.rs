//! What each plan's usage service answers: Kimi Code's usages, OpenCode Go's
//! usage and the GLM Coding Plan's quota limits, as quota windows.
mod glm;

use super::{numeric, Credential};
use crate::account::status;
use crate::l10n::tr;
use crate::naming::{vendor_windows::kimi_name, window_names::Period};
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
                    let count = numeric(&spec["duration"])
                        .filter(|n| *n > 0.0)
                        .ok_or_else(|| tr!("Kimi quota window has no duration"))?;
                    let unit = spec["timeUnit"]
                        .as_str()
                        .ok_or_else(|| tr!("Kimi quota window has no unit"))?;
                    let multiplier = match unit {
                        "TIME_UNIT_MINUTE" => 60.0,
                        "TIME_UNIT_HOUR" => 3600.0,
                        "TIME_UNIT_DAY" => 86400.0,
                        "TIME_UNIT_WEEK" => 604800.0,
                        _ => return Err(tr!("Kimi quota window unit is unsupported")),
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
                        .ok_or_else(|| tr!("Kimi quota capacity is unavailable"))?;
                    let used = numeric(&value["used"])
                        .or_else(|| numeric(&value["remaining"]).map(|n| (limit - n).max(0.0)))
                        .ok_or_else(|| tr!("Kimi quota usage is unavailable"))?;
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
                return Err(tr!("OpenCode Go returned an unsupported quota format"));
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
                    numeric(&value["percent"])
                        .ok_or_else(|| tr!("OpenCode Go quota percentage is unavailable"))?,
                    date(&value["resetsAt"]),
                    duration,
                    true,
                    None,
                );
            }
        }
        "glm" | "zcode" => glm::windows(root, &mut add)?,
        _ => unreachable!(),
    }
    if windows.is_empty() {
        return Err(tr!("Coding plan did not return a verifiable quota window"));
    }
    for row in &mut windows {
        link(row, entry);
    }
    Ok((windows, plan))
}

/// A Kimi Code account as its profile names it (`/me`, which Kimi Code's own
/// account panel reads): the person's email, else nickname, and the name of
/// their membership level. Without a profile, the plan the usage answer
/// named (`membership.level`, which older answers carry) names both.
pub(super) fn kimi_account(
    profile: &Value,
    plan: Option<String>,
) -> (Option<String>, Option<String>) {
    let field = |key: &str| {
        profile[key]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let plan = field("user_level_name").or(plan);
    let label = field("email")
        .or_else(|| field("nickname"))
        .or_else(|| plan.clone());
    (label, plan)
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
        "glm" | "zcode" if china => (&["zhipuai-coding-plan", "pi#zai-coding-cn"], Some("glm")),
        "glm" | "zcode" => (&["zai-coding-plan", "pi#zai"], Some("glm")),
        "opencode" => (&["opencode-go"], None),
        _ => return,
    };
    row["consumerRoutes"] = json!(routes);
    if entry.provider == "zcode" {
        row["consumerSource"] = json!("zcode");
    } else if let Some(family) = family.filter(|_| entry.clients.contains("Claude")) {
        row["consumerSource"] = json!("claude");
        if row["allModels"] == true {
            row["modelFamilies"] = json!([family]);
        }
    }
}
