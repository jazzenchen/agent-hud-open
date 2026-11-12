//! Windows: the app keeps its taskbar entry, and the global shortcut is not
//! implemented yet (macOS uses ⌘⌥H to show Settings; the tray menu does here).

#[path = "windows/session.rs"]
mod session;

pub(crate) fn prepare(app: &tauri::AppHandle) -> Result<(), String> {
    session::install(app)
}

pub(crate) fn install(_app: &tauri::AppHandle) {}

pub(crate) fn window_opened(_app: &tauri::AppHandle) {}

pub(crate) fn windows_changed(_app: &tauri::AppHandle) {}

/// No Dock: Settings opens from the tray menu.
pub(crate) fn reopened(_event: &tauri::RunEvent) -> bool {
    false
}

pub(crate) fn statistics_chrome<'a>(
    builder: tauri::WebviewWindowBuilder<'a, tauri::Wry, tauri::AppHandle>,
) -> tauri::WebviewWindowBuilder<'a, tauri::Wry, tauri::AppHandle> {
    builder
}

pub(crate) fn system_prefers_chinese() -> bool {
    // The primary language is the low ten bits; 0x04 is Chinese.
    unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() & 0x3ff == 0x04 }
}

/// A message box from a process that has not started Tauri yet.
pub(crate) fn alert(title: &str, body: &str, _ok: &str) {
    use windows::{
        core::HSTRING,
        Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK},
    };
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(body),
            &HSTRING::from(title),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}
