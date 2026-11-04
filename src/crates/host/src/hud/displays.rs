//! Display discovery, native screen matching, and edge-frame geometry.
use std::sync::{atomic::Ordering, Arc, PoisonError};

use super::{
    host, label_for_screen, owner_for_window, Edge, HudConfig, HudGeometry, HudMonitor, HudWindows,
    Rect, ScreenMeasurement,
};

#[tauri::command]
pub async fn hud_monitors(app: tauri::AppHandle) -> Result<Vec<HudMonitor>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        display_catalog(&app)
            .map(|displays| displays.into_iter().map(|display| display.info).collect())
    })
    .await
    .map_err(|error| format!("Display geometry worker failed: {error}"))?
}

#[tauri::command]
pub async fn hud_geometry(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    screen_id: Option<String>,
    monitor_index: Option<usize>,
    windows: tauri::State<'_, HudWindows>,
) -> Result<Option<HudGeometry>, String> {
    let owners = Arc::clone(&windows.0);
    tauri::async_runtime::spawn_blocking(move || {
        let shared = if window.label().starts_with("hud-") {
            let owner = owner_for_window(&owners, &window)?;
            if screen_id.as_ref().is_some_and(|id| id != &owner.screen_id) {
                return Err("A HUD can only query its own screen geometry".into());
            }
            Some(owner)
        } else {
            let selected = match screen_id {
                Some(id) => Some(id),
                None => match monitor_index {
                    Some(index) => display_catalog(&app)?
                        .into_iter()
                        .find(|display| display.info.index == index)
                        .map(|display| display.info.screen_id),
                    None => None,
                },
            };
            let registry = owners.lock().unwrap_or_else(PoisonError::into_inner);
            selected.and_then(|id| registry.get(&id).cloned())
        };
        let Some(shared) = shared else {
            return Ok(None);
        };
        let state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(if shared.stopped.load(Ordering::Acquire) {
            None
        } else {
            state.geometry.clone()
        })
    })
    .await
    .map_err(|error| format!("HUD geometry worker failed: {error}"))?
}

#[derive(Clone)]
pub(super) struct Display {
    pub(super) monitor: tauri::Monitor,
    pub(super) info: HudMonitor,
    pub(super) measured: ScreenMeasurement,
}

pub(super) fn display_catalog(app: &tauri::AppHandle) -> Result<Vec<Display>, String> {
    let primary = app.primary_monitor().map_err(|error| error.to_string())?;
    let monitors = app
        .available_monitors()
        .map_err(|error| error.to_string())?;
    let screens = host::measure_screens(app)?;
    monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| -> Result<Display, String> {
            let bounds = monitor_bounds(&monitor);
            let work = monitor.work_area();
            let measured = host::screen(&screens, &monitor, index)?;
            let (runtime_id, persistent_id, name, notch) = (
                measured.runtime_id.clone(),
                measured.persistent_id.clone(),
                measured.name.clone(),
                measured.notch,
            );
            let screen_id = persistent_id.clone().unwrap_or_else(|| runtime_id.clone());
            let info = HudMonitor {
                window_label: label_for_screen(&screen_id),
                screen_id,
                index,
                runtime_id,
                persistent_id,
                name,
                primary: primary.as_ref().is_some_and(|primary| {
                    primary.position() == monitor.position() && primary.size() == monitor.size()
                }),
                bounds,
                work_area: Rect {
                    x: work.position.x,
                    y: work.position.y,
                    width: work.size.width,
                    height: work.size.height,
                },
                scale_factor: monitor.scale_factor(),
                notch,
                notch_coordinate_space: "global-logical-top-left",
            };
            Ok(Display {
                monitor,
                info,
                measured,
            })
        })
        .collect()
}

pub(super) fn resolve_display(
    app: &tauri::AppHandle,
    config: &HudConfig,
) -> Result<Display, String> {
    display_catalog(app)?
        .into_iter()
        .find(|display| match config.screen_id.as_deref() {
            Some(id) => display.info.screen_id == id,
            None => display.info.index == config.monitor_index,
        })
        .ok_or_else(|| "The selected HUD screen is no longer connected".into())
}

pub(super) fn monitor_bounds(monitor: &tauri::Monitor) -> Rect {
    Rect {
        x: monitor.position().x,
        y: monitor.position().y,
        width: monitor.size().width,
        height: monitor.size().height,
    }
}

pub(super) fn edge_frame(bounds: Rect, scale: f64, config: &HudConfig) -> Rect {
    let width = (config.width * scale)
        .round()
        .clamp(1.0, f64::from(bounds.width)) as u32;
    let height = (config.height * scale)
        .round()
        .clamp(1.0, f64::from(bounds.height)) as u32;
    let horizontal_travel = f64::from(bounds.width - width);
    let vertical_travel = f64::from(bounds.height - height);
    let (dx, dy) = match config.edge {
        Edge::Top => ((horizontal_travel * config.offset).round(), 0.0),
        Edge::Bottom => ((horizontal_travel * config.offset).round(), vertical_travel),
        Edge::Left => (0.0, (vertical_travel * config.offset).round()),
        Edge::Right => (horizontal_travel, (vertical_travel * config.offset).round()),
    };
    Rect {
        x: bounds.x.saturating_add(dx as i32),
        y: bounds.y.saturating_add(dy as i32),
        width,
        height,
    }
}
