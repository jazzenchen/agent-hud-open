//! Showing, placing and hiding each screen's HUD window: its frame on the
//! display, the geometry it reports, and the input it takes afterwards.
use std::sync::{Arc, PoisonError};
use tauri::{Emitter, Manager, WebviewUrl};

use super::{
    apply_ignore_cursor, edge_frame, emit_drag_end, host, monitor_bounds, pointer_outside,
    publish_pointer, release_text_focus, restore_drag, Display, HudConfig, HudGeometry, HudPointer,
    HudState, HudWindows, LogicalRect, Shared,
};
use host::edge_hud_unsupported;

fn query_screen(screen_id: &str) -> String {
    screen_id
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

/// Shows, places or hides a screen's HUD window, creating it on first use.
/// Never on the UI thread: Windows WebView2 creation can deadlock there.
pub(super) fn configure_display(
    app: &tauri::AppHandle,
    config: HudConfig,
    display: Display,
) -> Result<(), String> {
    let shared = {
        let windows = app.state::<HudWindows>();
        let mut registry = windows.0.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(
            registry
                .entry(display.info.screen_id.clone())
                .or_insert_with(|| Arc::new(Shared::new(display.info.screen_id.clone()))),
        )
    };
    let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
    shared.ensure_connected()?;
    let was_dragging = state.drag.is_some();
    if was_dragging {
        if let Err(error) = restore_drag(app, &mut state, &shared) {
            eprintln!("Agent HUD drag rollback: {error}");
        }
    }
    let result = configure(app, config, &mut state, &shared, display, false);
    if let Some(geometry) = state.geometry.as_ref().filter(|_| result.is_ok()) {
        emit_geometry(app, geometry);
    }
    if was_dragging {
        emit_drag_end(app, &shared, true);
    }
    drop(state);
    shared.changed.notify_all();
    result
}

pub(super) fn configure(
    app: &tauri::AppHandle,
    mut config: HudConfig,
    state: &mut HudState,
    owner: &Shared,
    display: Display,
    keep_drag: bool,
) -> Result<(), String> {
    config.screen_id = Some(owner.screen_id.clone());
    config.monitor_index = display.info.index;
    if !config.visible {
        if !keep_drag {
            state.drag = None;
        }
        if let Some(window) = app.get_webview_window(&owner.label) {
            release_text_focus(&window, state)?;
            apply_ignore_cursor(&window, state, true, true)?;
            window.hide().map_err(|error| error.to_string())?;
        }
        crate::activity::set(app, &owner.label, false);
        publish_pointer(app, owner, state, HudPointer::outside(owner), false);
        state.config = Some(config);
        return Ok(());
    }
    if let Some(reason) = edge_hud_unsupported() {
        return Err(reason.into());
    }
    config.offset = config.offset.clamp(0.0, 1.0);
    let monitor = &display.monitor;
    let scale = monitor.scale_factor();
    let measured = &display.measured;
    let frame = edge_frame(monitor_bounds(monitor), scale, &config);
    // Report the actual clamped logical size, rather than an impossible requested canvas.
    config.width = f64::from(frame.width) / scale;
    config.height = f64::from(frame.height) / scale;
    let window = match app.get_webview_window(&owner.label) {
        Some(window) => window,
        None => {
            let url = WebviewUrl::App(
                format!(
                    "index.html?surface=hud&screen={}",
                    query_screen(&owner.screen_id)
                )
                .into(),
            );
            host::panel_builder(app, &owner.label, url)
                .title("Agent HUD")
                .decorations(false)
                .resizable(false)
                .maximizable(false)
                .minimizable(false)
                .transparent(true)
                .background_color(tauri::window::Color(0, 0, 0, 0))
                .shadow(false)
                .skip_taskbar(true)
                .always_on_top(true)
                .visible_on_all_workspaces(true)
                .focused(false)
                .focusable(false)
                .accept_first_mouse(true)
                .visible(false)
                .build()
                .map_err(|error| format!("Could not create the HUD surface: {error}"))?
        }
    };
    if !keep_drag {
        // A held HUD is the key window for its cursor; only a drag-free configure resets that.
        window
            .set_focusable(state.text_focus.is_some())
            .map_err(|error| error.to_string())?;
    }
    // An explicit configuration change wins over an unfinished pickup. A drop uses the same
    // configure path only after taking the drag, so it can never submit twice.
    if !keep_drag {
        state.drag = None;
    }
    let canvas = host::place(&window, frame, scale, config.mode)?;
    crate::activity::set(app, &owner.label, true);
    state.config = Some(config.clone());
    let expected = LogicalRect {
        x: f64::from(frame.x) / scale,
        y: f64::from(frame.y) / scale,
        width: f64::from(frame.width) / scale,
        height: f64::from(frame.height) / scale,
    };
    state.geometry = Some(HudGeometry {
        screen_id: owner.screen_id.clone(),
        monitor_index: config.monitor_index,
        scale_factor: measured.scale_factor,
        coordinate_space: host::COORDINATE_SPACE,
        screen: measured.screen,
        menu_bar_height: measured.menu_bar_height,
        has_notch: measured.notch.is_some(),
        notch: measured.notch,
        canvas,
        notch_in_viewport: measured.notch.map(|notch| LogicalRect {
            x: notch.x - canvas.x,
            y: notch.y - canvas.y,
            width: notch.width,
            height: notch.height,
        }),
        placement_constrained: (expected.x - canvas.x).abs() > 0.5
            || (expected.y - canvas.y).abs() > 0.5
            || (expected.width - canvas.width).abs() > 0.5
            || (expected.height - canvas.height).abs() > 0.5,
    });
    // Existing rectangles own input after a resize as well. The initial show should request
    // ignoreCursor:true until React has measured its actual visible surfaces.
    let ignore = if state.drag.is_some() {
        false
    } else {
        match state.regions.as_deref() {
            Some(regions) => {
                pointer_outside(app, &window, regions, state.geometry.as_ref()).unwrap_or(true)
            }
            None => config.ignore_cursor,
        }
    };
    apply_ignore_cursor(&window, state, ignore, !keep_drag)?;
    Ok(())
}

pub(super) fn emit_geometry(app: &tauri::AppHandle, geometry: &HudGeometry) {
    if let Err(error) = app.emit("hud:geometry", geometry) {
        eprintln!("Agent HUD geometry event: {error}");
    }
}
