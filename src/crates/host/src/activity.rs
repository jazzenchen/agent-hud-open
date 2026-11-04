//! Which windows are on screen. A page listens to the app's events only while
//! its window is active and reads what it missed when it is active again, so a
//! hidden window is sent nothing: Tauri evaluates an event's payload in every
//! page that still has a listener for it, whatever the target.
use serde::Serialize;
use std::{collections::HashMap, sync::Mutex};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub(crate) struct WindowActivity(Mutex<HashMap<String, bool>>);

#[derive(Clone, Copy, Serialize)]
pub(crate) struct Activity {
    active: bool,
}

/// HUD panels are shown and hidden by their configuration, which records
/// their activity; AppKit orders them without Tao's show().
fn configured(label: &str) -> bool {
    label.starts_with("hud-")
}

fn current(window: &tauri::WebviewWindow) -> Activity {
    Activity {
        active: window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false),
    }
}

fn recorded(app: &tauri::AppHandle, label: &str) -> Option<bool> {
    let state = app.try_state::<WindowActivity>()?;
    let windows = state.0.lock().ok()?;
    windows.get(label).copied()
}

#[tauri::command]
pub(crate) fn window_activity(window: tauri::WebviewWindow) -> Activity {
    let app = window.app_handle();
    if configured(window.label()) {
        return Activity {
            active: recorded(app, window.label()).unwrap_or(true),
        };
    }
    record(app, window.label(), current(&window)).1
}

/// Returns whether the window's activity changed, and the activity.
fn record(app: &tauri::AppHandle, label: &str, value: Activity) -> (bool, Activity) {
    let changed = app
        .try_state::<WindowActivity>()
        .and_then(|state| {
            let mut windows = state.0.lock().ok()?;
            Some(windows.insert(label.into(), value.active) != Some(value.active))
        })
        .unwrap_or(true);
    (changed, value)
}

/// A window that is going away receives no more events.
pub(crate) fn forget(app: &tauri::AppHandle, label: &str) {
    if let Some(state) = app.try_state::<WindowActivity>() {
        if let Ok(mut windows) = state.0.lock() {
            windows.remove(label);
        }
    }
}

/// Settings or Statistics was shown, hidden, minimized or focused.
pub(crate) fn notify(app: &tauri::AppHandle, label: &str) {
    if configured(label) {
        return;
    }
    if let Some(window) = app.get_webview_window(label) {
        let (_, value) = record(app, label, current(&window));
        let _ = app.emit_to(label, "window:activity", value);
    }
}

/// A HUD panel was shown or hidden by its configuration.
pub(crate) fn set(app: &tauri::AppHandle, label: &str, active: bool) {
    let (changed, value) = record(app, label, Activity { active });
    if changed {
        let _ = app.emit_to(label, "window:activity", value);
    }
}

/// An app event for every active window. A window with no record yet, such
/// as an offscreen HUD picture, is sent it too.
pub(crate) fn publish<S: Serialize + ?Sized>(app: &tauri::AppHandle, event: &str, value: &S) {
    // The collector holds Backend here. Never synchronously dispatch window
    // getters to the UI thread: Exit can be waiting for that same Backend lock.
    let inactive: Vec<String> = app
        .try_state::<WindowActivity>()
        .and_then(|state| {
            let windows = state.0.lock().ok()?;
            Some(
                windows
                    .iter()
                    .filter(|(_, active)| !**active)
                    .map(|(label, _)| label.clone())
                    .collect(),
            )
        })
        .unwrap_or_default();
    let filter = |target: &tauri::EventTarget| match target {
        tauri::EventTarget::WebviewWindow { label } => !inactive.contains(label),
        _ => false,
    };
    if event == "hud:snapshot" {
        // Tauri injects event payloads as JavaScript source. A full session
        // object literal makes WebKit compile a large syntax tree every pass.
        // Transfer JSON as a string and decode data once in the subscriber.
        let json = serde_json::to_string(value).expect("a snapshot is JSON");
        let _ = app.emit_filter("hud:snapshot-json", json, filter);
    } else {
        let _ = app.emit_filter(event, value, filter);
    }
}
