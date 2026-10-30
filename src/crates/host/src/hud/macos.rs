//! Public AppKit HUD operations. The pinned creation-time adapter allocates an
//! NSPanel subclass for HUDs while ordinary Tauri windows remain NSWindow.
use super::{
    base_limitations, monitor_bounds, HudCapabilities, LogicalRect, Point, PointerKeys, Rect,
    ScreenMeasurement, Shared,
};
use objc2::runtime::NSObjectProtocol;
use objc2::{rc::Retained, MainThreadMarker, Message};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSCursor, NSRunningApplication, NSScreen,
    NSStatusWindowLevel, NSView, NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior,
    NSWorkspace,
};
use objc2_core_foundation::{CFRetained, CFUUID};
use objc2_foundation::{ns_string, NSNumber, NSPoint};
use std::ptr::NonNull;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// NSEvent reports the pointer in one global point plane across displays.
pub(super) const GLOBAL_POINTS: bool = true;
pub(super) const COORDINATE_SPACE: &str = "global-logical-top-left";

pub(super) fn native_drag() -> bool {
    true
}

pub(super) fn fixed_canvas() -> bool {
    false
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
/// returns the frame AppKit gave it in physical pixels and logical points.
pub(super) fn place(
    window: &tauri::WebviewWindow,
    frame: Rect,
    scale: f64,
    keep_drag: bool,
    mode: super::types::Mode,
) -> Result<(Rect, LogicalRect), String> {
    let canvas = configure(
        window,
        mode,
        LogicalRect {
            x: f64::from(frame.x) / scale,
            y: f64::from(frame.y) / scale,
            width: f64::from(frame.width) / scale,
            height: f64::from(frame.height) / scale,
        },
    )?;
    let actual = if keep_drag {
        Rect {
            x: (canvas.x * scale).round() as i32,
            y: (canvas.y * scale).round() as i32,
            width: (canvas.width * scale).round() as u32,
            height: (canvas.height * scale).round() as u32,
        }
    } else {
        let position = window.outer_position().map_err(|error| error.to_string())?;
        let size = window.outer_size().map_err(|error| error.to_string())?;
        Rect {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        }
    };
    Ok((actual, canvas))
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

/// The AppKit screen whose frame matches this Tauri monitor.
pub(super) fn screen(
    screens: &[ScreenMeasurement],
    monitor: &tauri::Monitor,
    _index: usize,
) -> Result<ScreenMeasurement, String> {
    let expected = monitor_bounds(monitor);
    let scale = monitor.scale_factor();
    screens
        .iter()
        .find(|screen| {
            (screen.scale_factor - scale).abs() < 0.001
                && (screen.screen.x * scale - f64::from(expected.x)).abs() <= 1.0
                && (screen.screen.y * scale - f64::from(expected.y)).abs() <= 1.0
                && (screen.screen.width * scale - f64::from(expected.width)).abs() <= 1.0
                && (screen.screen.height * scale - f64::from(expected.height)).abs() <= 1.0
        })
        .cloned()
        .ok_or_else(|| {
            "Could not match the selected Tauri display bounds to an AppKit NSScreen".into()
        })
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

pub(super) fn capabilities() -> HudCapabilities {
    let mut limitations = base_limitations();
    limitations.push("The native adapter sets status-bar level + 1 and fullscreen/Spaces collection flags. Fullscreen and focus behavior still require native GUI qualification.");
    HudCapabilities {
        platform: std::env::consts::OS,
        backend: "appkit",
        lazy_creation: true,
        single_window: false,
        per_screen_windows: true,
        transparent_surface: true,
        native_material: false,
        non_focusing_window: true,
        native_nonactivating_panel: true,
        keyboard_handoff: true,
        cursor_passthrough: true,
        interactive_regions: true,
        native_drag: native_drag(),
        global_pointer: "native_cursor_polling",
        absolute_edge_position: "supported",
        all_workspaces: "configured_unverified",
        fullscreen_overlay: "configured_unverified",
        hardware_notch: true,
        persistent_display_identity: true,
        limitations,
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGDisplayCreateUUIDFromDisplayID(display: u32) -> Option<NonNull<CFUUID>>;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
}

fn screen_identity(screen: &NSScreen) -> Result<(String, Option<String>), String> {
    let description = screen.deviceDescription();
    let value = description
        .objectForKey(ns_string!("NSScreenNumber"))
        .ok_or("NSScreen has no native display number")?;
    let number = value
        .downcast_ref::<NSNumber>()
        .ok_or("NSScreenNumber is not an NSNumber")?;
    let display = number.unsignedIntValue();
    let runtime = format!("display:{display}");
    // Same public identity source and exact UUID key format as ScreenIdentity.swift.
    // A missing UUID stays explicitly session-only instead of masquerading as persistent.
    let persistent = unsafe { CGDisplayCreateUUIDFromDisplayID(display) }
        .map(|uuid| unsafe { CFRetained::from_raw(uuid) })
        .and_then(|uuid| CFUUID::new_string(None, Some(&uuid)))
        .map(|uuid| uuid.to_string());
    Ok((runtime, persistent))
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

/// Retain the actual application identity instead of trusting a possibly reused PID.
/// This is transient local focus ownership, not a saved navigation destination.
pub(super) struct TextFocusOrigin(Retained<NSRunningApplication>);

pub(super) fn begin_text_focus(
    window: &tauri::WebviewWindow,
) -> Result<Option<TextFocusOrigin>, String> {
    let retained_window = window.clone();
    on_main(window.app_handle(), move |marker| {
        let pointer = retained_window
            .ns_window()
            .map_err(|error| error.to_string())?;
        if pointer.is_null() {
            return Err("The HUD native window is unavailable".into());
        }
        let native = unsafe { &*pointer.cast::<NSWindow>() };
        if native.isKindOfClass(objc2::class!(NSPanel)) {
            // An explicit user text gesture may take keys without activating our
            // application. The original frontmost application remains active.
            native.makeKeyWindow();
            if !native.isKeyWindow() {
                return Err("The HUD panel could not obtain keyboard focus".into());
            }
            focus_web_view(native);
            return Ok(None);
        }
        let current = NSRunningApplication::currentApplication();
        let previous = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .filter(|application| !application.isEqual(Some(&current)));
        let was_key = native.isKeyWindow();
        // This is reached only after an explicit text-control pointerdown. Ordinary
        // HUD ordering continues to use orderFrontRegardless without becoming key.
        native.makeKeyAndOrderFront(None);
        if !native.isKeyWindow() {
            return Err("The HUD could not obtain keyboard focus".into());
        }
        #[allow(deprecated)]
        NSApplication::sharedApplication(marker).activateIgnoringOtherApps(true);
        Ok(if was_key {
            None
        } else {
            previous.map(TextFocusOrigin)
        })
    })
}

/// Keys and ⌘C reach the page only while its web view is the first responder.
/// Focusing a text field asks WebKit for that; a selection of plain words does
/// not, so the panel hands it over here unless something inside already has it.
fn focus_web_view(window: &NSWindow) {
    let Some(content) = window.contentView() else {
        return;
    };
    let Some(web) = web_view(&content) else {
        return;
    };
    let held = window
        .firstResponder()
        .and_then(|responder| responder.downcast::<NSView>().ok())
        .is_some_and(|view| view.isDescendantOf(&web));
    if !held {
        window.makeFirstResponder(Some(&web));
    }
}

fn web_view(view: &NSView) -> Option<Retained<NSView>> {
    if view.isKindOfClass(objc2::class!(WKWebView)) {
        return Some(view.retain());
    }
    view.subviews().iter().find_map(|child| web_view(&child))
}

pub(super) fn end_text_focus(
    window: &tauri::WebviewWindow,
    previous: Option<TextFocusOrigin>,
) -> Result<(), String> {
    let retained_window = window.clone();
    on_main(window.app_handle(), move |_| {
        let pointer = retained_window
            .ns_window()
            .map_err(|error| error.to_string())?;
        if pointer.is_null() {
            return Ok(());
        }
        let native = unsafe { &*pointer.cast::<NSWindow>() };
        if native.isKindOfClass(objc2::class!(NSPanel)) {
            if native.isKeyWindow() {
                native.makeFirstResponder(None);
                native.orderOut(None);
                native.orderFrontRegardless();
            }
            return Ok(());
        }
        let current = NSRunningApplication::currentApplication();
        let still_ours = native.isKeyWindow()
            && NSWorkspace::sharedWorkspace()
                .frontmostApplication()
                .is_some_and(|application| application.isEqual(Some(&current)));
        if native.isKeyWindow() {
            // Tauri documents that set_focusable(false) alone cannot resign a key NSWindow.
            native.resignKeyWindow();
        }
        if still_ours {
            if let Some(previous) = previous.filter(|previous| !previous.0.isTerminated()) {
                // If the user already chose another app/window, still_ours is false.
                // AppKit may decline restoration; this never retries or forces activation.
                let _ = previous
                    .0
                    .activateWithOptions(NSApplicationActivationOptions::empty());
            }
        }
        Ok(())
    })
}

pub(super) fn measure_screens(app: &tauri::AppHandle) -> Result<Vec<ScreenMeasurement>, String> {
    on_main(app, |marker| {
        let screens = NSScreen::screens(marker);
        let primary = screens.firstObject().ok_or("AppKit reported no screens")?;
        let first_frame = primary.frame();
        let primary_top = first_frame.origin.y + first_frame.size.height;
        screens
            .iter()
            .map(|screen| {
                let frame = screen.frame();
                let visible = screen.visibleFrame();
                let scale = screen.backingScaleFactor();
                let (runtime_id, persistent_id) = screen_identity(&screen)?;
                let screen_top = frame.origin.y + frame.size.height;
                let logical = LogicalRect {
                    x: frame.origin.x,
                    y: primary_top - screen_top,
                    width: frame.size.width,
                    height: frame.size.height,
                };
                if ![logical.x, logical.y, logical.width, logical.height, scale]
                    .iter()
                    .all(|value| value.is_finite())
                    || logical.width <= 0.0
                    || logical.height <= 0.0
                    || scale <= 0.0
                {
                    return Err("AppKit reported invalid screen geometry".into());
                }
                // These public properties arrived in macOS 12. Older supported hosts return an
                // accurately measured menu-bar stand-in, without claiming hardware-notch support.
                let top = if screen.respondsToSelector(objc2::sel!(safeAreaInsets)) {
                    screen.safeAreaInsets().top
                } else {
                    0.0
                };
                let notch = if top.is_finite() && top > 0.0 {
                    let left_area = screen.auxiliaryTopLeftArea();
                    let right_area = screen.auxiliaryTopRightArea();
                    // Objective-C expresses the Swift optional auxiliary areas as NSZeroRect.
                    let left = if left_area.size.width > 0.0 && left_area.size.height > 0.0 {
                        left_area.origin.x + left_area.size.width
                    } else {
                        frame.origin.x + frame.size.width / 2.0 - 100.0
                    };
                    let right = if right_area.size.width > 0.0 && right_area.size.height > 0.0 {
                        right_area.origin.x
                    } else {
                        frame.origin.x + frame.size.width / 2.0 + 100.0
                    };
                    Some(LogicalRect {
                        x: left,
                        y: logical.y,
                        width: (right - left).max(1.0),
                        height: top,
                    })
                } else {
                    None
                };
                Ok(ScreenMeasurement {
                    runtime_id,
                    persistent_id,
                    name: Some(screen.localizedName().to_string()),
                    screen: logical,
                    scale_factor: scale,
                    menu_bar_height: (screen_top - visible.origin.y - visible.size.height)
                        .max(22.0),
                    notch,
                })
            })
            .collect()
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
