//! A lazy native HUD surface. This module owns windows; the data runtime owns no GUI.
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, PoisonError,
    },
    time::Instant,
};
use tauri::Manager;

mod configure;
mod displays;
mod drag;
mod input;
mod plans;
mod text_focus;
mod types;
mod worker;

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
//   edge_hud_unsupported                 what this host cannot do
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

use configure::{configure, configure_display, emit_geometry};
pub(super) use displays::*;
pub use displays::{hud_geometry, hud_monitors};
pub use drag::hud_drag_start;
pub(super) use drag::*;
pub use input::hud_report_regions;
pub(super) use input::*;
pub(crate) use plans::{Planner, Screen};
pub use text_focus::hud_text_focus;
pub(super) use text_focus::*;
pub use types::*;
use worker::ensure_worker;

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
        let owners = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        for owner in owners.values() {
            owner.stopped.store(true, Ordering::Release);
            owner.changed.notify_all();
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

fn owner_for_window(
    owners: &Mutex<HashMap<String, Arc<Shared>>>,
    window: &tauri::WebviewWindow,
) -> Result<Arc<Shared>, String> {
    if !window.label().starts_with("hud-") {
        return Err("Only a registered per-screen HUD surface can control HUD input".into());
    }
    owners
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .values()
        .find(|owner| owner.label == window.label())
        .cloned()
        .ok_or_else(|| "The HUD surface is not attached to a connected screen".into())
}

/// Ends the owners of screens no longer connected: their windows stop taking
/// input and close, and a drag on one is cancelled. Runs before newly
/// connected screens are configured.
fn reconcile(app: &tauri::AppHandle, connected: &HashSet<String>) {
    let Some(windows) = app.try_state::<HudWindows>() else {
        return;
    };
    let removed: Vec<_> = {
        let mut owners = windows.0.lock().unwrap_or_else(PoisonError::into_inner);
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
        let cancelled = {
            let mut state = owner.state.lock().unwrap_or_else(PoisonError::into_inner);
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
        };
        if cancelled {
            host::set_drag_cursor(app, &owner, false);
            emit_drag_end(app, &owner, true);
        }
        crate::activity::forget(app, &owner.label);
    }
}
