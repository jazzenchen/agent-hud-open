//! The Windows HUD host: a WebView2 window positioned in physical desktop
//! pixels, with Win32 button and modifier state. Geometry, hover and drag state
//! stay in the shared owner.
pub(super) use super::generic::{
    global_pointer, panel_builder, place, set_drag_cursor, set_ignore_cursor, shown,
};
use super::{base_limitations, HudCapabilities, PointerKeys};
#[path = "windows/displays.rs"]
mod displays;
pub(super) use displays::{measure_screens, screen};

/// Windows reports global physical desktop pixels, with a per-window DPI scale.
pub(super) const GLOBAL_POINTS: bool = false;
pub(super) const COORDINATE_SPACE: &str = "monitor-scaled-physical-top-left";

pub(super) fn native_drag() -> bool {
    true
}

pub(super) fn fixed_canvas() -> bool {
    false
}

pub(super) fn edge_hud_unsupported() -> Option<&'static str> {
    None
}

/// The window that had the keyboard before the HUD took it for typing.
pub(super) struct TextFocusOrigin(isize);

fn hwnd(window: &tauri::WebviewWindow) -> Result<windows::Win32::Foundation::HWND, String> {
    let raw = window.hwnd().map_err(|error| error.to_string())?.0 as isize;
    Ok(windows::Win32::Foundation::HWND(
        raw as *mut core::ffi::c_void,
    ))
}

/// Gives the HUD the keyboard for a text field. Windows lets a background
/// process take the foreground only while attached to the input of the
/// window in front, so the HUD joins that thread for the switch.
pub(super) fn begin_text_focus(
    window: &tauri::WebviewWindow,
) -> Result<Option<TextFocusOrigin>, String> {
    use windows::Win32::{
        System::Threading::{AttachThreadInput, GetCurrentThreadId},
        UI::{
            Input::KeyboardAndMouse::SetFocus,
            WindowsAndMessaging::{
                BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId,
                SetForegroundWindow,
            },
        },
    };
    let target = hwnd(window)?;
    unsafe {
        let previous = GetForegroundWindow();
        let origin = (!previous.is_invalid() && previous != target)
            .then_some(TextFocusOrigin(previous.0 as isize));
        let ours = GetCurrentThreadId();
        let theirs = if previous.is_invalid() {
            0
        } else {
            GetWindowThreadProcessId(previous, None)
        };
        let attached =
            theirs != 0 && theirs != ours && AttachThreadInput(ours, theirs, true).as_bool();
        let _ = BringWindowToTop(target);
        let _ = SetForegroundWindow(target);
        let _ = SetFocus(Some(target));
        if attached {
            let _ = AttachThreadInput(ours, theirs, false);
        }
        if GetForegroundWindow() != target {
            // The last resort Tauri offers.
            super::generic::begin_text_focus(window)?;
        }
        Ok(origin)
    }
}

/// Returns the keyboard to the window that had it, if it is still there.
pub(super) fn end_text_focus(
    window: &tauri::WebviewWindow,
    previous: Option<TextFocusOrigin>,
) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, IsWindow, SetForegroundWindow,
    };
    let Some(TextFocusOrigin(previous)) = previous else {
        return super::generic::end_text_focus(window, None);
    };
    let previous = windows::Win32::Foundation::HWND(previous as *mut core::ffi::c_void);
    unsafe {
        // Only while the HUD still holds the keyboard: a window the person
        // has since clicked into keeps it.
        if GetForegroundWindow() == hwnd(window)? && IsWindow(Some(previous)).as_bool() {
            let _ = SetForegroundWindow(previous);
        }
    }
    Ok(())
}

fn key_down(virtual_key: i32) -> bool {
    // Query only button/modifier state. The low "pressed since last call" bit is
    // deliberately ignored; no keyboard event stream is installed.
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetAsyncKeyState(virtual_key: i32) -> i16;
    }
    unsafe { GetAsyncKeyState(virtual_key) < 0 }
}

pub(super) fn left_mouse_down() -> Result<bool, String> {
    Ok(key_down(mouse_button_keys().0))
}

/// GetAsyncKeyState reports physical buttons; the HUD follows the user's
/// primary button just as the WebView's pointer events do.
fn mouse_button_keys() -> (i32, i32) {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};
    logical_mouse_buttons(unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0)
}

fn logical_mouse_buttons(swapped: bool) -> (i32, i32) {
    if swapped {
        (0x02, 0x01)
    } else {
        (0x01, 0x02)
    }
}

pub(super) fn pointer_keys() -> PointerKeys {
    let (primary, secondary) = mouse_button_keys();
    let buttons = [
        (primary, 1),
        (secondary, 2),
        (0x04, 4),
        (0x05, 8),
        (0x06, 16),
    ]
    .into_iter()
    .fold(0, |bits, (key, mask)| {
        bits | if key_down(key) { mask } else { 0 }
    });
    PointerKeys {
        buttons: Some(buttons),
        alt: Some(key_down(0x12)),
        ctrl: Some(key_down(0x11)),
        meta: Some(key_down(0x5b) || key_down(0x5c)),
        shift: Some(key_down(0x10)),
    }
}

pub(super) fn capabilities() -> HudCapabilities {
    let mut limitations = base_limitations();
    limitations.push("Tauri does not support visible-on-all-workspaces on Windows. Exclusive fullscreen applications and secure desktops are not covered.");
    limitations.push("Windows monitor device paths identify unambiguous displays. Mirroring, remote sessions, or unavailable device paths use session IDs; moving a display to another connection or changing its driver can change its device path.");
    HudCapabilities {
        platform: std::env::consts::OS,
        backend: "win32-webview2",
        lazy_creation: true,
        single_window: false,
        per_screen_windows: true,
        transparent_surface: true,
        native_material: false,
        non_focusing_window: true,
        native_nonactivating_panel: false,
        keyboard_handoff: false,
        cursor_passthrough: true,
        interactive_regions: true,
        native_drag: native_drag(),
        global_pointer: "native_cursor_polling",
        absolute_edge_position: "supported",
        all_workspaces: "unsupported",
        fullscreen_overlay: "not_guaranteed",
        hardware_notch: false,
        persistent_display_identity: true,
        limitations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swapped_mouse_buttons_preserve_primary_and_secondary_semantics() {
        assert_eq!(logical_mouse_buttons(false), (0x01, 0x02));
        assert_eq!(logical_mouse_buttons(true), (0x02, 0x01));
    }
}
