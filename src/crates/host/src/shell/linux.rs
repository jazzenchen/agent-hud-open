//! Linux: the app keeps its panel entry, and the global shortcut is not
//! implemented yet (macOS uses ⌘⌥H to show Settings; the tray menu does here).

pub(crate) fn prepare(_app: &tauri::AppHandle) -> Result<(), String> {
    Ok(())
}

pub(crate) fn install(_app: &tauri::AppHandle) {}

pub(crate) fn window_opened(_window: &tauri::WebviewWindow) {}

pub(crate) fn windows_changed(_app: &tauri::AppHandle) {}

/// No Dock: Settings opens from the tray menu.
pub(crate) fn reopened(_event: &tauri::RunEvent) -> bool {
    false
}

/// The screen under the pointer, both in physical pixels here.
pub(crate) fn pointer_monitor(app: &tauri::AppHandle) -> Option<tauri::Monitor> {
    let pointer = app.cursor_position().ok()?;
    app.monitor_from_point(pointer.x, pointer.y).ok().flatten()
}

pub(crate) fn toolbar_chrome<'a>(
    builder: tauri::WebviewWindowBuilder<'a, tauri::Wry, tauri::AppHandle>,
) -> tauri::WebviewWindowBuilder<'a, tauri::Wry, tauri::AppHandle> {
    builder
}

pub(crate) fn system_languages() -> Vec<String> {
    // LANGUAGE lists several, separated by colons.
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
        .map(|value| value.split(':').map(str::to_owned).collect())
        .unwrap_or_default()
}

/// Not drawn yet: GTK is not running before Tauri starts, so the message
/// goes to standard error.
pub(crate) fn alert(title: &str, body: &str, _ok: &str) {
    eprintln!("{title}: {body}");
}

/// No shortcut for the clipboard window on this system yet.
pub(crate) const CLIP_SHORTCUT: bool = false;

pub(crate) fn clip_shortcut(_app: &tauri::AppHandle, _shortcut: Option<crate::inspect::Shortcut>) {}
