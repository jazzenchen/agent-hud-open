//! Native HUD pointer polling, hit-testing, cursor pass-through, and text-focus leases.
use std::{
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

use super::{
    emit_drag_end, emit_drag_preview, emit_geometry, host, owner_for_window, poll_drag,
    restore_drag, set_drag_cursor, DragTick, HudGeometry, HudInputStatus, HudPointer, HudState,
    HudWindows, InteractiveRegion, Point, PointerKeys, Shared,
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
        let mut state = shared
            .state
            .lock()
            .map_err(|_| "HUD input owner is unavailable")?;
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

/// Keyboard focus is an explicit text-editing lease, never a hover side effect.
/// The injected caller window must be a registered, visible per-screen HUD.
#[tauri::command]
pub async fn hud_text_focus(
    window: tauri::WebviewWindow,
    active: bool,
    lease_id: String,
    windows: tauri::State<'_, HudWindows>,
) -> Result<(), String> {
    let shared = owner_for_window(&windows.0, &window)?;
    if lease_id.is_empty() || lease_id.len() > 128 {
        return Err("The HUD text-input lease is invalid".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut state = shared
            .state
            .lock()
            .map_err(|_| "HUD input owner is unavailable")?;
        if !active {
            // A disposed React surface must not release a newer text-input owner.
            return if state.text_focus.as_deref() == Some(lease_id.as_str()) {
                release_text_focus(&window, &mut state)
            } else {
                Ok(())
            };
        }
        shared.ensure_connected()?;
        if !window.is_visible().map_err(|error| error.to_string())? {
            return Err("A hidden HUD cannot obtain keyboard focus".into());
        }
        if state.drag.is_some() {
            return Err("Text input is unavailable while moving the HUD".into());
        }
        if state.text_focus.is_some() {
            state.text_focus = Some(lease_id);
            return Ok(());
        }
        window
            .set_focusable(true)
            .map_err(|error| error.to_string())?;
        match host::begin_text_focus(&window) {
            Ok(previous) => state.text_focus_origin = previous,
            Err(error) => {
                let _ = window.set_focusable(false);
                let _ = host::end_text_focus(&window, None);
                return Err(error);
            }
        }
        state.text_focus = Some(lease_id);
        Ok(())
    })
    .await
    .map_err(|error| format!("HUD keyboard focus worker failed: {error}"))?
}

pub(super) fn release_text_focus(
    window: &tauri::WebviewWindow,
    state: &mut HudState,
) -> Result<(), String> {
    if state.text_focus.take().is_none() {
        return Ok(());
    }
    let focusable = window
        .set_focusable(false)
        .map_err(|error| error.to_string());
    let resigned = host::end_text_focus(window, state.text_focus_origin.take());
    focusable?;
    resigned?;
    Ok(())
}

pub(super) fn ensure_worker(
    app: &tauri::AppHandle,
    shared: &Arc<Shared>,
    state: &mut HudState,
) -> Result<(), String> {
    if !state.worker_started {
        let app = app.clone();
        let worker_state = Arc::clone(shared);
        std::thread::Builder::new()
            .name("agenthud-hud-input".into())
            .spawn(move || poll_pointer(app, worker_state))
            .map_err(|error| format!("Could not start the HUD pointer watcher: {error}"))?;
        state.worker_started = true;
    }
    Ok(())
}

fn poll_pointer(app: tauri::AppHandle, shared: Arc<Shared>) {
    // A screen owner retains one native window until reconciliation stops its worker
    // before destroying that window. Re-cloning the dispatcher every tick wakes Tao's
    // main run loop even when the pointer and every HUD property are unchanged.
    let Some(window) = app.get_webview_window(&shared.label) else {
        return;
    };
    while !shared.stopped.load(Ordering::Acquire) {
        let Ok(mut state) = shared.state.lock() else {
            return;
        };
        while state.drag.is_none()
            && state.regions.as_ref().is_none_or(Vec::is_empty)
            && !shared.stopped.load(Ordering::Acquire)
        {
            // Timed wait also makes shutdown independent of a notification arriving just before wait.
            let Ok((next, _)) = shared.changed.wait_timeout(state, Duration::from_secs(1)) else {
                return;
            };
            state = next;
        }
        if shared.stopped.load(Ordering::Acquire) {
            return;
        }
        let visible = host::shown(
            &window,
            state.config.as_ref().is_some_and(|config| config.visible),
        );
        let mut placement = None;
        let mut preview = None;
        let mut ended = None;
        if visible {
            if state.drag.is_some() {
                match poll_drag(&app, &window, &mut state, &shared) {
                    Ok(DragTick::Unchanged) => {}
                    Ok(DragTick::Preview(value)) => preview = Some(value),
                    Ok(DragTick::Dropped(value)) => {
                        placement = Some(value);
                        ended = Some(false);
                    }
                    Err(error) => {
                        if let Err(rollback) = restore_drag(&app, &mut state, &shared) {
                            eprintln!("Agent HUD drag rollback: {rollback}");
                        }
                        restore_input(&app, &window, &mut state);
                        ended = Some(true);
                        eprintln!("Agent HUD drag: {error}");
                    }
                }
            }
            let pointer = sample_pointer(&app, &window, &state, &shared);
            let ignored = state.drag.is_none() && !pointer.inside;
            let _ = apply_ignore_cursor(&window, &mut state, ignored, false);
            publish_pointer(&app, &shared, &mut state, pointer, false);
        } else if let Some(drag) = state.drag.take() {
            let mut original = drag.config;
            original.visible = false;
            state.config = Some(original);
            set_drag_cursor(&app, &shared, false);
            ended = Some(true);
            let _ = apply_ignore_cursor(&window, &mut state, true, true);
        }
        if state.text_focus.is_some() && (!visible || !window.is_focused().unwrap_or(true)) {
            let _ = release_text_focus(&window, &mut state);
        }
        if !visible {
            publish_pointer(
                &app,
                &shared,
                &mut state,
                HudPointer::outside(&shared),
                false,
            );
        }
        let pause = if state.drag.is_some() {
            Duration::from_millis(16)
        } else if visible {
            Duration::from_millis(32)
        } else {
            Duration::from_millis(500)
        };
        let geometry = if placement.is_some() || preview.is_some() || ended.is_some() {
            state.geometry.clone()
        } else {
            None
        };
        // emit() queues notifications without waiting for JS. Keep this owner's state lock
        // through emission so preview, final commit and cancellation retain their native order.
        if let Some(geometry) = geometry {
            emit_geometry(&app, &geometry);
        }
        if let Some(placement) = placement {
            // The dropped place becomes the screen's saved placement.
            match serde_json::to_value(&placement) {
                Ok(placement) => crate::coordinator::save_placement(&app, placement),
                Err(error) => eprintln!("Agent HUD placement: {error}"),
            }
        }
        if let Some(preview) = preview {
            emit_drag_preview(&app, &preview);
        }
        if let Some(cancelled) = ended {
            emit_drag_end(&app, &shared, cancelled);
        }
        if shared.changed.wait_timeout(state, pause).is_err() {
            return;
        }
    }
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

pub(super) fn global_pointer(app: &tauri::AppHandle) -> Result<Point, String> {
    let pointer = host::global_pointer(app)?;
    if !pointer.x.is_finite() || !pointer.y.is_finite() {
        return Err("The native pointer reported invalid global coordinates".into());
    }
    Ok(pointer)
}

pub(super) fn left_mouse_down() -> Result<bool, String> {
    host::left_mouse_down()
}

fn pointer_keys() -> PointerKeys {
    host::pointer_keys()
}

fn sample_pointer(
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
    let keys = pointer_keys();
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
    let pointer = global_pointer(app)?;
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
    if !scale.is_finite() || scale <= 0.0 {
        return Err("The HUD window reported an invalid scale".into());
    }
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
