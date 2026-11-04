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
        // A failed position read must never leave a transparent canvas blocking the user's apps.
        apply_ignore_cursor(&window, &mut state, ignored, true)?;
        // Apply input acceptance before notifying React: its expanded controls must be
        // able to receive clicks even if the ignore->accept switch causes no DOM enter.
        publish_pointer(&app, &shared, &mut state, pointer.clone(), true);
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
    if let Some(geometry) = geometry {
        // configure() records the actual native content rectangle, including every
        // drag preview and constrained placement. Keep the same coordinate planes:
        // uniform global points on macOS; physical pixels / selected DPI elsewhere.
        let units = global_units_per_point(geometry.scale_factor);
        return Ok(Point {
            x: pointer.x / units - geometry.canvas.x,
            y: pointer.y / units - geometry.canvas.y,
        });
    }
    // Configuration/rollback hit tests run before the new canvas readback is stored.
    // Their infrequent checks must use the live native frame instead of old geometry.
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
        host::set_ignore_cursor(
            window,
            ignored,
            state.regions.as_deref().unwrap_or_default(),
        )?;
        state.last_ignore_cursor = Some(ignored);
    }
    Ok(())
}
