//! macOS: the Dock icon follows the open windows, ⌘⌥H shows Settings, a Dock
//! click reopens Settings, and Statistics draws under a transparent title bar.
use tauri::Manager;

#[path = "macos_hotkey.rs"]
mod hotkey;

pub(crate) fn prepare(_app: &tauri::AppHandle) -> Result<(), String> {
    Ok(())
}

pub(crate) fn install(app: &tauri::AppHandle) {
    hotkey::install(app);
    windows_changed(app);
}

/// Settings or Statistics was shown: the app joins the Dock and app switcher.
pub(crate) fn window_opened(app: &tauri::AppHandle) {
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
}

/// The app keeps a Dock icon only while Settings, Statistics or a sign-in
/// window is open.
pub(crate) fn windows_changed(app: &tauri::AppHandle) {
    let any_open = app.webview_windows().iter().any(|(label, w)| {
        (matches!(label.as_str(), "main" | "statistics") || crate::web_sign_in::foreign(label))
            && (w.is_visible().unwrap_or(false) || w.is_minimized().unwrap_or(false))
    });
    let _ = app.set_activation_policy(if any_open {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    });
}

/// A click on the Dock icon while no window is open.
pub(crate) fn reopened(event: &tauri::RunEvent) -> bool {
    matches!(event, tauri::RunEvent::Reopen { .. })
}

pub(crate) fn statistics_chrome<'a>(
    builder: tauri::WebviewWindowBuilder<'a, tauri::Wry, tauri::AppHandle>,
) -> tauri::WebviewWindowBuilder<'a, tauri::Wry, tauri::AppHandle> {
    builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
}

pub(crate) fn system_prefers_chinese() -> bool {
    objc2_foundation::NSLocale::preferredLanguages()
        .firstObject()
        .is_some_and(|language| language.to_string().starts_with("zh"))
}

/// An app-modal alert from a process that has not started Tauri yet.
pub(crate) fn alert(title: &str, body: &str, ok: &str) {
    use objc2_app_kit::{NSAlert, NSApplication, NSApplicationActivationPolicy};
    use objc2_foundation::{MainThreadMarker, NSString};
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("{title}: {body}");
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app.activate();
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(title));
    alert.setInformativeText(&NSString::from_str(body));
    alert.addButtonWithTitle(&NSString::from_str(ok));
    alert.runModal();
}
