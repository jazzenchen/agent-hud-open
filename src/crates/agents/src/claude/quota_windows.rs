//! Claude's plan windows as the engine and claude.ai report them, and the
//! reading they make for the account a profile names.
use crate::account::{
    account_id, client_home_key, legacy_account, legacy_unresolved, status, Host, Reading,
};
use crate::naming::window_names;
use serde_json::{json, Value};
use std::path::Path;

pub(super) fn identity(value: &Value) -> Option<String> {
    let user = value["accountUuid"].as_str().unwrap_or_default();
    let organization = value["organizationUuid"].as_str().unwrap_or_default();
    (!user.is_empty() || !organization.is_empty())
        .then(|| account_id("claude", &[user, organization]))
}
pub(super) fn plan(profile: &Value, fallback: Option<&str>) -> Option<String> {
    let fallback = fallback?;
    let mapped = match profile["organizationType"].as_str() {
        Some("claude_max") => "max",
        Some("claude_pro") => "pro",
        Some("claude_team") => "team",
        Some("claude_enterprise") => "enterprise",
        _ => fallback,
    };
    let value = if mapped == "max" && profile["organizationType"] == "claude_max" {
        match profile["userRateLimitTier"]
            .as_str()
            .or_else(|| profile["organizationRateLimitTier"].as_str())
        {
            Some("default_claude_max_5x") => "max_5x",
            Some("default_claude_max_20x") => "max_20x",
            _ => mapped,
        }
    } else {
        mapped
    };
    Some(value.into())
}
/// A profile's plan without the engine's word for it: the organization's
/// type, or failing that its rate-limit tier.
pub(super) fn profile_plan(profile: &Value) -> Option<String> {
    let tier = profile["userRateLimitTier"]
        .as_str()
        .or_else(|| profile["organizationRateLimitTier"].as_str());
    let guess = profile["organizationType"]
        .as_str()
        .and_then(|kind| kind.strip_prefix("claude_"))
        .or_else(|| {
            tier.and_then(|tier| {
                ["max", "pro", "team", "enterprise"]
                    .into_iter()
                    .find(|plan| tier.contains(plan))
            })
        })?;
    plan(profile, Some(guess))
}

/// A reading of `windows` for the account `who` names in Claude Code's
/// profile fields (accountUuid, organizationUuid, emailAddress). Both
/// sign-ins name one account the same way, so its windows carry on from one
/// to the other. A login that names no account is the configuration
/// `folder`'s.
pub(super) fn plan_reading(
    host: &dyn Host,
    windows: Vec<Window>,
    who: &Value,
    plan: Option<String>,
    folder: &Path,
) -> Reading {
    let id = identity(who).unwrap_or_else(|| {
        account_id("claude", &["unresolved", folder.to_string_lossy().as_ref()])
    });
    let label = who["emailAddress"].as_str();
    let legacy = legacy_account(
        "Claude",
        who["accountUuid"].as_str().unwrap_or(""),
        who["organizationUuid"].as_str().unwrap_or(""),
    );
    let account_identity = legacy.unwrap_or_else(|| {
        legacy_unresolved(
            "Claude",
            &client_home_key(host, folder, &host.home().join(".claude")),
        )
    });
    let mut reading = Reading::empty("claude");
    reading.account =
        Some(json!({"id":account_identity,"provider":"claude","label":label,"plan":plan}));
    for window in windows {
        let Window {
            key,
            name,
            percent,
            reset,
            family,
        } = window;
        // "5h", "Weekly", and a family's weekly window by the family alone.
        let short = match family.as_deref() {
            Some(_) => window_names::leading(name.trim_start_matches("Weekly limit · ")),
            None if key == "claude-session" => window_names::Period::FiveHours.short_name(),
            None => window_names::Period::Week.short_name(),
        };
        reading.quotas.push(json!({"id":format!("{id}:{key}"),"vendor":"claude","name":name,"shortLabel":short,"usedPercent":percent,"resetAt":reset,"status":status(percent),"account":label,"accountId":account_identity,"legacyWindowId":format!("{account_identity}/{key}"),"consumerSource":"claude","windowDurationSeconds":if key=="claude-session"{5*3600}else{7*86400},"allModels":family.is_none(),"modelFamilies":family.map(|family|vec![family])}));
    }
    reading
}

/// One plan window as the engine reports it.
pub(super) struct Window {
    pub(super) key: String,
    pub(super) name: String,
    pub(super) percent: f64,
    pub(super) reset: Option<String>,
    family: Option<String>,
}

/// The model families Claude names (agent-hud-native-open ClaudeModelInfo.families).
const FAMILIES: [&str; 5] = ["opus", "sonnet", "haiku", "fable", "mythos"];

/// A model id's or name's family, lowercased: the first of its words that
/// names one (agent-hud-native-open ClaudeModelInfo.parse).
fn family(model: &str) -> Option<&'static str> {
    model
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .find_map(|word| FAMILIES.into_iter().find(|family| *family == word))
}

/// The windows of a `rate_limits` object in the order Claude Code's own
/// /usage screen shows them: the session, the week across models, then each
/// model family's week, families in alphabetical order (agent-hud-native-open
/// ClaudeUsage.rows). A family's week comes from `seven_day_<family>` or a
/// `limits[]` entry of kind `weekly_scoped`, which wins. None when the
/// object has neither the session nor the weekly window, as Open reads it.
pub(super) fn windows(limits: &Value) -> Option<Vec<Window>> {
    let window = |object: &Value, value_key: &str| {
        let percent = object[value_key]
            .as_f64()
            .filter(|percent| percent.is_finite())?;
        let reset = object["resets_at"]
            .as_str()
            .filter(|value| chrono::DateTime::parse_from_rfc3339(value).is_ok())
            .map(str::to_owned);
        Some((percent, reset))
    };
    let session = window(&limits["five_hour"], "utilization");
    let week = window(&limits["seven_day"], "utilization");
    if session.is_none() && week.is_none() {
        return None;
    }
    // Open reads `seven_day_opus` and `seven_day_sonnet`; the other families'
    // keys are read too, so a family window the service adds is not missed.
    let mut families = std::collections::BTreeMap::<String, (f64, Option<String>)>::new();
    for family in FAMILIES {
        if let Some(found) = window(
            &limits[format!("seven_day_{family}").as_str()],
            "utilization",
        ) {
            families.insert(family.into(), found);
        }
    }
    for entry in limits["limits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["kind"] == "weekly_scoped")
    {
        let model = &entry["scope"]["model"];
        let name = model["display_name"]
            .as_str()
            .or_else(|| model["id"].as_str());
        let Some(key) = name
            .and_then(family)
            .map(str::to_owned)
            .or_else(|| model["display_name"].as_str().map(str::to_lowercase))
        else {
            continue;
        };
        if let Some(found) = window(entry, "percent") {
            families.insert(key, found);
        }
    }
    let mut rows = Vec::new();
    for (key, name, found) in [
        ("claude-session", "Current session", session),
        ("claude-weekly", "Weekly limit · All models", week),
    ] {
        if let Some((percent, reset)) = found {
            rows.push(Window {
                key: key.into(),
                name: name.into(),
                percent,
                reset,
                family: None,
            });
        }
    }
    for (family, (percent, reset)) in families {
        let mut letters = family.chars();
        let capitalized = letters
            .next()
            .map(|first| first.to_uppercase().chain(letters).collect::<String>())
            .unwrap_or_default();
        rows.push(Window {
            key: format!("claude-weekly-{family}"),
            name: format!("Weekly limit · {capitalized}"),
            percent,
            reset,
            family: Some(family),
        });
    }
    Some(rows)
}
