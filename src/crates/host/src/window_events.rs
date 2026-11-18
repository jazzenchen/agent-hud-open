//! What the app does as one of its windows is closed, goes away, changes
//! theme, gains focus or resizes.
use tauri::{Manager, Window, WindowEvent};

use crate::{activity, auxiliary, coordinator, shell, web_sign_in};

pub(crate) fn handle(window: &Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        if web_sign_in::foreign(window.label()) {
            return;
        }
        if auxiliary::closes(window.label()) {
            // Closing Settings or Statistics ends its page and web
            // content process; the next open builds it again.
            activity::forget(window.app_handle(), window.label());
            return;
        }
        api.prevent_close();
        let _ = window.hide();
        activity::notify(window.app_handle(), window.label());
    } else if let WindowEvent::Destroyed = event {
        if auxiliary::closes(window.label()) || web_sign_in::foreign(window.label()) {
            shell::windows_changed(window.app_handle());
        }
    } else if let WindowEvent::ThemeChanged(theme) = event {
        coordinator::appearance_changed(window.app_handle(), *theme == tauri::Theme::Light);
    } else if matches!(event, WindowEvent::Focused(_) | WindowEvent::Resized(_)) {
        activity::notify(window.app_handle(), window.label());
    }
}
