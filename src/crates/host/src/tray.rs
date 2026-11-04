//! The menu bar item: the highest usage, the quota gradient and a menu of
//! every agent's windows and balances (Open's StatusItemController). The app
//! projects it from each snapshot itself; no window has to stay open for it.
use std::sync::{Mutex, PoisonError};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

mod icon;
mod projection;
use icon::status_icon;
pub(crate) use projection::project;
use projection::{Entry, Presentation};

/// What the menu bar item shows now.
#[derive(Default)]
pub struct TrayState(Mutex<Option<Presentation>>);

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
            "tray-settings" => crate::auxiliary::show(app),
            "tray-statistics" => statistics(app),
            "tray-toggle-glow" => crate::coordinator::toggle_glow(app),
            "tray-refresh" => crate::auxiliary::refresh_data(app),
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
        if let Err(error) = crate::auxiliary::show_statistics(&app) {
            eprintln!("Agent HUD Statistics: {error}");
        }
    });
}

/// Draws the menu bar item where it changed. Never on the UI thread: each
/// tray call waits for it.
pub(crate) fn apply(app: &tauri::AppHandle, state: Presentation) -> Result<(), String> {
    use tauri::Manager;
    let owner = app.state::<TrayState>();
    let mut shown = owner.0.lock().unwrap_or_else(PoisonError::into_inner);
    if shown.as_ref() == Some(&state) {
        return Ok(());
    }
    let tray = app
        .tray_by_id("main")
        .ok_or("The menu bar item is unavailable")?;
    let chinese = state.language == "zh";
    let value = state.used_percent.map_or_else(
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
            } => {
                let id = format!("tray-data-{index}");
                // Native menu labels retain the source title/value relationship. The OS
                // draws separators, disabled account headers and shortcuts.
                let label = if value.is_empty() {
                    title.clone()
                } else {
                    format!("{title}    {value}")
                };
                menu.append(&MenuItem::with_id(app, id, label, *enabled, None::<&str>)?)?;
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

#[cfg(test)]
mod tests;
