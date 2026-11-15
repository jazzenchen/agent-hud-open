//! First-document appearance and the native surface underneath it.
use crate::shell;
use preference::Preference;
use serde_json::Value;
use std::sync::{Mutex, PoisonError};
use tauri::utils::config::WindowConfig;
use tauri::{Manager, WebviewWindow, WebviewWindowBuilder};

mod preference;

pub(crate) struct Appearance(Mutex<Preference>);

impl Appearance {
    pub(crate) fn load(demo: bool) -> Self {
        // No Runtime or provider reads: only the same saved preference Core
        // will load. A missing/invalid file has Core's System default.
        let saved = (!demo)
            .then(agent_hud_runtime::local_tools_directory)
            .transpose()
            .ok()
            .flatten()
            .and_then(|directory| std::fs::read(directory.join("settings.json")).ok())
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        Self(Mutex::new(Preference::parse(
            saved
                .as_ref()
                .and_then(|value| value["settings"]["appearance"].as_str()),
        )))
    }
}

fn preference(app: &tauri::AppHandle) -> Preference {
    *app.state::<Appearance>()
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn remember(app: &tauri::AppHandle, settings: &Value) {
    *app.state::<Appearance>()
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner) =
        Preference::parse(settings["appearance"].as_str());
}

pub(crate) fn script(app: &tauri::AppHandle) -> String {
    format!(
        "{}\ninitializeAppearance({:?});",
        include_str!("../../../packages/app/src/lib/startup-theme-bootstrap.mjs"),
        preference(app).name(),
    )
}

fn defer_main(configs: &mut [WindowConfig]) -> Option<WindowConfig> {
    let config = configs
        .iter_mut()
        .find(|config| config.label == "main" && config.create)?;
    let initial = config.clone();
    config.create = false;
    Some(initial)
}

pub(crate) fn defer_initial(context: &mut tauri::Context<tauri::Wry>) -> Option<WindowConfig> {
    defer_main(&mut context.config_mut().app.windows)
}

pub(crate) fn open_initial(
    app: &tauri::AppHandle,
    config: Option<&WindowConfig>,
) -> Result<(), String> {
    if let Some(config) = config {
        let builder = WebviewWindowBuilder::from_config(app, config).map_err(|e| e.to_string())?;
        let window = build(app, builder)?;
        if config.visible {
            window.show().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Construct out of sight only while assigning the native background; show
/// immediately afterwards. Nothing waits for navigation, data or React.
pub(crate) fn build(
    app: &tauri::AppHandle,
    builder: WebviewWindowBuilder<'_, tauri::Wry, tauri::AppHandle>,
) -> Result<WebviewWindow, String> {
    let window = builder
        .initialization_script(script(app))
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    let system_light = window.theme().ok() == Some(tauri::Theme::Light);
    apply(&window, preference(app), system_light)?;
    Ok(window)
}

fn apply(window: &WebviewWindow, preference: Preference, system_light: bool) -> Result<(), String> {
    let (r, g, b, a) = preference.background(system_light);
    let color = tauri::window::Color(r, g, b, a);
    window
        .set_background_color(Some(color))
        .map_err(|e| e.to_string())?;
    // Tauri's setter does not implement the macOS WebView layer.
    shell::page_background(window, color)
}

/// React reports its resolved look too, including Match screen. Never change
/// the app-wide native theme: other windows may be on differently styled screens.
#[tauri::command]
pub(crate) fn window_appearance(window: WebviewWindow, theme: String) -> Result<(), String> {
    if !matches!(window.label(), "main" | "statistics") {
        return Err("Only Settings and Statistics have an opaque background".into());
    }
    if !matches!(theme.as_str(), "light" | "dark") {
        return Err("A resolved light or dark appearance is required".into());
    }
    apply(&window, Preference::parse(Some(&theme)), false)
}

#[cfg(test)]
#[path = "appearance/tests.rs"]
mod tests;
