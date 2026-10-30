//! A lazy native HUD surface. This module owns windows; the data runtime owns no GUI.
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    time::Instant,
};
use tauri::{Emitter, Manager, WebviewUrl};

mod displays;
mod drag;
mod input;
mod plans;
mod types;

// The native HUD host. macos.rs, windows.rs and linux.rs each provide the same
// items, and the rest of this folder never branches on the operating system:
//   GLOBAL_POINTS, COORDINATE_SPACE      how the pointer and geometry are expressed
//   native_drag                          whether an edge drag can follow the button
//   fixed_canvas                         whether the canvas stays put while a drag
//                                          previews another edge or offset
//   panel_builder, place, shown          create, position and show a HUD window
//   measure_screens, screen              display identity, notch and menu bar
//   TextFocusOrigin, begin_text_focus,   the explicit keyboard-focus lease
//     end_text_focus
//   set_ignore_cursor                    pass input through all but the reported regions
//   set_drag_cursor, global_pointer,     pointer and button state while hovering
//     left_mouse_down, pointer_keys        or dragging
//   edge_hud_unsupported, capabilities   what this host cannot do
// generic.rs holds the pieces that need only Tauri's own window API; a host
// re-exports those until it has a native adapter.
#[cfg(not(target_os = "macos"))]
mod generic;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod host;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod host;
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod host;

pub use displays::hud_monitors;
pub(super) use displays::*;
pub use drag::hud_drag_start;
pub(super) use drag::*;
pub(super) use input::*;
pub use input::{hud_report_regions, hud_text_focus};
pub(crate) use plans::{Planner, Screen};
pub use types::*;

#[derive(Default)]
pub struct HudWindows(Arc<Mutex<HashMap<String, Arc<Shared>>>>);

struct Shared {
    screen_id: String,
    label: String,
    state: Mutex<HudState>,
    changed: Condvar,
    stopped: AtomicBool,
}

#[derive(Default)]
struct HudState {
    /// None uses explicit ignoreCursor. Some(regions) gives this owner sole input control.
    regions: Option<Vec<InteractiveRegion>>,
    last_ignore_cursor: Option<bool>,
    worker_started: bool,
    config: Option<HudConfig>,
    drag: Option<DragState>,
    geometry: Option<HudGeometry>,
    pointer: Option<HudPointer>,
    pointer_emitted_at: Option<Instant>,
    text_focus: Option<String>,
    text_focus_origin: Option<host::TextFocusOrigin>,
}

impl HudWindows {
    pub fn stop(&self) {
        if let Ok(owners) = self.0.lock() {
            for owner in owners.values() {
                owner.stopped.store(true, Ordering::Release);
                owner.changed.notify_all();
            }
        }
    }
}

impl Shared {
    fn new(screen_id: String) -> Self {
        Self {
            label: label_for_screen(&screen_id),
            screen_id,
            state: Mutex::default(),
            changed: Condvar::new(),
            stopped: AtomicBool::new(false),
        }
    }

    pub(super) fn ensure_connected(&self) -> Result<(), String> {
        if self.stopped.load(Ordering::Acquire) {
            Err("The HUD screen owner was disconnected".into())
        } else {
            Ok(())
        }
    }
}

pub(super) fn label_for_screen(screen_id: &str) -> String {
    // Stable, injective and valid for Tauri labels even when a fallback ID contains ':'.
    format!(
        "hud-{}",
        screen_id
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

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

fn owner_for_window(
    owners: &Mutex<HashMap<String, Arc<Shared>>>,
    window: &tauri::WebviewWindow,
) -> Result<Arc<Shared>, String> {
    if !window.label().starts_with("hud-") {
        return Err("Only a registered per-screen HUD surface can control HUD input".into());
    }
    owners
        .lock()
        .map_err(|_| "HUD screen registry is unavailable")?
        .values()
        .find(|owner| owner.label == window.label())
        .cloned()
        .ok_or_else(|| "The HUD surface is not attached to a connected screen".into())
}

use host::edge_hud_unsupported;

/// Ends the owners of screens no longer connected: their windows stop taking
/// input and close, and a drag on one is cancelled. Runs before newly
/// connected screens are configured.
fn reconcile(app: &tauri::AppHandle, connected: &HashSet<String>) {
    let Some(windows) = app.try_state::<HudWindows>() else {
        return;
    };
    let removed: Vec<_> = {
        let Ok(mut owners) = windows.0.lock() else {
            return;
        };
        let disconnected: Vec<_> = owners
            .keys()
            .filter(|key| !connected.contains(*key))
            .cloned()
            .collect();
        disconnected
            .into_iter()
            .filter_map(|key| {
                let owner = owners.remove(&key)?;
                owner.stopped.store(true, Ordering::Release);
                owner.changed.notify_all();
                Some(owner)
            })
            .collect()
    };
    for owner in removed {
        let cancelled = if let Ok(mut state) = owner.state.lock() {
            let cancelled = state.drag.take().is_some();
            if let Some(window) = app.get_webview_window(&owner.label) {
                let _ = release_text_focus(&window, &mut state);
                let _ = apply_ignore_cursor(&window, &mut state, true, true);
                if let Err(error) = window.hide() {
                    eprintln!("Agent HUD hide on disconnect: {error}");
                }
                if let Err(error) = window.destroy() {
                    eprintln!("Agent HUD destroy on disconnect: {error}");
                }
            }
            cancelled
        } else {
            false
        };
        if cancelled {
            set_drag_cursor(app, &owner, false);
            emit_drag_end(app, &owner, true);
        }
        crate::activity::forget(app, &owner.label);
    }
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
            let registry = owners
                .lock()
                .map_err(|_| "HUD screen registry is unavailable")?;
            selected.and_then(|id| registry.get(&id).cloned())
        };
        let Some(shared) = shared else {
            return Ok(None);
        };
        shared
            .state
            .lock()
            .map(|state| {
                if shared.stopped.load(Ordering::Acquire) {
                    None
                } else {
                    state.geometry.clone()
                }
            })
            .map_err(|_| "HUD geometry owner is unavailable".to_owned())
    })
    .await
    .map_err(|error| format!("HUD geometry worker failed: {error}"))?
}

/// Shows, places or hides a screen's HUD window, creating it on first use.
/// Never on the UI thread: Windows WebView2 creation can deadlock there.
fn configure_display(
    app: &tauri::AppHandle,
    config: HudConfig,
    display: Display,
) -> Result<(), String> {
    let shared = {
        let windows = app.state::<HudWindows>();
        let mut registry = windows
            .0
            .lock()
            .map_err(|_| "HUD screen registry is unavailable")?;
        Arc::clone(
            registry
                .entry(display.info.screen_id.clone())
                .or_insert_with(|| Arc::new(Shared::new(display.info.screen_id.clone()))),
        )
    };
    let mut state = shared
        .state
        .lock()
        .map_err(|_| "HUD window lock is unavailable")?;
    shared.ensure_connected()?;
    let was_dragging = state.drag.is_some();
    if was_dragging {
        if let Err(error) = restore_drag(app, &mut state, &shared) {
            eprintln!("Agent HUD drag rollback: {error}");
        }
    }
    let result = configure(app, config, &mut state, &shared, display, false);
    let geometry = result
        .as_ref()
        .ok()
        .and_then(|result| result.geometry.clone());
    if let Some(geometry) = geometry {
        emit_geometry(app, &geometry);
    }
    if was_dragging {
        emit_drag_end(app, &shared, true);
    }
    drop(state);
    shared.changed.notify_all();
    result.map(|_| ())
}

fn configure(
    app: &tauri::AppHandle,
    mut config: HudConfig,
    state: &mut HudState,
    owner: &Shared,
    display: Display,
    keep_drag: bool,
) -> Result<HudWindowState, String> {
    config.screen_id = Some(owner.screen_id.clone());
    config.monitor_index = display.info.index;
    let capabilities = hud_native_capabilities();
    if !config.visible {
        if !keep_drag {
            state.drag = None;
        }
        let window = app.get_webview_window(&owner.label);
        if let Some(window) = window.as_ref() {
            release_text_focus(window, state)?;
            apply_ignore_cursor(window, state, true, true)?;
            window.hide().map_err(|error| error.to_string())?;
        }
        crate::activity::set(app, &owner.label, false);
        publish_pointer(app, owner, state, HudPointer::outside(owner), false);
        state.config = Some(config.clone());
        return Ok(HudWindowState {
            screen_id: owner.screen_id.clone(),
            window_label: owner.label.clone(),
            created: window.is_some(),
            visible: false,
            config,
            frame: None,
            requested_frame: None,
            capabilities,
            geometry: state.geometry.clone(),
        });
    }
    if let Some(reason) = edge_hud_unsupported() {
        return Err(reason.into());
    }
    if !config.width.is_finite()
        || !config.height.is_finite()
        || config.width <= 0.0
        || config.height <= 0.0
        || !config.offset.is_finite()
    {
        return Err(
            "HUD width and height must be positive finite numbers, and offset must be finite"
                .into(),
        );
    }
    config.offset = config.offset.clamp(0.0, 1.0);
    if matches!(config.mode, Mode::Notch) {
        config.edge = Edge::Top;
        config.offset = 0.5;
    }
    let monitor = &display.monitor;
    let scale = monitor.scale_factor();
    if !scale.is_finite() || scale <= 0.0 {
        return Err("The HUD monitor reported an invalid scale".into());
    }
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
    let (actual, canvas) = host::place(&window, frame, scale, keep_drag, config.mode)?;
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
    Ok(HudWindowState {
        screen_id: owner.screen_id.clone(),
        window_label: owner.label.clone(),
        created: true,
        visible: true,
        config,
        frame: Some(actual),
        requested_frame: Some(frame),
        capabilities,
        geometry: state.geometry.clone(),
    })
}

pub(super) fn emit_geometry(app: &tauri::AppHandle, geometry: &HudGeometry) {
    if let Err(error) = app.emit("hud:geometry", geometry) {
        eprintln!("Agent HUD geometry event: {error}");
    }
}
