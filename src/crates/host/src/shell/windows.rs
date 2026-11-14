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

pub(crate) fn system_languages() -> Vec<String> {
    use windows::Win32::Globalization::{GetUserDefaultUILanguage, LCIDToLocaleName};
    // The UI language's name, such as "zh-TW" or "ja-JP".
    let mut name = [0u16; 85];
    let length = unsafe { LCIDToLocaleName(GetUserDefaultUILanguage() as u32, Some(&mut name), 0) };
    if length <= 1 {
        return Vec::new();
    }
    vec![String::from_utf16_lossy(&name[..length as usize - 1])]
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
