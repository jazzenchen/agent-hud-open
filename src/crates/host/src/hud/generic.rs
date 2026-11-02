//! HUD host pieces built only on Tauri's cross-platform window API. Hosts
//! without a native adapter for a piece re-export it from here; a native
//! adapter replaces the re-export with its own function of the same signature.
use super::{monitor_bounds, LogicalRect, Point, Rect, ScreenMeasurement, Shared};
use tauri::{Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

/// Not a nonactivating panel: an ordinary Tauri window, never focused by
/// configuration.
pub(super) fn panel_builder<'a>(
    app: &'a tauri::AppHandle,
    label: &str,
    url: WebviewUrl,
) -> WebviewWindowBuilder<'a, tauri::Wry, tauri::AppHandle> {
    WebviewWindowBuilder::new(app, label, url)
}

/// Sizes, positions and shows the window at `frame` (physical pixels); show()
/// keeps focusable/focused disabled and never calls set_focus(). Returns the
/// frame the system gave it in physical pixels and the monitor's points.
pub(super) fn place(
    window: &tauri::WebviewWindow,
    frame: Rect,
    scale: f64,
    _keep_drag: bool,
    _mode: super::types::Mode,
) -> Result<(Rect, LogicalRect), String> {
    window
        .set_size(PhysicalSize::new(frame.width, frame.height))
        .map_err(|error| error.to_string())?;
    window
        .set_position(PhysicalPosition::new(frame.x, frame.y))
        .map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let actual = Rect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    let canvas = LogicalRect {
        x: f64::from(actual.x) / scale,
        y: f64::from(actual.y) / scale,
        width: f64::from(actual.width) / scale,
        height: f64::from(actual.height) / scale,
    };
    Ok((actual, canvas))
}

/// Ignoring the cursor passes the whole window through; the regions are only
/// hit-tested by the pointer poll.
pub(super) fn set_ignore_cursor(
    window: &tauri::WebviewWindow,
    ignored: bool,
    _regions: &[super::InteractiveRegion],
) -> Result<(), String> {
    window
        .set_ignore_cursor_events(ignored)
        .map_err(|error| error.to_string())
}

pub(super) fn shown(window: &tauri::WebviewWindow, _configured_visible: bool) -> bool {
    window.is_visible().unwrap_or(false)
}

/// No native screen list: each Tauri monitor is measured on its own.
pub(super) fn measure_screens(_app: &tauri::AppHandle) -> Result<Vec<ScreenMeasurement>, String> {
    Ok(Vec::new())
}

/// Session-only identity by monitor index; no notch and a nominal 24 pt top bar.
pub(super) fn screen(
    _screens: &[ScreenMeasurement],
    monitor: &tauri::Monitor,
    index: usize,
) -> Result<ScreenMeasurement, String> {
    let scale = monitor.scale_factor();
    let bounds = monitor_bounds(monitor);
    Ok(ScreenMeasurement {
        runtime_id: format!("monitor:{index}"),
        persistent_id: None,
        name: monitor.name().cloned(),
        screen: LogicalRect {
            x: f64::from(bounds.x) / scale,
            y: f64::from(bounds.y) / scale,
            width: f64::from(bounds.width) / scale,
            height: f64::from(bounds.height) / scale,
        },
        scale_factor: scale,
        menu_bar_height: 24.0,
        notch: None,
    })
}

/// No previous application is remembered; the window takes focus directly.
// Hosts that remember no previous application never make one.
#[allow(dead_code)]
pub(super) struct TextFocusOrigin;

pub(super) fn begin_text_focus(
    window: &tauri::WebviewWindow,
) -> Result<Option<TextFocusOrigin>, String> {
    window.set_focus().map_err(|error| error.to_string())?;
    Ok(None)
}

pub(super) fn end_text_focus(
    _window: &tauri::WebviewWindow,
    _previous: Option<TextFocusOrigin>,
) -> Result<(), String> {
    Ok(())
}

pub(super) fn set_drag_cursor(app: &tauri::AppHandle, owner: &Shared, active: bool) {
    if let Some(window) = app.get_webview_window(&owner.label) {
        let _ = window.set_cursor_icon(if active {
            tauri::CursorIcon::Grabbing
        } else {
            tauri::CursorIcon::Default
        });
    }
}

pub(super) fn global_pointer(app: &tauri::AppHandle) -> Result<Point, String> {
    let position = app.cursor_position().map_err(|error| error.to_string())?;
    Ok(Point {
        x: position.x,
        y: position.y,
    })
}
