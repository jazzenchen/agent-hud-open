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

/// The area an edge HUD rests against: the whole display, or on a host whose
/// system bars must stay uncovered, the work area beside them.
pub(super) fn dock_bounds(monitor: &tauri::Monitor) -> Rect {
    if !host::DOCK_IN_WORK_AREA {
        return monitor_bounds(monitor);
    }
    let work = monitor.work_area();
    Rect {
        x: work.position.x,
        y: work.position.y,
        width: work.size.width,
        height: work.size.height,
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

/// The part of a HUD canvas the window covers: all of it, or the `depth`
/// physical pixels next to its edge. The page lays out on the whole canvas
/// either way. A shallow window at rest leaves the rest of the screen to other
/// apps' windows and dialogs: macOS hides a sheet's Install button while any
/// window above it overlaps it, transparent or not (agent-hud-open #84).
pub(super) fn depth_frame(canvas: Rect, edge: Edge, depth: Option<u32>) -> Rect {
    let Some(depth) = depth else {
        return canvas;
    };
    match edge {
        Edge::Top => Rect {
            height: depth.clamp(1, canvas.height),
            ..canvas
        },
        Edge::Bottom => {
            let height = depth.clamp(1, canvas.height);
            Rect {
                y: canvas.y.saturating_add((canvas.height - height) as i32),
                height,
                ..canvas
            }
        }
        Edge::Left => Rect {
            width: depth.clamp(1, canvas.width),
            ..canvas
        },
        Edge::Right => {
            let width = depth.clamp(1, canvas.width);
            Rect {
                x: canvas.x.saturating_add((canvas.width - width) as i32),
                width,
                ..canvas
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shallow_window_keeps_to_its_edge_of_the_canvas() {
        let canvas = Rect {
            x: 100,
            y: 0,
            width: 940,
            height: 1080,
        };
        assert!(depth_frame(canvas, Edge::Top, None) == canvas);
        let top = depth_frame(canvas, Edge::Top, Some(120));
        assert_eq!((top.x, top.y, top.width, top.height), (100, 0, 940, 120));
        let bottom = depth_frame(canvas, Edge::Bottom, Some(120));
        assert_eq!(
            (bottom.x, bottom.y, bottom.width, bottom.height),
            (100, 960, 940, 120)
        );
        let left = depth_frame(canvas, Edge::Left, Some(80));
        assert_eq!(
            (left.x, left.y, left.width, left.height),
            (100, 0, 80, 1080)
        );
        let right = depth_frame(canvas, Edge::Right, Some(80));
        assert_eq!(
            (right.x, right.y, right.width, right.height),
            (960, 0, 80, 1080)
        );
        // Never deeper than the canvas, never empty.
        assert_eq!(depth_frame(canvas, Edge::Bottom, Some(5000)).height, 1080);
        assert_eq!(depth_frame(canvas, Edge::Left, Some(0)).width, 1);
    }
}
