//! The explicit keyboard-focus lease: the HUD panel takes the keys for a text
//! field without activating the app, and an ordinary window gives them back.
use objc2::runtime::NSObjectProtocol;
use objc2::{rc::Retained, Message};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSRunningApplication, NSView, NSWindow,
    NSWorkspace,
};
use tauri::Manager;

/// Retain the actual application identity instead of trusting a possibly reused PID.
/// This is transient local focus ownership, not a saved navigation destination.
pub(in crate::hud) struct TextFocusOrigin(Retained<NSRunningApplication>);

pub(in crate::hud) fn begin_text_focus(
    window: &tauri::WebviewWindow,
) -> Result<Option<TextFocusOrigin>, String> {
    let retained_window = window.clone();
    super::on_main(window.app_handle(), move |marker| {
        let pointer = retained_window
            .ns_window()
            .map_err(|error| error.to_string())?;
        if pointer.is_null() {
            return Err("The HUD native window is unavailable".into());
        }
        let native = unsafe { &*pointer.cast::<NSWindow>() };
        if native.isKindOfClass(objc2::class!(NSPanel)) {
            // An explicit user text gesture may take keys without activating our
            // application. The original frontmost application remains active.
            native.makeKeyWindow();
            if !native.isKeyWindow() {
                return Err("The HUD panel could not obtain keyboard focus".into());
            }
            focus_web_view(native);
            return Ok(None);
        }
        let current = NSRunningApplication::currentApplication();
        let previous = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .filter(|application| !application.isEqual(Some(&current)));
        let was_key = native.isKeyWindow();
        // This is reached only after an explicit text-control pointerdown. Ordinary
        // HUD ordering continues to use orderFrontRegardless without becoming key.
        native.makeKeyAndOrderFront(None);
        if !native.isKeyWindow() {
            return Err("The HUD could not obtain keyboard focus".into());
        }
        #[allow(deprecated)]
        NSApplication::sharedApplication(marker).activateIgnoringOtherApps(true);
        Ok(if was_key {
            None
        } else {
            previous.map(TextFocusOrigin)
        })
    })
}

/// Keys and ⌘C reach the page only while its web view is the first responder.
/// Focusing a text field asks WebKit for that; a selection of plain words does
/// not, so the panel hands it over here unless something inside already has it.
fn focus_web_view(window: &NSWindow) {
    let Some(content) = window.contentView() else {
        return;
    };
    let Some(web) = web_view(&content) else {
        return;
    };
    let held = window
        .firstResponder()
        .and_then(|responder| responder.downcast::<NSView>().ok())
        .is_some_and(|view| view.isDescendantOf(&web));
    if !held {
        window.makeFirstResponder(Some(&web));
    }
}

fn web_view(view: &NSView) -> Option<Retained<NSView>> {
    if view.isKindOfClass(objc2::class!(WKWebView)) {
        return Some(view.retain());
    }
    view.subviews().iter().find_map(|child| web_view(&child))
}

pub(in crate::hud) fn end_text_focus(
    window: &tauri::WebviewWindow,
    previous: Option<TextFocusOrigin>,
) -> Result<(), String> {
    let retained_window = window.clone();
    super::on_main(window.app_handle(), move |_| {
        let pointer = retained_window
            .ns_window()
            .map_err(|error| error.to_string())?;
        if pointer.is_null() {
            return Ok(());
        }
        let native = unsafe { &*pointer.cast::<NSWindow>() };
        if native.isKindOfClass(objc2::class!(NSPanel)) {
            if native.isKeyWindow() {
                native.makeFirstResponder(None);
                native.orderOut(None);
                native.orderFrontRegardless();
            }
            return Ok(());
        }
        let current = NSRunningApplication::currentApplication();
        let still_ours = native.isKeyWindow()
            && NSWorkspace::sharedWorkspace()
                .frontmostApplication()
                .is_some_and(|application| application.isEqual(Some(&current)));
        if native.isKeyWindow() {
            // Tauri documents that set_focusable(false) alone cannot resign a key NSWindow.
            native.resignKeyWindow();
        }
        if still_ours {
            if let Some(previous) = previous.filter(|previous| !previous.0.isTerminated()) {
                // If the user already chose another app/window, still_ours is false.
                // AppKit may decline restoration; this never retries or forces activation.
                let _ = previous
                    .0
                    .activateWithOptions(NSApplicationActivationOptions::empty());
            }
        }
        Ok(())
    })
}
