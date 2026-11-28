//! Native HUD pointer sampling, hit-testing and cursor pass-through.
use std::{
    sync::PoisonError,
    time::{Duration, Instant},
};
use tauri::Emitter;

use super::{
    ensure_worker, host, owner_for_window, HudGeometry, HudInputStatus, HudPointer, HudState,
    HudWindows, InteractiveRegion, Point, Shared,
};

/// A single native owner follows every interactive rectangle. No GUI is created here.
#[tauri::command]
pub async fn hud_report_regions(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    regions: Vec<InteractiveRegion>,
    windows: tauri::State<'_, HudWindows>,
) -> Result<HudInputStatus, String> {
    let shared = owner_for_window(&windows.0, &window)?;
    if let Some(reason) = host::edge_hud_unsupported() {
        return Err(reason.into());
    }
    if regions.iter().any(|region| {
        !region.x.is_finite()
            || !region.y.is_finite()
            || !region.width.is_finite()
            || !region.height.is_finite()
            || region.width < 0.0
            || region.height < 0.0
    }) {
        return Err("HUD regions must contain finite coordinates and non-negative sizes".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
        shared.ensure_connected()?;
        state.regions = Some(regions);
        let pointer = if window.is_visible().unwrap_or(false) {
            sample_pointer(&app, &window, &state, &shared)
        } else {
            HudPointer::outside(&shared)
        };
        let ignored = state.drag.is_none() && !pointer.inside;
        // Hosts whose cursor passthrough is a whole-window boolean only need a
        // native call when `ignored` changes; layer-shell updates its input shape.
        let force_shape = ignored && host::cursor_shape_uses_regions(&window);
        apply_ignore_cursor(&window, &mut state, ignored, force_shape)?;
        // Apply input acceptance before notifying React: its expanded controls must be
        // able to receive clicks even if the ignore->accept switch causes no DOM enter.
        // Unchanged pointer snapshots are returned directly by this RPC without re-emitting.
        publish_pointer(&app, &shared, &mut state, pointer.clone(), false);
        ensure_worker(&app, &shared, &mut state)?;
        shared.changed.notify_all();
        Ok(HudInputStatus {
            region_count: state.regions.as_ref().map_or(0, Vec::len),
            ignore_cursor: ignored,
            pointer_polling: true,
            pointer,
        })
    })
    .await
    .map_err(|error| format!("HUD input worker failed: {error}"))?
}

pub(super) fn restore_input(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    state: &mut HudState,
) {
    let ignored = match state.regions.as_deref() {
        Some(regions) => {
            pointer_outside(app, window, regions, state.geometry.as_ref()).unwrap_or(true)
        }
        None => state
            .config
            .as_ref()
            .is_none_or(|config| config.ignore_cursor),
    };
    let _ = apply_ignore_cursor(window, state, ignored, true);
}

/// macOS has one global point plane even when displays have different backing scales.
/// Windows reports global physical desktop pixels, with a per-window DPI scale.
pub(super) fn global_units_per_point(scale: f64) -> f64 {
    if host::GLOBAL_POINTS {
        1.0
    } else {
        scale
    }
}

pub(super) fn sample_pointer(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    state: &HudState,
    owner: &Shared,
) -> HudPointer {
    let mut outside = HudPointer::outside(owner);
    // The caller has already checked visibility while holding this owner's state.
    let Ok(point) = pointer_in_viewport(app, window, state.geometry.as_ref()) else {
        return outside;
    };
    outside.point = Some(point);
    let regions = state.regions.as_deref().unwrap_or_default();
    if !region_contains(regions, point) {
        return outside;
    }
    let keys = host::pointer_keys();
    HudPointer {
        inside: true,
        point: Some(point),
        buttons: keys.buttons,
        alt_key: keys.alt,
        ctrl_key: keys.ctrl,
        meta_key: keys.meta,
        shift_key: keys.shift,
        ..outside
    }
}

pub(super) fn publish_pointer(
    app: &tauri::AppHandle,
    owner: &Shared,
    state: &mut HudState,
    pointer: HudPointer,
    force: bool,
) {
    if force || state.pointer.as_ref() != Some(&pointer) {
        let transition = state.pointer.as_ref().is_none_or(|previous| {
            previous.inside != pointer.inside || previous.point.is_some() != pointer.point.is_some()
        });
        // The same native sample supplies local Bot gaze outside the interactive
        // region. Limit only those motion notifications to 12fps; transitions, failed
        // reads and inside coordinates always arrive immediately and remain exact.
        if !force
            && !pointer.inside
            && !transition
            && state
                .pointer_emitted_at
                .is_some_and(|last| last.elapsed() < Duration::from_millis(83))
        {
            return;
        }
        state.pointer = Some(pointer.clone());
        state.pointer_emitted_at = Some(Instant::now());
        if let Err(error) = app.emit_to(&owner.label, "hud:pointer", &pointer) {
            eprintln!("Agent HUD pointer event: {error}");
        }
    }
}

pub(super) fn pointer_outside(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    regions: &[InteractiveRegion],
    geometry: Option<&HudGeometry>,
) -> Result<bool, String> {
    if regions.is_empty() {
        return Ok(true);
    }
    Ok(!region_contains(
        regions,
        pointer_in_viewport(app, window, geometry)?,
    ))
}

fn pointer_in_viewport(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    geometry: Option<&HudGeometry>,
) -> Result<Point, String> {
    let pointer = host::global_pointer(app)?;
    if let Some(geometry) = geometry.filter(|_| !host::live_pointer_origin()) {
        // configure() records the actual native content rectangle, including every
        // drag preview and constrained placement. Keep the same coordinate planes:
        // uniform global points on macOS; physical pixels / selected DPI elsewhere.
        let units = global_units_per_point(geometry.scale_factor);
        return Ok(Point {
            x: pointer.x / units - geometry.canvas.x,
            y: pointer.y / units - geometry.canvas.y,
        });
    }
    // X11 placement is asynchronous: the WM may move the window after configure()
    // cached its origin. Its full-canvas window must use the live content origin.
    // Configuration/rollback also use this path before geometry has been stored.
    let position = window.inner_position().map_err(|error| error.to_string())?;
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let divisor = if host::GLOBAL_POINTS { scale } else { 1.0 };
    let units = global_units_per_point(scale);
    let x = (pointer.x - f64::from(position.x) / divisor) / units;
    let y = (pointer.y - f64::from(position.y) / divisor) / units;
    Ok(Point { x, y })
}

fn region_contains(regions: &[InteractiveRegion], point: Point) -> bool {
    regions.iter().any(|region| {
        point.x >= region.x
            && point.y >= region.y
            && point.x < region.x + region.width
            && point.y < region.y + region.height
    })
}

pub(super) fn apply_ignore_cursor(
    window: &tauri::WebviewWindow,
    state: &mut HudState,
    ignored: bool,
    force: bool,
) -> Result<(), String> {
    if force || state.last_ignore_cursor != Some(ignored) {
        // Regions are in canvas coordinates; a host's input shape is the window's.
        let (dx, dy) = state.window_offset;
        let regions: Vec<InteractiveRegion> = state
            .regions
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|region| InteractiveRegion {
                x: region.x - dx,
                y: region.y - dy,
                ..*region
            })
            .collect();
        host::set_ignore_cursor(window, ignored, &regions)?;
        state.last_ignore_cursor = Some(ignored);
    }
    Ok(())
}

/// Near-field margin (in points) where pointer polling steps up to ~60 Hz so
/// crossing into or out of the resting HUD feels immediate on high-refresh displays.
const NEAR_FIELD_MARGIN: f64 = 64.0;

pub(super) fn pointer_near_regions(
    regions: &[InteractiveRegion],
    point: Point,
    margin: f64,
) -> bool {
    regions.iter().any(|region| {
        point.x >= region.x - margin
            && point.y >= region.y - margin
            && point.x <= region.x + region.width + margin
            && point.y <= region.y + region.height + margin
    })
}

pub(super) fn pointer_poll_interval(
    dragging: bool,
    visible: bool,
    point: Option<Point>,
    regions: &[InteractiveRegion],
) -> Duration {
    if dragging {
        Duration::from_millis(16)
    } else if !visible {
        Duration::from_millis(500)
    } else if point.is_some_and(|pt| pointer_near_regions(regions, pt, NEAR_FIELD_MARGIN)) {
        Duration::from_millis(16)
    } else {
        Duration::from_millis(32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_field_pointer_steps_polling_up_to_60hz_and_relaxes_away() {
        let regions = [InteractiveRegion {
            x: 370.0,
            y: 0.0,
            width: 200.0,
            height: 24.0,
        }];
        assert_eq!(
            pointer_poll_interval(false, false, Some(Point { x: 400.0, y: 10.0 }), &regions),
            Duration::from_millis(500)
        );
        assert_eq!(
            pointer_poll_interval(true, true, None, &regions),
            Duration::from_millis(16)
        );
        // Inside or within 64 pt of the HUD: 16 ms (~60 Hz).
        assert_eq!(
            pointer_poll_interval(false, true, Some(Point { x: 400.0, y: 12.0 }), &regions),
            Duration::from_millis(16)
        );
        assert_eq!(
            pointer_poll_interval(false, true, Some(Point { x: 320.0, y: 70.0 }), &regions),
            Duration::from_millis(16)
        );
        // Far away on the desktop: 32 ms idle poll.
        assert_eq!(
            pointer_poll_interval(false, true, Some(Point { x: 100.0, y: 500.0 }), &regions),
            Duration::from_millis(32)
        );
    }
}
