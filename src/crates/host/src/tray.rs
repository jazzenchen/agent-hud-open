//! The menu bar item: the highest usage, the quota gradient and a menu of
//! every agent's windows and balances (Open's StatusItemController). The app
//! projects it from each snapshot itself; no window has to stay open for it.
use agent_hud_runtime::l10n::{self, tr_in};
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
    let tooltip = if state.remaining {
        tr_in(
            state.language,
            "Agent HUD · lowest remaining quota: {{value}}",
        )
    } else {
        tr_in(state.language, "Agent HUD · highest usage: {{value}}")
    };
    tray.set_tooltip(Some(l10n::fill(&tooltip, &[("value", &value)])))
        .map_err(|e| e.to_string())?;
    tray.set_menu(Some(make_menu(app, &state).map_err(|e| e.to_string())?))
        .map_err(|e| e.to_string())?;
    tray.set_visible(state.visible).map_err(|e| e.to_string())?;
    *shown = Some(state);
    Ok(())
}

fn make_menu(app: &tauri::AppHandle, state: &Presentation) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    let language = state.language;
    if state.entries.is_empty() {
        menu.append(&MenuItem::with_id(
            app,
            "tray-empty",
            tr_in(language, "No agents enabled"),
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
    let glow = if state.glow_hidden {
        tr_in(language, "Show notch glow")
    } else {
        tr_in(language, "Hide notch glow")
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
        tr_in(language, "Statistics…"),
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "tray-refresh",
        tr_in(language, "Refresh data"),
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "tray-settings",
        tr_in(language, "Settings…"),
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
            tr_in(language, "New Version…"),
            true,
            None::<&str>,
        )?)?;
    }
    menu.append(&MenuItem::with_id(
        app,
        "tray-quit",
        tr_in(language, "Quit"),
        true,
        Some("CmdOrCtrl+Q"),
    )?)?;
    Ok(menu)
}

#[cfg(test)]
mod tests;
