//! Public AppKit HUD operations. The pinned creation-time adapter allocates an
//! NSPanel subclass for HUDs while ordinary Tauri windows remain NSWindow.
use super::{LogicalRect, Rect};
use objc2::MainThreadMarker;
use objc2::{runtime::NSObjectProtocol, ClassType};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSScreen, NSStatusWindowLevel, NSWindow, NSWindowAnimationBehavior,
    NSWindowCollectionBehavior,
};
use objc2_core_foundation::CFUUID;
use objc2_foundation::{NSPoint, NSRect, NSSize};
use objc2_web_kit::WKWebView;
use std::ptr::NonNull;
use std::{sync::mpsc, time::Duration};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
#[path = "macos/displays.rs"]
mod displays;
#[path = "macos/text_focus.rs"]
mod text_focus;
pub(super) use displays::{measure_screens, screen};
pub(super) use text_focus::{begin_text_focus, end_text_focus, TextFocusOrigin};
#[path = "macos/pointer.rs"]
mod pointer;
pub(super) use pointer::{global_pointer, left_mouse_down, pointer_keys, set_drag_cursor};

/// NSEvent reports the pointer in one global point plane across displays.
pub(super) const GLOBAL_POINTS: bool = true;
pub(super) const COORDINATE_SPACE: &str = "global-logical-top-left";
/// The top edge and the Notch HUD sit over the menu bar on purpose.
pub(super) const DOCK_IN_WORK_AREA: bool = false;

pub(super) fn native_drag() -> bool {
    true
}

pub(super) fn fixed_canvas() -> bool {
    false
}

/// Tauri itself reloads a WKWebView whose content process ended.
pub(super) fn watch_page(
    _window: &tauri::WebviewWindow,
    _on_event: std::sync::Arc<dyn Fn(super::recovery::PageEvent) + Send + Sync>,
) -> Result<(), String> {
    Ok(())
}

/// The HUD is a nonactivating NSPanel, chosen before allocation.
pub(super) fn panel_builder<'a>(
    app: &'a tauri::AppHandle,
    label: &str,
    url: WebviewUrl,
) -> WebviewWindowBuilder<'a, tauri::Wry, tauri::AppHandle> {
    tao::platform::macos::with_nonactivating_panel(|| WebviewWindowBuilder::new(app, label, url))
}

/// Orders the panel at `frame` (physical pixels) without making it key, and
/// returns the frame AppKit gave it in logical points.
pub(super) fn place(
    window: &tauri::WebviewWindow,
    frame: Rect,
    canvas: Rect,
    scale: f64,
    mode: super::types::Mode,
) -> Result<LogicalRect, String> {
    clear_under_page(window);
    let logical = |rect: Rect| LogicalRect {
        x: f64::from(rect.x) / scale,
        y: f64::from(rect.y) / scale,
        width: f64::from(rect.width) / scale,
        height: f64::from(rect.height) / scale,
    };
    configure(window, mode, logical(frame), logical(canvas))
}

/// The panel is ordered without Tao's show(), so the configuration says
/// whether it is on screen.
pub(super) fn shown(_window: &tauri::WebviewWindow, configured_visible: bool) -> bool {
    configured_visible
}

pub(super) fn edge_hud_unsupported() -> Option<&'static str> {
    None
}

/// Ignoring the cursor passes the whole panel through; the regions are only
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

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGDisplayCreateUUIDFromDisplayID(display: u32) -> Option<NonNull<CFUUID>>;
}

fn on_main<T: Send + 'static>(
    app: &tauri::AppHandle,
    callback: impl FnOnce(MainThreadMarker) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    if let Some(marker) = MainThreadMarker::new() {
        return callback(marker);
    }
    let (sender, receiver) = mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let result = MainThreadMarker::new()
            .ok_or_else(|| "AppKit geometry requires the main thread".to_owned())
            .and_then(callback);
        let _ = sender.send(result);
    })
    .map_err(|error| error.to_string())?;
    receiver
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "AppKit did not return screen geometry in time".to_owned())?
}

fn configure(
    window: &tauri::WebviewWindow,
    mode: super::types::Mode,
    requested: LogicalRect,
    canvas: LogicalRect,
) -> Result<LogicalRect, String> {
    let retained_window = window.clone();
    on_main(window.app_handle(), move |marker| {
        let pointer = retained_window
            .ns_window()
            .map_err(|error| error.to_string())?;
        if pointer.is_null() {
            return Err("The HUD native window is unavailable".into());
        }
        // The cloned Tauri window keeps this native object alive for this main-thread callback.
        // We never change its Objective-C class, delegate, ownership or nonactivating style mask.
        let native = unsafe { &*pointer.cast::<NSWindow>() };
        native.setLevel(NSStatusWindowLevel + 1);
        // Mission Control hides Simple's dock with the other apps' windows;
        // the notch HUDs stay put on every desktop (agent-hud-native-open #135).
        let placement = if mode == super::types::Mode::Simple {
            NSWindowCollectionBehavior::Transient
        } else {
            NSWindowCollectionBehavior::Stationary
        };
        native.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | placement
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        native.setHidesOnDeactivate(false);
        native.setOpaque(false);
        native.setExcludedFromWindowsMenu(true);
        native.setAnimationBehavior(NSWindowAnimationBehavior::None);
        let primary = NSScreen::screens(marker)
            .firstObject()
            .ok_or("AppKit reported no screens")?;
        let primary_frame = primary.frame();
        let primary_top = primary_frame.origin.y + primary_frame.size.height;
        let mut target = native.frame();
        target.origin.x = requested.x;
        target.origin.y = primary_top - requested.y - requested.height;
        target.size.width = requested.width;
        target.size.height = requested.height;
        // Set the complete frame after level/collection flags, in the same main-thread
        // transaction. Tao's separate async size/position calls cannot provide this readback.
        native.setFrame_display(target, false);
        native.orderFrontRegardless();
        let actual = native.frame();
        if (actual.origin.x - target.origin.x).abs() > 0.5
            || (actual.origin.y - target.origin.y).abs() > 0.5
            || (actual.size.width - target.size.width).abs() > 0.5
            || (actual.size.height - target.size.height).abs() > 0.5
        {
            // A public correction attempt after ordering. An inherited AppKit constraint
            // may still win; the caller reports that fact instead of guessing a menu offset.
            native.setFrame_display(target, false);
        }
        let content = native.contentRectForFrameRect(native.frame());
        let content = LogicalRect {
            x: content.origin.x,
            y: primary_top - content.origin.y - content.size.height,
            width: content.size.width,
            height: content.size.height,
        };
        hold_page(native, canvas, content);
        Ok(content)
    })
}

/// Sizes the web view to the whole canvas and holds it where the canvas is on
/// the screen, so the window only shows part of it. Set in the same main-thread
/// pass as the window's frame, a window that grows or shrinks away from its
/// edge (a right or bottom HUD picked up, dropped, opened or closed) only
/// uncovers or covers more of the page. Resizing the web view instead made
/// WebKit show its old picture against the window's new corner for the frame
/// or two it took to lay the page out again: the dock flashed out of sight,
/// or across the screen.
fn hold_page(native: &NSWindow, canvas: LogicalRect, content: LogicalRect) {
    let Some(parent) = native.contentView() else {
        return;
    };
    let below_top = canvas.y - content.y;
    let frame = NSRect::new(
        NSPoint::new(
            canvas.x - content.x,
            if parent.isFlipped() {
                below_top
            } else {
                content.height - below_top - canvas.height
            },
        ),
        NSSize::new(canvas.width, canvas.height),
    );
    for view in parent.subviews().iter() {
        if view.isKindOfClass(WKWebView::class()) {
            view.setAutoresizingMask(NSAutoresizingMaskOptions::empty());
            view.setFrame(frame);
        }
    }
}

/// WebKit fills the part of a view it has not painted yet with the "under
/// page" color, dark by default. A HUD window that grows before its panel
/// opens would show that as a wide black band for a frame or two, so the HUD's
/// is clear, like the rest of its transparent window.
fn clear_under_page(window: &tauri::WebviewWindow) {
    use objc2_app_kit::NSColor;

    let _ = window.with_webview(|platform| {
        let pointer = platform.inner().cast::<WKWebView>();
        if pointer.is_null() {
            return;
        }
        let view = unsafe { &*pointer };
        unsafe { view.setUnderPageBackgroundColor(Some(&NSColor::clearColor())) };
    });
}
