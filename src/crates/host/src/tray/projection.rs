//! What the menu bar item shows, projected from a snapshot: the highest
//! usage, each agent's level for the gradient, and the menu's entries.
use crate::vendors::Vendor;
use agent_hud_runtime::l10n::{self, tr_in, Language};
use serde_json::Value;

#[derive(Clone, PartialEq)]
pub(crate) struct Presentation {
    pub(super) language: Language,
    pub(super) visible: bool,
    pub(super) glow_hidden: bool,
    pub(super) light: bool,
    pub(super) used_percent: Option<f64>,
    pub(super) remaining: bool,
    pub(super) levels: Vec<Level>,
    pub(super) entries: Vec<Entry>,
    /// The product's newer version, for New Version….
    pub(super) update: Option<String>,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            language: l10n::current(),
            visible: true,
            glow_hidden: false,
            light: false,
            used_percent: None,
            remaining: false,
            levels: vec![],
            entries: vec![],
            update: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Level {
    Normal,
    Warning,
    Critical,
    Stale,
}

impl Level {
    fn of(level: &str) -> Self {
        match level {
            "normal" => Self::Normal,
            "warning" => Self::Warning,
            "critical" => Self::Critical,
            _ => Self::Stale,
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) enum Entry {
    Separator,
    Item {
        title: String,
        value: String,
        enabled: bool,
    },
}

fn item(title: &str, value: &str, enabled: bool) -> Entry {
    Entry::Item {
        title: title.to_owned(),
        value: value.to_owned(),
        enabled,
    }
}

/// "2h 05m" until a reset within the day, else its local weekday and time
/// (use-tray.ts `resetCountdown`).
pub(super) fn reset_countdown(value: &str, now_ms: i64) -> Option<String> {
    let at = chrono::DateTime::parse_from_rfc3339(value).ok()?;
    let seconds = ((at.timestamp_millis() - now_ms) as f64 / 1000.0)
        .round()
        .max(0.0) as i64;
    if seconds < 86_400 {
        let (hours, minutes) = (seconds / 3600, seconds % 3600 / 60);
        return Some(if hours > 0 {
            format!("{hours}h {minutes:02}m")
        } else {
            format!("{minutes}m")
        });
    }
    Some(
        at.with_timezone(&chrono::Local)
            .format("%a %H:%M")
            .to_string(),
    )
}

/// The menu bar item for a snapshot: `vendors` are the primary screen's HUD
/// agents, `light` whether that HUD is drawn light.
pub(crate) fn project(
    snapshot: &Value,
    vendors: &[Vendor],
    visible: bool,
    glow_hidden: bool,
    light: bool,
    update: Option<&str>,
    now_ms: i64,
) -> Presentation {
    let language = snapshot["language"]
        .as_str()
        .and_then(Language::from_tag)
        .unwrap_or_else(l10n::current);
    let remaining = snapshot["settings"]["quotaDisplay"] == "remaining";
    let mut entries = vec![];
    let quota_vendors: Vec<&Vendor> = vendors
        .iter()
        .filter(|vendor| !vendor.quotas.is_empty() || vendor.balance.is_none())
        .collect();
    for (index, vendor) in quota_vendors.iter().enumerate() {
        if index > 0 {
            entries.push(Entry::Separator);
        }
        entries.push(item(&vendor.name, "", false));
        let mut accounts: Vec<&str> = vec![];
        for quota in &vendor.quotas {
            if let Some(account) = quota["account"]
                .as_str()
                .filter(|account| !account.is_empty())
            {
                if !accounts.contains(&account) {
                    accounts.push(account);
                }
            }
        }
        for account in accounts {
            entries.push(item(account, "", false));
        }
        for quota in &vendor.quotas {
            let value = match quota["usedPercent"].as_f64() {
                Some(used) => {
                    let percent = format!(
                        "{}%",
                        crate::vendors::quota_percent(used, remaining).round()
                    );
                    match quota["resetAt"]
                        .as_str()
                        .and_then(|at| reset_countdown(at, now_ms))
                    {
                        Some(countdown) => format!("{percent} · {countdown}"),
                        None => percent,
                    }
                }
                None => "—".into(),
            };
            entries.push(item(quota["name"].as_str().unwrap_or(""), &value, true));
        }
        if vendor.quotas.is_empty() && vendor.balance.is_none() {
            let title = tr_in(language, "No quota reading");
            entries.push(item(&title, "—", false));
        }
    }
    for (index, vendor) in vendors
        .iter()
        .filter(|vendor| vendor.balance.is_some())
        .enumerate()
    {
        if !quota_vendors.is_empty() || index > 0 {
            entries.push(Entry::Separator);
        }
        let title = l10n::fill(
            &tr_in(language, "{{vendor}} · Balance"),
            &[("vendor", &vendor.name)],
        );
        entries.push(item(&title, vendor.balance.as_deref().unwrap_or(""), true));
    }
    let used_percent = vendors
        .iter()
        .filter(|vendor| vendor.level != "stale")
        .filter_map(|vendor| vendor.used)
        .reduce(f64::max);
    let paused = snapshot["status"]["paused"] == true;
    Presentation {
        language,
        visible,
        glow_hidden,
        light,
        used_percent,
        remaining,
        levels: if paused {
            vec![]
        } else {
            vendors
                .iter()
                .map(|vendor| Level::of(vendor.level))
                .collect()
        },
        entries,
        update: update.map(str::to_owned),
    }
}
