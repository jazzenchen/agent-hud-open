//! The menu bar item: the highest usage, the quota gradient and a menu of
//! every agent's windows and balances (Open's StatusItemController). The app
//! projects it from each snapshot itself; no window has to stay open for it.
use crate::vendors::Vendor;
use serde_json::Value;
use std::sync::Mutex;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

/// What the menu bar item shows now.
#[derive(Default)]
pub struct TrayState(Mutex<Option<Presentation>>);

#[derive(Clone, PartialEq)]
pub(crate) struct Presentation {
    language: &'static str,
    visible: bool,
    glow_hidden: bool,
    light: bool,
    used_percent: Option<f64>,
    remaining: bool,
    levels: Vec<Level>,
    entries: Vec<Entry>,
    /// The product's newer version, for New Version….
    update: Option<String>,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            language: "en",
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
enum Level {
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
enum Entry {
    Separator,
    Item {
        title: String,
        value: String,
        enabled: bool,
        checked: Option<bool>,
    },
}

fn item(title: &str, value: &str, enabled: bool) -> Entry {
    Entry::Item {
        title: title.to_owned(),
        value: value.to_owned(),
        enabled,
        checked: None,
    }
}

pub fn install(app: &tauri::App) -> tauri::Result<()> {
    let initial = Presentation::default();
    let menu = make_menu(app.handle(), &initial)?;
    TrayIconBuilder::with_id("main")
        .icon(status_icon(&initial))
        .icon_as_template(false)
        .title(" —")
        .tooltip("Agent HUD · highest usage")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray-settings" => super::show(app),
            "tray-statistics" => statistics(app),
            "tray-toggle-glow" => crate::coordinator::toggle_glow(app),
            "tray-refresh" => super::refresh_data(app),
            "tray-update" => crate::coordinator::open_update_page(app),
            "tray-quit" => app.exit(0),
            id if id.starts_with("tray-data-") => statistics(app),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn statistics(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = super::show_statistics(&app) {
            eprintln!("Agent HUD Statistics: {error}");
        }
    });
}

/// "2h 05m" until a reset within the day, else its local weekday and time
/// (use-tray.ts `resetCountdown`).
fn reset_countdown(value: &str, now_ms: i64) -> Option<String> {
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
    let zh = snapshot["language"] == "zh";
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
            let title = if zh {
                "暂无额度读数"
            } else {
                "No quota reading"
            };
            entries.push(item(title, "—", false));
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
        let title = format!("{} · {}", vendor.name, if zh { "余额" } else { "Balance" });
        entries.push(item(&title, vendor.balance.as_deref().unwrap_or(""), true));
    }
    let used_percent = vendors
        .iter()
        .filter(|vendor| vendor.level != "stale")
        .filter_map(|vendor| vendor.used)
        .reduce(f64::max);
    let paused = snapshot["status"]["paused"] == true;
    Presentation {
        language: if zh { "zh" } else { "en" },
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

/// Draws the menu bar item where it changed. Never on the UI thread: each
/// tray call waits for it.
pub(crate) fn apply(app: &tauri::AppHandle, state: Presentation) -> Result<(), String> {
    use tauri::Manager;
    let owner = app.state::<TrayState>();
    let mut shown = owner.0.lock().map_err(|_| "Tray owner is unavailable")?;
    if shown.as_ref() == Some(&state) {
        return Ok(());
    }
    let tray = app
        .tray_by_id("main")
        .ok_or("The menu bar item is unavailable")?;
    let chinese = state.language == "zh";
    let value = state
        .used_percent
        .filter(|value| value.is_finite())
        .map_or_else(
            || "—".into(),
            |value| {
                format!(
                    "{}%",
                    crate::vendors::quota_percent(value, state.remaining).round()
                )
            },
        );
    tray.set_icon_with_as_template(Some(status_icon(&state)), false)
        .map_err(|e| e.to_string())?;
    tray.set_title(Some(format!(" {value}")))
        .map_err(|e| e.to_string())?;
    tray.set_tooltip(Some(format!(
        "Agent HUD · {}: {value}",
        match (chinese, state.remaining) {
            (true, true) => "最低剩余额度",
            (true, false) => "最高已用额度",
            (false, true) => "lowest remaining quota",
            (false, false) => "highest usage",
        }
    )))
    .map_err(|e| e.to_string())?;
    tray.set_menu(Some(make_menu(app, &state).map_err(|e| e.to_string())?))
        .map_err(|e| e.to_string())?;
    tray.set_visible(state.visible).map_err(|e| e.to_string())?;
    *shown = Some(state);
    Ok(())
}

fn make_menu(app: &tauri::AppHandle, state: &Presentation) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    let zh = state.language == "zh";
    let text = |chinese, english| if zh { chinese } else { english };
    if state.entries.is_empty() {
        menu.append(&MenuItem::with_id(
            app,
            "tray-empty",
            text("没有启用的模型", "No agents enabled"),
            false,
            None::<&str>,
        )?)?;
    }
    for (index, entry) in state.entries.iter().enumerate() {
        match entry {
            Entry::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
            Entry::Item {
                title,
                value,
                enabled,
                checked,
            } => {
                let id = format!("tray-data-{index}");
                // Native menu labels retain the source title/value relationship. The OS
                // draws separators, disabled account headers, checkmarks and shortcuts.
                let label = if value.is_empty() {
                    title.clone()
                } else {
                    format!("{title}    {value}")
                };
                if let Some(checked) = checked {
                    menu.append(&CheckMenuItem::with_id(
                        app,
                        id,
                        label,
                        *enabled,
                        *checked,
                        None::<&str>,
                    )?)?;
                } else {
                    menu.append(&MenuItem::with_id(app, id, label, *enabled, None::<&str>)?)?;
                }
            }
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let glow = match (zh, state.glow_hidden) {
        (true, true) => "显示灵动岛光晕",
        (true, false) => "隐藏灵动岛光晕",
        (false, true) => "Show notch glow",
        (false, false) => "Hide notch glow",
    };
    menu.append(&MenuItem::with_id(
        app,
        "tray-toggle-glow",
        glow,
        true,
        Some("Alt+CmdOrCtrl+H"),
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "tray-statistics",
        text("统计…", "Statistics…"),
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "tray-refresh",
        text("刷新数据", "Refresh data"),
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "tray-settings",
        text("设置…", "Settings…"),
        true,
        Some("CmdOrCtrl+,"),
    )?)?;
    // Open's AppUpdater.menuItems(): New Version… only while the product offers a
    // newer version. A native menu draws it in the menu's own ink, and has no tooltip
    // for the version, which Settings shows.
    if state.update.is_some() {
        menu.append(&MenuItem::with_id(
            app,
            "tray-update",
            text("发现新版本…", "New Version…"),
            true,
            None::<&str>,
        )?)?;
    }
    menu.append(&MenuItem::with_id(
        app,
        "tray-quit",
        text("退出", "Quit"),
        true,
        Some("CmdOrCtrl+Q"),
    )?)?;
    Ok(menu)
}

fn level_color(level: Level, light: bool) -> [f64; 3] {
    let hex = match (level, light) {
        (Level::Normal, false) => 0x3ddc84,
        (Level::Normal, true) => 0x30d158,
        (Level::Warning, false) => 0xffd23f,
        (Level::Warning, true) => 0xffcc00,
        (Level::Critical, false) => 0xff453a,
        (Level::Critical, true) => 0xff3b30,
        (Level::Stale, _) => 0x9a9aa0,
    };
    [
        f64::from((hex >> 16) & 255),
        f64::from((hex >> 8) & 255),
        f64::from(hex & 255),
    ]
}

fn gradient(state: &Presentation, x: f64) -> [f64; 3] {
    if state.levels.is_empty() {
        return level_color(Level::Stale, state.light);
    }
    // GlowGradient.stops: stop i is at (i + .5) / n, with clamped outer colors.
    let coordinate =
        (x * state.levels.len() as f64 - 0.5).clamp(0.0, (state.levels.len() - 1) as f64);
    let left = coordinate.floor() as usize;
    let right = (left + 1).min(state.levels.len() - 1);
    let t = coordinate - left as f64;
    let a = level_color(state.levels[left], state.light);
    let b = level_color(state.levels[right], state.light);
    std::array::from_fn(|channel| a[channel] + (b[channel] - a[channel]) * t)
}

fn status_icon(state: &Presentation) -> tauri::image::Image<'static> {
    // StatusIconRenderer.swift: 22x11 pt silhouette, radius 4, inset .75, 1.5 pt
    // label-color stroke and the HUD quota gradient. tray-icon fixes macOS images at
    // 18 pt tall, so transparent vertical padding preserves the original 11 pt shape.
    const SCALE: u32 = 4;
    let (width, height) = (22 * SCALE, 18 * SCALE);
    let mut pixels = vec![0; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let px = (f64::from(x) + 0.5) / f64::from(SCALE);
            let py = (f64::from(y) + 0.5) / f64::from(SCALE);
            let qx = (px - 11.0).abs() - (10.25 - 4.0);
            let qy = (py - 9.0).abs() - (4.75 - 4.0);
            let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - 4.0;
            let coverage = ((0.75 - distance) * f64::from(SCALE) + 0.5).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let border = ((distance + 0.75) * f64::from(SCALE) + 0.5).clamp(0.0, 1.0);
            let fill = gradient(state, px / 22.0);
            let ink = if state.light { 25.0 } else { 242.0 };
            let index = ((y * width + x) * 4) as usize;
            for channel in 0..3 {
                pixels[index + channel] =
                    (fill[channel] + (ink - fill[channel]) * border).round() as u8;
            }
            pixels[index + 3] = (coverage * 255.0).round() as u8;
        }
    }
    tauri::image::Image::new_owned(pixels, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn titles(state: &Presentation) -> Vec<String> {
        state
            .entries
            .iter()
            .map(|entry| match entry {
                Entry::Separator => "—".into(),
                Entry::Item { title, value, .. } if value.is_empty() => title.clone(),
                Entry::Item { title, value, .. } => format!("{title} {value}"),
            })
            .collect()
    }

    #[test]
    fn the_menu_lists_windows_then_balances_as_use_tray_did() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-21T12:00:00Z")
            .unwrap()
            .timestamp_millis();
        let snapshot = json!({
            "language": "en",
            "quotas": [
                {"vendor": "claude", "name": "5h", "usedPercent": 40.4, "status": "normal", "account": "me@work", "resetAt": "2026-10-21T14:05:00Z"},
                {"vendor": "claude", "name": "Week", "usedPercent": 72.5, "status": "warning", "account": "me@work", "resetAt": null},
                {"vendor": "codex", "name": "5h", "usedPercent": 95.0, "status": "stale", "account": null, "resetAt": null}
            ],
            "billing": [{"vendor": "deepseek", "amount": "8.85", "currency": "CNY", "level": "normal"}],
            "providers": [], "approvals": [], "sessions": []
        });
        let agents = crate::vendors::vendors(&snapshot, "simple", now);
        let state = project(&snapshot, &agents, true, false, false, None, now);
        assert_eq!(
            titles(&state),
            [
                "Claude",
                "me@work",
                "5h 40% · 2h 05m",
                "Week 73%",
                "—",
                "OpenAI",
                "5h 95%",
                "—",
                "DeepSeek · Balance ¥8.85"
            ]
        );
        // The highest reading that is not stale; one level per agent.
        assert_eq!(state.used_percent, Some(72.5));
        assert!(state.levels == [Level::Warning, Level::Stale, Level::Normal]);
        let paused = json!({"language": "zh", "status": {"paused": true}, "quotas": [], "billing": [], "providers": [], "approvals": [],
            "sessions": [{"id": "a", "source": "kimi", "state": "idle", "updatedAt": "2026-10-21T11:00:00Z"}]});
        let agents = crate::vendors::vendors(&paused, "logos", now);
        let state = project(&paused, &agents, true, true, false, None, now);
        assert_eq!(titles(&state), ["Kimi", "暂无额度读数 —"]);
        assert!(state.levels.is_empty() && state.glow_hidden && state.used_percent.is_none());
    }

    #[test]
    fn remaining_display_changes_menu_figures_but_keeps_the_tightest_window_and_risk() {
        let snapshot = json!({
            "language": "en", "settings": {},
            "quotas": [
                {"vendor":"claude", "name":"5h", "usedPercent":0.0, "status":"normal"},
                {"vendor":"claude", "name":"Week", "usedPercent":92.0, "status":"critical"},
                {"vendor":"codex", "name":"5h", "usedPercent":null, "status":"stale"}
            ],
            "billing":[], "providers":[], "approvals":[], "sessions":[]
        });
        let agents = crate::vendors::vendors(&snapshot, "simple", 0);
        let consumed = project(&snapshot, &agents, true, false, false, None, 0);
        let mut remaining_snapshot = snapshot.clone();
        remaining_snapshot["settings"]["quotaDisplay"] = json!("remaining");
        let remaining = project(&remaining_snapshot, &agents, true, false, false, None, 0);
        assert_eq!(
            titles(&remaining),
            ["Claude", "5h 100%", "Week 8%", "—", "OpenAI", "5h —"]
        );
        assert!(remaining.remaining);
        assert_eq!(remaining.used_percent, Some(92.0));
        assert_eq!(remaining.used_percent, consumed.used_percent);
        assert!(remaining.levels == consumed.levels);
        assert_eq!(
            crate::vendors::quota_percent(remaining.used_percent.unwrap(), remaining.remaining),
            8.0
        );
    }

    #[test]
    fn a_reset_within_a_day_counts_down() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-21T12:00:00Z")
            .unwrap()
            .timestamp_millis();
        assert_eq!(
            reset_countdown("2026-10-21T12:09:40Z", now).as_deref(),
            Some("9m")
        );
        assert_eq!(
            reset_countdown("2026-10-21T11:00:00Z", now).as_deref(),
            Some("0m")
        );
        assert_eq!(
            reset_countdown("2026-10-22T11:59:59Z", now).as_deref(),
            Some("23h 59m")
        );
        assert!(reset_countdown("2026-10-23T12:00:00Z", now).is_some_and(|text| text.len() == 9));
        assert_eq!(reset_countdown("soon", now), None);
    }

    #[test]
    fn an_update_notice_redraws_the_menu() {
        let snapshot =
            json!({"language": "en", "quotas": [], "billing": [], "providers": [], "sessions": []});
        let none = project(&snapshot, &[], true, false, false, None, 0);
        let offered = project(&snapshot, &[], true, false, false, Some("0.6.1"), 0);
        assert_eq!(offered.update.as_deref(), Some("0.6.1"));
        // `apply` skips an unchanged presentation, so the notice must change it.
        assert!(none != offered);
    }
}
