//! Linux: the app keeps its panel entry, and the global shortcut is not
//! implemented yet (macOS uses ⌘⌥H to show Settings; the tray menu does here).

pub(crate) fn prepare(_app: &tauri::AppHandle) -> Result<(), String> {
    Ok(())
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
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
        .is_some_and(|value| value.starts_with("zh"))
}

/// Not drawn yet: GTK is not running before Tauri starts, so the message
/// goes to standard error.
pub(crate) fn alert(title: &str, body: &str, _ok: &str) {
    eprintln!("{title}: {body}");
}
