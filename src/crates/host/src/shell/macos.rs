//! macOS: the Dock icon follows the open windows, ⌘⌥H shows Settings, the
//! clipboard window's shortcut calls it up, a Dock click reopens Settings,
//! and Statistics draws under a transparent title bar.
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

pub(crate) const CLIP_SHORTCUT: bool = true;

/// Holds the clipboard window's shortcut, or lets it go.
pub(crate) fn clip_shortcut(app: &tauri::AppHandle, shortcut: Option<crate::inspect::Shortcut>) {
    hotkey::set_clip(app, shortcut);
}

/// Settings, Statistics or a sign-in window is about to show: the app joins
/// the Dock and app switcher, and the window comes to the desktop in front of
/// the user. Without this, a window opened on another desktop (Space) takes
/// the user back to that desktop instead.
pub(crate) fn window_opened(window: &tauri::WebviewWindow) {
    let _ = window
        .app_handle()
        .set_activation_policy(tauri::ActivationPolicy::Regular);
    // Queued on the same main-thread channel as the show and focus that
    // follow, so the window has this behaviour before it is ordered front.
    let handle = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(pointer) = handle.ns_window() else {
            return;
        };
        if pointer.is_null() {
            return;
        }
        // The cloned Tauri window keeps the native window alive for this callback.
        let native = unsafe { &*pointer.cast::<objc2_app_kit::NSWindow>() };
        native.setCollectionBehavior(
            native.collectionBehavior()
                | objc2_app_kit::NSWindowCollectionBehavior::MoveToActiveSpace,
        );
    });
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

/// The screen under the pointer. Tauri gives the pointer in pixels at the
/// main screen's scale but finds a screen by points, so the pointer goes
/// back to points first; else a window lands on the wrong screen whenever
/// the main screen is Retina.
pub(crate) fn pointer_monitor(app: &tauri::AppHandle) -> Option<tauri::Monitor> {
    let pointer = app.cursor_position().ok()?;
    let scale = app.primary_monitor().ok().flatten()?.scale_factor();
    app.monitor_from_point(pointer.x / scale, pointer.y / scale)
        .ok()
        .flatten()
}

pub(crate) fn toolbar_chrome<'a>(
    builder: tauri::WebviewWindowBuilder<'a, tauri::Wry, tauri::AppHandle>,
) -> tauri::WebviewWindowBuilder<'a, tauri::Wry, tauri::AppHandle> {
    builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
}

pub(crate) fn system_languages() -> Vec<String> {
    objc2_foundation::NSLocale::preferredLanguages()
        .iter()
        .map(|language| language.to_string())
        .collect()
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
