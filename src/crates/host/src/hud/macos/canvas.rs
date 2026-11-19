//! The HUD's web view: held at the canvas's place on screen, and kept clear
//! where the page has not painted yet.
use super::super::LogicalRect;
use objc2::{runtime::NSObjectProtocol, ClassType};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSWindow};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use objc2_web_kit::WKWebView;

/// Sizes the web view to the whole canvas and holds it where the canvas is on
/// the screen, so the window only shows part of it. Set in the same main-thread
/// pass as the window's frame, a window that grows or shrinks away from its
/// edge (a right or bottom HUD picked up, dropped, opened or closed) only
/// uncovers or covers more of the page. Resizing the web view instead made
/// WebKit show its old picture against the window's new corner for the frame
/// or two it took to lay the page out again: the dock flashed out of sight,
/// or across the screen.
pub(super) fn hold_page(native: &NSWindow, canvas: LogicalRect, content: LogicalRect) {
    let Some(parent) = native.contentView() else {
        return;
    };
    let below_top = canvas.y - content.y;
    let frame = NSRect::new(
        NSPoint::new(
            canvas.x - content.x,
            if parent.isFlipped() {
                below_top
            } else {
                content.height - below_top - canvas.height
            },
        ),
        NSSize::new(canvas.width, canvas.height),
    );
    for view in parent.subviews().iter() {
        if view.isKindOfClass(WKWebView::class()) {
            view.setAutoresizingMask(NSAutoresizingMaskOptions::empty());
            view.setFrame(frame);
        }
    }
}

/// WebKit fills the part of a view it has not painted yet with the "under
/// page" color, dark by default. A HUD window that grows before its panel
/// opens would show that as a wide black band for a frame or two, so the HUD's
/// is clear, like the rest of its transparent window.
pub(super) fn clear_under_page(window: &tauri::WebviewWindow) {
    use objc2_app_kit::NSColor;

    let _ = window.with_webview(|platform| {
        let pointer = platform.inner().cast::<WKWebView>();
        if pointer.is_null() {
            return;
        }
        let view = unsafe { &*pointer };
        unsafe { view.setUnderPageBackgroundColor(Some(&NSColor::clearColor())) };
    });
}
