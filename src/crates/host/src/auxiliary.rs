//! The auxiliary windows, Settings and Statistics, built again each time they
//! open, with the page or tab they open on; and the menus' Quit and Refresh.
use crate::{activity, service::Service, shell};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Mutex, PoisonError};
use tauri::{Emitter, Manager};

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StatisticsTarget {
    tab: Option<String>,
    session_id: Option<String>,
}

#[derive(Default)]
pub(crate) struct AuxiliaryWindows {
    lock: Mutex<()>,
    target: Mutex<Option<StatisticsTarget>>,
    /// The Settings page asked for (`show_settings_page`) that Settings has
    /// not taken yet.
    page: Mutex<Option<String>>,
}

/// Settings, built again from its configuration when it was closed. Never on
/// the UI thread: Windows WebView2 creation can deadlock there.
fn show_settings(app: &tauri::AppHandle) -> Result<(), String> {
    let windows = app.state::<AuxiliaryWindows>();
    let _guard = windows.lock.lock().unwrap_or_else(PoisonError::into_inner);
    let window = match app.get_webview_window("main") {
        Some(window) => window,
        None => {
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|window| window.label == "main")
                .ok_or("Settings has no window configuration")?;
            // Shown below, once the app has joined the Dock.
            tauri::WebviewWindowBuilder::from_config(app, config)
                .and_then(|builder| builder.visible(false).build())
                .map_err(|error| format!("Could not create the Settings window: {error}"))?
        }
    };
    shell::window_opened(app);
    window.unminimize().map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    activity::notify(app, "main");
    window.set_focus().map_err(|error| error.to_string())
}

pub(crate) fn show(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = show_settings(&app) {
            eprintln!("Agent HUD Settings: {error}");
        }
    });
}

/// Settings, on `page`. A Settings page already open moves there; one that
/// opens takes the page when it starts (`take_settings_page`).
pub(crate) fn show_settings_page(app: &tauri::AppHandle, page: String) {
    *app.state::<AuxiliaryWindows>()
        .page
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(page.clone());
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || match show_settings(&app) {
        Ok(()) => {
            use tauri::Emitter;
            let _ = app.emit_to("main", "settings:page", page);
        }
        Err(error) => eprintln!("Agent HUD Settings: {error}"),
    });
}

#[tauri::command]
pub(crate) fn take_settings_page(windows: tauri::State<'_, AuxiliaryWindows>) -> Option<String> {
    windows
        .page
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
}

/// Settings, on `page` when the HUD names one, as a card's Open Agent
/// plugins does.
#[tauri::command]
pub(crate) async fn hud_show_main(
    app: tauri::AppHandle,
    page: Option<String>,
) -> Result<(), String> {
    if let Some(page) = page {
        show_settings_page(&app, page);
        return Ok(());
    }
    tauri::async_runtime::spawn_blocking(move || show_settings(&app))
        .await
        .map_err(|error| format!("Settings window worker failed: {error}"))?
}

/// Quit from the Settings and Statistics windows' menu, as the menu bar's Quit.
#[tauri::command]
pub(crate) fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// The menu bar's Refresh data: a collection pass that reads accounts again.
pub(crate) fn refresh_data(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<Service>();
        let _turn = service
            .collection
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let _ = service
            .backend
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .request(&app, "refresh", Value::Null);
    });
}

#[tauri::command]
pub(crate) async fn open_statistics(
    app: tauri::AppHandle,
    tab: Option<String>,
    session_id: Option<String>,
) -> Result<(), String> {
    if tab.is_some() || session_id.is_some() {
        let windows = app.state::<AuxiliaryWindows>();
        *windows
            .target
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(StatisticsTarget {
            tab: tab.clone(),
            session_id: session_id.clone(),
        });
    }
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || show_statistics(&handle))
        .await
        .map_err(|error| format!("Statistics window worker failed: {error}"))??;
    if let Some(session_id) = session_id {
        let _ = app.emit("statistics:session", session_id);
    } else if let Some(tab) = tab {
        let _ = app.emit("statistics:tab", tab);
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn take_statistics_target(
    windows: tauri::State<'_, AuxiliaryWindows>,
) -> Option<StatisticsTarget> {
    windows
        .target
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
}

pub(crate) fn show_statistics(app: &tauri::AppHandle) -> Result<(), String> {
    let windows = app.state::<AuxiliaryWindows>();
    let _guard = windows.lock.lock().unwrap_or_else(PoisonError::into_inner);
    let window = match app.get_webview_window("statistics") {
        Some(window) => window,
        None => {
            let builder = tauri::WebviewWindowBuilder::new(
                app,
                "statistics",
                tauri::WebviewUrl::App("index.html?window=statistics".into()),
            )
            .title(format!("{} — Statistics", app.package_info().name))
            // A wide dashboard that re-flows down to a narrow column.
            .inner_size(1280.0, 820.0)
            .min_inner_size(720.0, 560.0)
            .center();
            shell::statistics_chrome(builder)
                .build()
                .map_err(|error| format!("Could not create the Statistics window: {error}"))?
        }
    };
    shell::window_opened(app);
    window.unminimize().map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    activity::notify(app, "statistics");
    window.set_focus().map_err(|error| error.to_string())
}
