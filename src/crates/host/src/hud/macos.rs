//! Public AppKit HUD operations. The pinned creation-time adapter allocates an
//! NSPanel subclass for HUDs while ordinary Tauri windows remain NSWindow.
use super::{LogicalRect, Point, PointerKeys, Rect, Shared};
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSCursor, NSScreen, NSStatusWindowLevel, NSWindow, NSWindowAnimationBehavior,
    NSWindowCollectionBehavior,
};
use objc2_core_foundation::CFUUID;
use objc2_foundation::NSPoint;
use std::ptr::NonNull;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
#[path = "macos/displays.rs"]
mod displays;
#[path = "macos/text_focus.rs"]
mod text_focus;
pub(super) use displays::{measure_screens, screen};
pub(super) use text_focus::{begin_text_focus, end_text_focus, TextFocusOrigin};

/// NSEvent reports the pointer in one global point plane across displays.
pub(super) const GLOBAL_POINTS: bool = true;
pub(super) const COORDINATE_SPACE: &str = "global-logical-top-left";

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
    scale: f64,
    mode: super::types::Mode,
) -> Result<LogicalRect, String> {
    configure(
        window,
        mode,
        LogicalRect {
            x: f64::from(frame.x) / scale,
            y: f64::from(frame.y) / scale,
            width: f64::from(frame.width) / scale,
            height: f64::from(frame.height) / scale,
        },
    )
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

/// macOS only takes a cursor from the key window, and this nonactivating panel
/// is never key. Like the original Swift HUD, it becomes key while held without
/// activating the app, and resigns key (without orderOut) when released.
pub(super) fn set_drag_cursor(app: &tauri::AppHandle, owner: &Shared, active: bool) {
    static HELD_KEY: AtomicBool = AtomicBool::new(false);
    if let Some(window) = app.get_webview_window(&owner.label) {
        if active && !HELD_KEY.swap(true, Ordering::AcqRel) {
            let _ = window.set_focusable(true);
            let _ = begin_text_focus(&window);
        } else if !active && HELD_KEY.swap(false, Ordering::AcqRel) {
            let _ = resign_drag_key(&window);
            let _ = window.set_focusable(false);
        }
    }
    if let Err(error) = cursor(app, active) {
        eprintln!("Agent HUD drag cursor: {error}");
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGDisplayCreateUUIDFromDisplayID(display: u32) -> Option<NonNull<CFUUID>>;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
}

pub(super) fn left_mouse_down() -> Result<bool, String> {
    // NSEvent's public class property reports button state outside our non-key window as
    // well. This is a state query, not an event tap or global keyboard listener; AX is unused.
    let buttons: usize = unsafe { objc2::msg_send![objc2::class!(NSEvent), pressedMouseButtons] };
    Ok(buttons & 1 != 0)
}

pub(super) fn pointer_keys() -> PointerKeys {
    // Public class properties also work when this non-key window passes events through.
    // NSEvent and DOM use the same left/right/middle bits for the first five buttons.
    let buttons: usize = unsafe { objc2::msg_send![objc2::class!(NSEvent), pressedMouseButtons] };
    let modifiers: usize = unsafe { objc2::msg_send![objc2::class!(NSEvent), modifierFlags] };
    PointerKeys {
        buttons: Some((buttons & 0x1f) as u32),
        alt: Some(modifiers & (1 << 19) != 0),
        ctrl: Some(modifiers & (1 << 18) != 0),
        meta: Some(modifiers & (1 << 20) != 0),
        shift: Some(modifiers & (1 << 17) != 0),
    }
}

pub(super) fn global_pointer(app: &tauri::AppHandle) -> Result<Point, String> {
    let _ = app;
    // NSEvent.mouseLocation and CoreGraphics display pixel height are thread-safe global
    // state queries, just like pressedMouseButtons and modifierFlags above. Reading them
    // directly avoids waking the main run loop every 32 ms while the pointer is at rest.
    let point: NSPoint = unsafe { objc2::msg_send![objc2::class!(NSEvent), mouseLocation] };
    let primary_height = unsafe { CGDisplayPixelsHigh(CGMainDisplayID()) } as f64;
    Ok(Point {
        x: point.x,
        y: primary_height - point.y,
    })
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

fn cursor(app: &tauri::AppHandle, active: bool) -> Result<(), String> {
    on_main(app, move |_| {
        if active {
            NSCursor::closedHandCursor().set();
        } else {
            NSCursor::arrowCursor().set();
        }
        Ok(())
    })
}

fn resign_drag_key(window: &tauri::WebviewWindow) -> Result<(), String> {
    let retained_window = window.clone();
    on_main(window.app_handle(), move |_| {
        let pointer = retained_window
            .ns_window()
            .map_err(|error| error.to_string())?;
        if !pointer.is_null() {
            let native = unsafe { &*pointer.cast::<NSWindow>() };
            if native.isKeyWindow() {
                native.makeFirstResponder(None);
                native.resignKeyWindow();
            }
        }
        Ok(())
    })
}

fn configure(
    window: &tauri::WebviewWindow,
    mode: super::types::Mode,
    requested: LogicalRect,
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
        Ok(LogicalRect {
            x: content.origin.x,
            y: primary_top - content.origin.y - content.size.height,
            width: content.size.width,
            height: content.size.height,
        })
    })
}
