//! The Linux HUD host. On a Wayland compositor with wlr-layer-shell, each HUD is
//! an overlay layer surface that covers its whole output and never moves; the
//! front end draws the strip inside it. Wayland has no global cursor, so the
//! surface takes input only inside its reported regions and its own GTK pointer
//! events stand in for the cursor and the drag button. Without layer-shell (GNOME) Wayland refuses an
//! edge HUD; on X11 it is an ordinary GTK/WebKit window whose placement depends
//! on the window manager.
pub(super) use super::generic::{measure_screens, panel_builder, screen, set_drag_cursor, shown};
use super::{InteractiveRegion, LogicalRect, Point, Rect};
use gtk::{cairo, prelude::*};
use gtk_layer_shell::{self as layer, LayerShell};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU8, Ordering},
        mpsc, Mutex, PoisonError,
    },
    time::Duration,
};
#[path = "linux/pointer.rs"]
mod pointer;
use pointer::track_pointer;
pub(super) use pointer::{global_pointer, left_mouse_down, pointer_keys};

pub(super) const GLOBAL_POINTS: bool = false;
pub(super) const COORDINATE_SPACE: &str = "monitor-scaled-physical-top-left";

/// Whether the compositor offers layer-shell; learned on the GTK thread when
/// the first HUD is placed.
static LAYER_SHELL: AtomicU8 = AtomicU8::new(UNKNOWN);
const UNKNOWN: u8 = 0;
const AVAILABLE: u8 = 1;
const MISSING: u8 = 2;

/// Each layer HUD's canvas origin in monitor points and its scale, by window label.
static SURFACES: Mutex<Option<HashMap<String, (Point, f64)>>> = Mutex::new(None);

fn wayland_session() -> bool {
    std::env::var("GDK_BACKEND").ok().as_deref() != Some("x11")
        && (std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").ok().as_deref() == Some("wayland"))
}

fn layer_shell() -> bool {
    wayland_session() && LAYER_SHELL.load(Ordering::Acquire) != MISSING
}

fn surface(label: &str) -> Option<(Point, f64)> {
    SURFACES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()?
        .get(label)
        .copied()
}

pub(super) fn native_drag() -> bool {
    layer_shell()
}

/// A layer HUD's canvas is its whole output, so a drag on one display never
/// changes it.
pub(super) fn fixed_canvas() -> bool {
    wayland_session() && LAYER_SHELL.load(Ordering::Acquire) == AVAILABLE
}

pub(super) fn edge_hud_unsupported() -> Option<&'static str> {
    (wayland_session() && !layer_shell()).then_some(
        "Edge HUD positioning on Wayland needs the wlr-layer-shell protocol, which this compositor does not offer. Use the dashboard or tray.",
    )
}

/// Not followed yet: a WebKitGTK page whose web process ended stays as it is.
pub(super) fn watch_page(
    _window: &tauri::WebviewWindow,
    _on_event: std::sync::Arc<dyn Fn(super::recovery::PageEvent) + Send + Sync>,
) -> Result<(), String> {
    Ok(())
}

/// Runs `job` with the window's GTK toplevel and WebView on the GTK thread and
/// waits for it. Tauri runs it in place when already on that thread.
fn on_gtk<T: Send + 'static>(
    window: &tauri::WebviewWindow,
    job: impl FnOnce(&gtk::Window, &webkit2gtk::WebView) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    window
        .with_webview(move |platform| {
            let webview = platform.inner();
            let result = webview
                .toplevel()
                .and_then(|toplevel| toplevel.downcast::<gtk::Window>().ok())
                .ok_or_else(|| "The HUD WebView has no GTK window".to_owned())
                .and_then(|toplevel| job(&toplevel, &webview));
            let _ = sender.send(result);
        })
        .map_err(|error| error.to_string())?;
    receiver
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "The GTK thread did not answer for the HUD window".to_owned())?
}

/// On layer-shell, makes the window a layer surface before its first show and
/// lets it cover the whole output that contains the requested frame. Its
/// pointer events are relative to the surface, so a surface that moved or
/// resized under the cursor would feed its own motion back into a drag, and
/// niri does not apply a resize while a button is held. The front end draws
/// the strip at the same place inside the larger canvas, and input stays
/// limited to the reported regions.
pub(super) fn place(
    window: &tauri::WebviewWindow,
    frame: Rect,
    scale: f64,
    mode: super::types::Mode,
) -> Result<LogicalRect, String> {
    if !layer_shell() {
        return super::generic::place(window, frame, scale, mode);
    }
    let requested = LogicalRect {
        x: f64::from(frame.x) / scale,
        y: f64::from(frame.y) / scale,
        width: f64::from(frame.width) / scale,
        height: f64::from(frame.height) / scale,
    };
    let label = window.label().to_owned();
    let placed = on_gtk(window, move |toplevel, webview| {
        if !toplevel.is_layer_window() {
            if !layer::is_supported() {
                LAYER_SHELL.store(MISSING, Ordering::Release);
                return Ok(None);
            }
            LAYER_SHELL.store(AVAILABLE, Ordering::Release);
            if toplevel.is_realized() {
                toplevel.unrealize();
            }
            toplevel.init_layer_shell();
            toplevel.set_namespace("agent-hud");
            toplevel.set_layer(layer::Layer::Overlay);
            toplevel.set_keyboard_mode(layer::KeyboardMode::None);
            // Ignore other surfaces' exclusive zones: the canvas is the whole output.
            toplevel.set_exclusive_zone(-1);
            for edge in [
                layer::Edge::Top,
                layer::Edge::Right,
                layer::Edge::Bottom,
                layer::Edge::Left,
            ] {
                toplevel.set_anchor(edge, true);
            }
            track_pointer(webview, label);
        }
        let center = (
            (requested.x + requested.width / 2.0).round() as i32,
            (requested.y + requested.height / 2.0).round() as i32,
        );
        let monitor = toplevel
            .display()
            .monitor_at_point(center.0, center.1)
            .ok_or("No GDK monitor contains the HUD canvas")?;
        let output = monitor.geometry();
        let canvas = LogicalRect {
            x: f64::from(output.x()),
            y: f64::from(output.y()),
            width: f64::from(output.width()),
            height: f64::from(output.height()),
        };
        if toplevel.monitor().as_ref() != Some(&monitor) {
            toplevel.set_monitor(&monitor);
        }
        Ok(Some(canvas))
    })?;
    let Some(canvas) = placed else {
        return Err(edge_hud_unsupported().unwrap_or_default().into());
    };
    SURFACES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .insert(
            window.label().to_owned(),
            (
                Point {
                    x: canvas.x,
                    y: canvas.y,
                },
                scale,
            ),
        );
    window.show().map_err(|error| error.to_string())?;
    Ok(canvas)
}

/// A layer HUD keeps taking input inside its regions while ignoring the
/// cursor; otherwise it would never hear the pointer come back.
pub(super) fn set_ignore_cursor(
    window: &tauri::WebviewWindow,
    ignored: bool,
    regions: &[InteractiveRegion],
) -> Result<(), String> {
    if surface(window.label()).is_none() {
        return super::generic::set_ignore_cursor(window, ignored, regions);
    }
    let rectangles: Option<Vec<_>> = ignored.then(|| {
        regions
            .iter()
            .map(|region| {
                let (x, y) = (region.x.floor(), region.y.floor());
                cairo::RectangleInt::new(
                    x as i32,
                    y as i32,
                    ((region.x + region.width).ceil() - x) as i32,
                    ((region.y + region.height).ceil() - y) as i32,
                )
            })
            .collect()
    });
    on_gtk(window, move |toplevel, _| {
        match rectangles {
            Some(rectangles) => {
                let shape = cairo::Region::create();
                for rectangle in &rectangles {
                    shape
                        .union_rectangle(rectangle)
                        .map_err(|error| error.to_string())?;
                }
                toplevel.input_shape_combine_region(Some(&shape));
            }
            None => toplevel.input_shape_combine_region(None),
        }
        Ok(())
    })
}

/// A layer surface takes the keyboard only while a text field holds the lease.
pub(super) struct TextFocusOrigin;

pub(super) fn begin_text_focus(
    window: &tauri::WebviewWindow,
) -> Result<Option<TextFocusOrigin>, String> {
    if surface(window.label()).is_none() {
        return super::generic::begin_text_focus(window).map(|_| None);
    }
    on_gtk(window, |toplevel, webview| {
        toplevel.set_keyboard_mode(layer::KeyboardMode::OnDemand);
        webview.grab_focus();
        Ok(None)
    })
}

pub(super) fn end_text_focus(
    window: &tauri::WebviewWindow,
    _previous: Option<TextFocusOrigin>,
) -> Result<(), String> {
    if surface(window.label()).is_none() {
        return super::generic::end_text_focus(window, None);
    }
    on_gtk(window, |toplevel, _| {
        toplevel.set_keyboard_mode(layer::KeyboardMode::None);
        Ok(())
    })
}
