//! Wayland has no global cursor: a layer HUD's own GTK pointer events, in
//! global physical pixels, stand in for the cursor and the drag button.
use super::super::{generic, Point, PointerKeys};
use super::{layer_shell, surface};
use gtk::{gdk, glib, prelude::*};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, PoisonError,
};

/// The cursor in global physical pixels while it is over a layer HUD.
static POINTER: Mutex<Option<Point>> = Mutex::new(None);
static PRIMARY_DOWN: AtomicBool = AtomicBool::new(false);

/// The WebView's own pointer events, translated to its toplevel surface and
/// then to global physical pixels through the canvas origin.
pub(super) fn track_pointer(webview: &webkit2gtk::WebView, label: String) {
    fn surface_point(window: Option<gdk::Window>, (mut x, mut y): (f64, f64)) -> (f64, f64) {
        let mut current = window;
        while let Some(window) = current {
            if window.window_type() == gdk::WindowType::Toplevel {
                break;
            }
            (x, y) = window.coords_to_parent(x, y);
            current = window.parent();
        }
        (x, y)
    }
    let record = move |point: Option<(f64, f64)>| {
        let global = point.and_then(|(x, y)| {
            let (origin, scale) = surface(&label)?;
            Some(Point {
                x: (origin.x + x) * scale,
                y: (origin.y + y) * scale,
            })
        });
        *POINTER.lock().unwrap_or_else(PoisonError::into_inner) = global;
    };
    let moved = record.clone();
    webview.connect_motion_notify_event(move |_, event| {
        moved(Some(surface_point(event.window(), event.position())));
        glib::Propagation::Proceed
    });
    let entered = record.clone();
    webview.connect_enter_notify_event(move |_, event| {
        entered(Some(surface_point(event.window(), event.position())));
        glib::Propagation::Proceed
    });
    webview.connect_leave_notify_event(move |_, _| {
        // A held button keeps an implicit grab: the drag still reports motion.
        if !PRIMARY_DOWN.load(Ordering::Acquire) {
            record(None);
        }
        glib::Propagation::Proceed
    });
    webview.connect_button_press_event(|_, event| {
        if event.button() == 1 {
            PRIMARY_DOWN.store(true, Ordering::Release);
        }
        glib::Propagation::Proceed
    });
    webview.connect_button_release_event(|_, event| {
        if event.button() == 1 {
            PRIMARY_DOWN.store(false, Ordering::Release);
        }
        glib::Propagation::Proceed
    });
}

pub(in crate::hud) fn global_pointer(app: &tauri::AppHandle) -> Result<Point, String> {
    if !layer_shell() {
        return generic::global_pointer(app);
    }
    POINTER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .ok_or_else(|| "The pointer is not over a HUD surface".to_owned())
}

pub(in crate::hud) fn left_mouse_down() -> Result<bool, String> {
    if layer_shell() {
        Ok(PRIMARY_DOWN.load(Ordering::Acquire))
    } else {
        Err("Native HUD dragging has no global mouse-button backend on X11".into())
    }
}

pub(in crate::hud) fn pointer_keys() -> PointerKeys {
    PointerKeys::default()
}
