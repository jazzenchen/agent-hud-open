//! MiniMax's `remains` answer: each model family's 5-hour and weekly windows,
//! as the services they meter.
use crate::account::status;
use crate::naming::window_names::Period;
use serde_json::{json, Value};

/// Why an answer is not a quota.
#[derive(Debug, PartialEq)]
pub(super) enum Answer {
    /// The key is not accepted here.
    Refused,
    /// A `base_resp` status other than success.
    Status(i64),
    /// No `model_remains`.
    Unrecognized,
}

/// What a plan's answer says: its name and its windows.
#[derive(Debug)]
pub(super) struct Parsed {
    pub(super) plan: Option<String>,
    pub(super) windows: Vec<Window>,
}

#[derive(Debug)]
pub(super) struct Window {
    key: String,
    pub(super) name: String,
    short: String,
    pub(super) used: f64,
    pub(super) reset: Option<i64>,
    pub(super) duration: f64,
    all_models: bool,
    /// Prompts used and allowed, where the plan counts them.
    pub(super) counts: Option<(f64, f64)>,
}
impl Window {
    pub(super) fn row(&self, account: &str, plan: Option<&str>) -> Value {
        let mut row = json!({"id":format!("{account}:{}",self.key),"vendor":"minimax","name":self.name,"shortLabel":self.short,"usedPercent":self.used,"resetAt":self.reset.and_then(iso),"status":status(self.used),"account":plan,"accountId":account,"allModels":self.all_models,"windowDurationSeconds":self.duration});
        if let Some((used, capacity)) = self.counts {
            row["nativeUsage"] = json!({"used":used,"capacity":capacity,"unit":"prompts"});
        }
        row
    }
}

fn iso(ms: i64) -> Option<String> {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms).map(|at| at.to_rfc3339())
}

/// A number, or a number written as a string ("96", "12%").
fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| {
            let text = value.as_str()?.trim();
            text.strip_suffix('%').unwrap_or(text).trim().parse().ok()
        })
        .filter(|n: &f64| n.is_finite())
}

/// An instant in epoch seconds or milliseconds, as milliseconds.
fn instant(value: &Value) -> Option<i64> {
    let raw = number(value)?;
    if raw > 1e12 {
        Some(raw as i64)
    } else if raw > 1e9 {
        Some((raw * 1000.0) as i64)
    } else {
        None
    }
}

/// A model family's name in MiniMax's words and the word a short name
/// keeps; None for the plan's text models.
fn service(model: &str) -> Option<(String, String)> {
    let lower = model.trim().to_lowercase();
    let named = |name: &str, word: &str| Some((name.to_owned(), word.to_owned()));
    if text_model(&lower) {
        None
    } else if lower == "video" {
        named("Video", "Video")
    } else if lower.contains("speech") {
        named("Text to Speech", "Speech")
    } else if lower.contains("hailuo") && lower.contains("fast") {
        named("Image to Video", "Video")
    } else if lower.contains("hailuo") {
        named("Text to Video", "Video")
    } else if lower.starts_with("image-") {
        named("Image Generation", "Image")
    } else if lower.contains("music") {
        named("Music Generation", "Music")
    } else {
        let word = model.split_whitespace().next().unwrap_or(model);
        named(model.trim(), word)
    }
}

/// The plan's text models: `general`, and the M series by name in older
/// answers.
fn text_model(lower: &str) -> bool {
    lower == "general" || lower.contains("minimax-m") || lower.starts_with("m2.")
}

/// The 5-hour (`current_interval_*`) or weekly (`current_weekly_*`) window of
/// one `model_remains` entry.
struct Lane {
    prefix: &'static str,
    start: &'static str,
    end: &'static str,
    remains: &'static str,
    key: &'static str,
    default_seconds: f64,
}
const LANES: [Lane; 2] = [
    Lane {
        prefix: "current_interval",
        start: "start_time",
        end: "end_time",
        remains: "remains_time",
        key: "interval",
        default_seconds: 18_000.0,
    },
    Lane {
        prefix: "current_weekly",
        start: "weekly_start_time",
        end: "weekly_end_time",
        remains: "weekly_remains_time",
        key: "weekly",
        default_seconds: 604_800.0,
    },
];

pub(super) fn parse(root: &Value, now: i64) -> Result<Parsed, Answer> {
    let data = if root["data"].is_object() {
        &root["data"]
    } else {
        root
    };
    let base = if data["base_resp"].is_object() {
        &data["base_resp"]
    } else {
        &root["base_resp"]
    };
    if let Some(code) = number(&base["status_code"]).filter(|code| *code != 0.0) {
        let message = base["status_msg"].as_str().unwrap_or("").to_lowercase();
        if code == 1004.0
            || ["invalid api key", "login", "log in", "cookie"]
                .iter()
                .any(|word| message.contains(word))
        {
            return Err(Answer::Refused);
        }
        return Err(Answer::Status(code as i64));
    }
    let remains = data["model_remains"]
        .as_array()
        .filter(|list| !list.is_empty())
        .ok_or(Answer::Unrecognized)?;
    let plan = [
        &data["current_subscribe_title"],
        &data["plan_name"],
        &data["combo_title"],
        &data["current_plan_title"],
        &data["current_combo_card"]["title"],
    ]
    .iter()
    .filter_map(|value| value.as_str().map(str::trim))
    .find(|name| !name.is_empty())
    .map(str::to_owned);
    let texts = remains
        .iter()
        .filter(|item| {
            item["model_name"]
                .as_str()
                .is_some_and(|model| text_model(&model.trim().to_lowercase()))
        })
        .count();
    let mut windows = Vec::new();
    for item in remains {
        let Some(model) = item["model_name"].as_str().filter(|m| !m.trim().is_empty()) else {
            continue;
        };
        let lower = model.trim().to_lowercase();
        // A second text family is named by its model, as any other.
        let named = service(model).or_else(|| {
            (texts > 1 && lower != "general")
                .then(|| (model.trim().to_owned(), model.trim().to_owned()))
        });
        for lane in &LANES {
            let field = |name: &str| &item[format!("{}_{name}", lane.prefix)];
            let total = number(field("total_count"));
            // CodexBar: the "usage" count is what is left.
            let left = number(field("usage_count"));
            let remaining_percent = number(field("remaining_percent")).or_else(|| {
                let used = number(field("used_percent")).filter(|used| *used >= 0.0)?;
                let limit = number(field("total_percent"))
                    .filter(|n| *n > 0.0)
                    .unwrap_or(100.0);
                Some((100.0 - used / limit * 100.0).clamp(0.0, 100.0))
            });
            let counted = total.filter(|total| *total > 0.0).zip(left);
            // Status 3 with nothing to count is a lane the plan leaves out
            // or does not limit.
            if number(field("status")) == Some(3.0)
                && counted.is_none()
                && remaining_percent.is_none_or(|left| left >= 100.0)
            {
                continue;
            }
            let (used, counts) = match (remaining_percent, counted) {
                (Some(left), _) => ((100.0 - left).clamp(0.0, 100.0), None),
                (None, Some((total, left))) => {
                    let used = (total - left).max(0.0);
                    (
                        (used / total * 100.0).clamp(0.0, 100.0),
                        Some((used, total)),
                    )
                }
                (None, None) => continue,
            };
            let start = instant(&item[lane.start]);
            let end = instant(&item[lane.end]);
            let duration = start
                .zip(end)
                .map(|(start, end)| (end - start) as f64 / 1000.0)
                .filter(|seconds| *seconds >= 60.0)
                .unwrap_or(lane.default_seconds);
            let reset = end.filter(|end| *end > now).or_else(|| {
                let left = number(&item[lane.remains]).filter(|n| *n > 0.0)?;
                // Milliseconds, as MiniMax writes it; a value that fits in
                // the window as seconds is seconds.
                let seconds = if left > duration { left / 1000.0 } else { left };
                Some(now + (seconds * 1000.0) as i64)
            });
            let period = Period::from_seconds(Some(duration));
            let window = match period {
                Some(Period::FiveHours) => "5-hour window".to_owned(),
                Some(Period::Week) => "Weekly window".to_owned(),
                Some(Period::Day) => "Daily window".to_owned(),
                Some(Period::Hours(hours)) => format!("{hours}-hour window"),
                Some(Period::Days(days)) => format!("{days}-day window"),
                _ => "Usage window".to_owned(),
            };
            let short = period.map_or_else(|| "Usage".to_owned(), Period::short_name);
            let (name, short) = match &named {
                None => (window, short),
                Some((name, word)) => (
                    format!("{name} · {window}"),
                    period.map_or_else(|| word.clone(), |p| format!("{word} {}", p.after_word())),
                ),
            };
            windows.push(Window {
                key: format!("{lower}:{}", lane.key),
                name,
                short,
                used,
                reset,
                duration,
                all_models: named.is_none(),
                counts,
            });
        }
    }
    Ok(Parsed { plan, windows })
}
