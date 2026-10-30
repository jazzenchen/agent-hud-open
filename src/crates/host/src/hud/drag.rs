//! Native HUD drag pickup, movement preview, rollback, and drop snapping.
use tauri::Emitter;

use super::{
    apply_ignore_cursor, configure, edge_frame, emit_geometry, ensure_worker, global_pointer,
    global_units_per_point, host, left_mouse_down, monitor_bounds, owner_for_window,
    resolve_display, restore_input, Bounds, Display, DragEnd, DragPointer, DragRuns, Edge,
    HudConfig, HudPlacement, HudState, HudWindows, InteractiveRegion, Mode, Point, Shared,
};

pub(super) struct DragState {
    pub(super) config: HudConfig,
    pub(super) display: Display,
    pub(super) edge: Edge,
    pub(super) mode: Mode,
    pub(super) runs: DragRuns,
    pub(super) grab_fraction: f64,
    pub(super) preview: Option<HudPlacement>,
}

pub(super) enum DragTick {
    Unchanged,
    Preview(HudPlacement),
    Dropped(HudPlacement),
}

/// The frontend owns the 6 pt / 300 ms pickup gesture. Native code owns movement and release,
/// including releases over another app where the webview no longer receives DOM events.
#[tauri::command]
pub async fn hud_drag_start(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    edge: Edge,
    mode: Mode,
    strip: InteractiveRegion,
    pointer: DragPointer,
    runs: DragRuns,
    windows: tauri::State<'_, HudWindows>,
) -> Result<(), String> {
    let shared = owner_for_window(&windows.0, &window)?;
    if !host::native_drag() {
        return Err(format!(
            "Native HUD dragging is unsupported on {}; a global mouse-button backend is required",
            std::env::consts::OS
        ));
    }
    if ![
        strip.x,
        strip.y,
        strip.width,
        strip.height,
        pointer.x,
        pointer.y,
        runs.horizontal,
        runs.vertical,
    ]
    .iter()
    .all(|value| value.is_finite())
        || strip.width <= 0.0
        || strip.height <= 0.0
        || runs.horizontal <= 0.0
        || runs.vertical <= 0.0
    {
        return Err(
            "HUD pickup coordinates must be finite, and the strip and target runs must have positive size".into(),
        );
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut state = shared
            .state
            .lock()
            .map_err(|_| "HUD input owner is unavailable")?;
        shared.ensure_connected()?;
        if state.drag.is_some() {
            return Err("A HUD drag is already active".into());
        }
        if !left_mouse_down()? {
            return Err("The left mouse button was released before the HUD pickup".into());
        }
        if !window.is_visible().map_err(|error| error.to_string())? {
            return Err("The HUD surface is hidden".into());
        }
        let config = state
            .config
            .clone()
            .ok_or("The HUD surface has not been configured")?;
        if mode != config.mode || edge != config.edge {
            return Err(
                "The HUD placement changed before pickup; measure the current strip again".into(),
            );
        }
        let viewport = window.inner_size().map_err(|error| error.to_string())?;
        let scale = window.scale_factor().map_err(|error| error.to_string())?;
        if !scale.is_finite() || scale <= 0.0 {
            return Err("The HUD window reported an invalid scale".into());
        }
        let width = f64::from(viewport.width) / scale;
        let height = f64::from(viewport.height) / scale;
        if pointer.x < 0.0 || pointer.y < 0.0 || pointer.x > width || pointer.y > height {
            return Err("The pickup point must be inside the HUD viewport".into());
        }
        let run = if edge.horizontal() {
            strip.width
        } else {
            strip.height
        };
        let grip = if edge.horizontal() {
            pointer.x - strip.x
        } else {
            pointer.y - strip.y
        };
        // Check the real native position before taking input ownership. No AX or keyboard hook is used.
        let position = global_pointer(&app)?;
        let display = resolve_display(&app, &config)?;
        ensure_worker(&app, &shared, &mut state)?;
        apply_ignore_cursor(&window, &mut state, false, true)?;
        state.drag = Some(DragState {
            config,
            display,
            edge,
            mode: if mode == Mode::Notch {
                Mode::Logos
            } else {
                mode
            },
            runs,
            grab_fraction: (grip / run).clamp(0.0, 1.0),
            preview: None,
        });
        let preview = match preview_drag(&app, &mut state, &shared, position) {
            Ok(preview) => preview,
            Err(error) => {
                if let Err(rollback) = restore_drag(&app, &mut state, &shared) {
                    eprintln!("Agent HUD pickup rollback: {rollback}");
                }
                restore_input(&app, &window, &mut state);
                if let Some(geometry) = state.geometry.as_ref() {
                    emit_geometry(&app, geometry);
                }
                emit_drag_end(&app, &shared, true);
                return Err(error);
            }
        };
        set_drag_cursor(&app, &shared, true);
        // The first preview is always sent, including a stationary hold. Serialize this
        // owner's notifications with its state so an old preview cannot follow a cancellation.
        if let Some(geometry) = state.geometry.as_ref() {
            emit_geometry(&app, geometry);
        }
        if let Some(preview) = preview {
            emit_drag_preview(&app, &preview);
        }
        shared.changed.notify_all();
        Ok(())
    })
    .await
    .map_err(|error| format!("HUD pickup worker failed: {error}"))?
}

pub(super) fn emit_drag_end(app: &tauri::AppHandle, owner: &Shared, cancelled: bool) {
    if let Err(error) = app.emit(
        "hud:drag-end",
        DragEnd {
            screen_id: owner.screen_id.clone(),
            cancelled,
        },
    ) {
        eprintln!("Agent HUD drag-end event: {error}");
    }
}

pub(super) fn emit_drag_preview(app: &tauri::AppHandle, preview: &HudPlacement) {
    if let Err(error) = app.emit("hud:drag-preview", preview) {
        eprintln!("Agent HUD drag preview event: {error}");
    }
}

pub(super) fn set_drag_cursor(app: &tauri::AppHandle, owner: &Shared, active: bool) {
    host::set_drag_cursor(app, owner, active);
}

pub(super) fn preview_drag(
    app: &tauri::AppHandle,
    state: &mut HudState,
    owner: &Shared,
    pointer: Point,
) -> Result<Option<HudPlacement>, String> {
    let drag = state.drag.as_ref().unwrap();
    let preview = drop_placement(drag, pointer, &drag.display);
    if drag.preview.as_ref() == Some(&preview) {
        return Ok(None);
    }
    let mut config = drag.config.clone();
    config.mode = preview.mode;
    config.edge = preview.edge;
    config.offset = preview.offset;
    config.monitor_index = preview.monitor_index;
    let display = drag.display.clone();
    // The tight host follows the pointer, as the original native HUD's window did. Only a
    // preview that actually moves or resizes the native frame needs the full configuration;
    // an edge-parallel move inside the column is laid out by the front end alone, and
    // re-running AppKit and Tao round trips on every 16 ms sample made the drag stutter.
    let scale = display.monitor.scale_factor();
    let frame = edge_frame(monitor_bounds(&display.monitor), scale, &config);
    // A host whose canvas covers the drag's whole display never needs one either.
    let unchanged = host::fixed_canvas() && state.geometry.is_some()
        || state.geometry.as_ref().is_some_and(|geometry| {
            (geometry.canvas.x - f64::from(frame.x) / scale).abs() < 0.5
                && (geometry.canvas.y - f64::from(frame.y) / scale).abs() < 0.5
                && (geometry.canvas.width - f64::from(frame.width) / scale).abs() < 0.5
                && (geometry.canvas.height - f64::from(frame.height) / scale).abs() < 0.5
        });
    if unchanged {
        if let Some(current) = state.config.as_mut() {
            current.mode = config.mode;
            current.edge = config.edge;
            current.offset = config.offset;
        }
    } else {
        configure(app, config, state, owner, display, true)?;
        set_drag_cursor(app, owner, true);
    }
    state.drag.as_mut().unwrap().preview = Some(preview.clone());
    Ok(Some(preview))
}

pub(super) fn restore_drag(
    app: &tauri::AppHandle,
    state: &mut HudState,
    owner: &Shared,
) -> Result<(), String> {
    set_drag_cursor(app, owner, false);
    if let Some(drag) = state.drag.take() {
        // Keep the pre-pickup configuration even if restoring the native frame fails.
        // The measured geometry remains the last actual frame until a successful readback.
        state.config = Some(drag.config.clone());
        configure(app, drag.config, state, owner, drag.display, false)?;
    }
    Ok(())
}

pub(super) fn poll_drag(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    state: &mut HudState,
    owner: &Shared,
) -> Result<DragTick, String> {
    let pointer = global_pointer(app)?;
    if left_mouse_down()? {
        apply_ignore_cursor(window, state, false, false)?;
        return preview_drag(app, state, owner, pointer)
            .map(|preview| preview.map_or(DragTick::Unchanged, DragTick::Preview));
    }
    let drag = state.drag.as_ref().unwrap();
    let display = resolve_display(app, &drag.config)?;
    let placement = drop_placement(drag, pointer, &display);
    let mut config = drag.config.clone();
    config.mode = placement.mode;
    config.edge = placement.edge;
    config.offset = placement.offset;
    config.monitor_index = placement.monitor_index;
    configure(app, config, state, owner, display, true)?;
    state.drag = None;
    set_drag_cursor(app, owner, false);
    restore_input(app, window, state);
    Ok(DragTick::Dropped(placement))
}

pub(super) fn drop_placement(drag: &DragState, pointer: Point, display: &Display) -> HudPlacement {
    let monitor = &display.monitor;
    let position = monitor.position();
    let size = monitor.size();
    // Tao expresses macOS positions using each monitor's own backing scale. Convert
    // every display to one global point space before comparing it with NSEvent's cursor.
    let divisor = if host::GLOBAL_POINTS {
        monitor.scale_factor()
    } else {
        1.0
    };
    let bounds = Bounds {
        x: f64::from(position.x) / divisor,
        y: f64::from(position.y) / divisor,
        width: f64::from(size.width) / divisor,
        height: f64::from(size.height) / divisor,
    };
    let edge_distance = |edge: Edge| match edge {
        Edge::Top => (pointer.y - bounds.y).abs(),
        Edge::Right => (bounds.x + bounds.width - pointer.x).abs(),
        Edge::Bottom => (bounds.y + bounds.height - pointer.y).abs(),
        Edge::Left => (pointer.x - bounds.x).abs(),
    };
    // Start with the original edge, and use strict comparison just like NotchGeometry.
    // Exact diagonal ties retain that edge instead of oscillating between two placements.
    let mut edge = drag.edge;
    for candidate in [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left] {
        if edge_distance(candidate) < edge_distance(edge) {
            edge = candidate;
        }
    }
    let length = if edge.horizontal() {
        bounds.width
    } else {
        bounds.height
    };
    let run = (if edge.horizontal() {
        drag.runs.horizontal
    } else {
        drag.runs.vertical
    } * global_units_per_point(monitor.scale_factor()))
    .round()
    .min(length);
    let reading_coordinate = if edge.horizontal() {
        pointer.x - bounds.x
    } else {
        pointer.y - bounds.y
    };
    let travel = length - run;
    let offset = if travel > 0.0 {
        ((reading_coordinate - run * drag.grab_fraction) / travel).clamp(0.0, 1.0)
    } else {
        0.5
    };
    HudPlacement {
        screen_id: display.info.screen_id.clone(),
        mode: drag.mode,
        edge,
        offset,
        monitor_index: display.info.index,
    }
}
