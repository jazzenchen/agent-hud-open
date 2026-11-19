//! The pointer as AppKit reports it: position, buttons and modifiers read as
//! global state, and the closed-hand cursor a drag holds the panel key for.
use super::super::{Point, PointerKeys, Shared};
use super::{begin_text_focus, on_main};
use objc2_app_kit::{NSCursor, NSWindow};
use objc2_foundation::NSPoint;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGMainDisplayID() -> u32;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
}

/// macOS only takes a cursor from the key window, and this nonactivating panel
/// is never key. Like the original Swift HUD, it becomes key while held without
/// activating the app, and resigns key (without orderOut) when released.
pub(in super::super) fn set_drag_cursor(app: &tauri::AppHandle, owner: &Shared, active: bool) {
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

pub(in super::super) fn left_mouse_down() -> Result<bool, String> {
    // NSEvent's public class property reports button state outside our non-key window as
    // well. This is a state query, not an event tap or global keyboard listener; AX is unused.
    let buttons: usize = unsafe { objc2::msg_send![objc2::class!(NSEvent), pressedMouseButtons] };
    Ok(buttons & 1 != 0)
}

pub(in super::super) fn pointer_keys() -> PointerKeys {
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

pub(in super::super) fn global_pointer(app: &tauri::AppHandle) -> Result<Point, String> {
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
