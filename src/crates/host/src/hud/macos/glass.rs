//! The system's glass (NSGlassEffectView, macOS 26) under the HUD's web view,
//! cut to the outlines the page reports. AppKit's public shape is a corner
//! radius only, and a layer mask cuts the glass's bright rim off with the
//! rest; `_setPath:`, the setter the system's own glass takes any outline
//! with, keeps the rim on the outline. It is private, so it is asked for
//! first: without it, or before macOS 26, there is no glass, and the page
//! keeps its see-through ground.
use crate::hud::glass::GlassShape;
use objc2::encode::{Encoding, RefEncode};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{msg_send, sel, Message};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSView, NSWindow, NSWindowOrderingMode};
use objc2_foundation::{NSObjectProtocol, NSString};
use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::c_void,
    panic::AssertUnwindSafe,
    sync::atomic::{AtomicBool, Ordering},
};

/// CoreGraphics' path, by reference only.
#[repr(C)]
struct CGPath {
    _private: [u8; 0],
}

unsafe impl RefEncode for CGPath {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("CGPath", &[]));
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPathCreateMutable() -> *mut CGPath;
    fn CGPathMoveToPoint(path: *mut CGPath, transform: *const c_void, x: f64, y: f64);
    fn CGPathAddLineToPoint(path: *mut CGPath, transform: *const c_void, x: f64, y: f64);
    fn CGPathAddQuadCurveToPoint(
        path: *mut CGPath,
        transform: *const c_void,
        cx: f64,
        cy: f64,
        x: f64,
        y: f64,
    );
    #[allow(clippy::too_many_arguments)]
    fn CGPathAddCurveToPoint(
        path: *mut CGPath,
        transform: *const c_void,
        c1x: f64,
        c1y: f64,
        c2x: f64,
        c2y: f64,
        x: f64,
        y: f64,
    );
    fn CGPathCloseSubpath(path: *mut CGPath);
    fn CGPathRelease(path: *mut CGPath);
}

thread_local! {
    /// Each HUD window's glass, by window label; main thread only.
    static GLASS: RefCell<HashMap<String, Retained<NSView>>> = RefCell::new(HashMap::new());
}

/// Set once AppKit refused the glass, which then stays off until the app restarts.
static REFUSED: AtomicBool = AtomicBool::new(false);

/// The glass class, when this Mac has one that takes an outline.
fn glass_class() -> Option<&'static AnyClass> {
    if REFUSED.load(Ordering::Relaxed) {
        return None;
    }
    AnyClass::get(c"NSGlassEffectView").filter(|class| class.responds_to(sel!(_setPath:)))
}

pub(in crate::hud) fn set_glass(
    window: &tauri::WebviewWindow,
    shapes: Vec<GlassShape>,
    dark: bool,
) -> Result<bool, String> {
    if glass_class().is_none() {
        return Ok(false);
    }
    let retained = window.clone();
    // Fire and forget: one report a frame while a surface moves, and the main
    // queue keeps their order.
    window
        .run_on_main_thread(move || {
            // An Objective-C exception must not unwind into the event loop,
            // which aborts the app; it turns the glass off instead.
            match objc2::exception::catch(AssertUnwindSafe(|| apply(&retained, &shapes, dark))) {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!("HUD glass: {error}"),
                Err(exception) => {
                    REFUSED.store(true, Ordering::Relaxed);
                    eprintln!("HUD glass: AppKit raised {exception:?}; glass is off until restart");
                }
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(true)
}

fn apply(window: &tauri::WebviewWindow, shapes: &[GlassShape], dark: bool) -> Result<(), String> {
    let class = glass_class().ok_or("NSGlassEffectView is unavailable")?;
    let pointer = window.ns_window().map_err(|error| error.to_string())?;
    if pointer.is_null() {
        return Ok(());
    }
    // The retained Tauri window keeps this native object alive for this main-thread call.
    let native = unsafe { &*pointer.cast::<NSWindow>() };
    // Borrowed, never owned: the window holds its views through this call.
    // Taking AppKit's back-pointers into `Retained` over-released wry's
    // content view, one release a report, until AppKit raised on freeing it
    // while still on screen (2026-11-16).
    let content: *mut NSView = unsafe { msg_send![native, contentView] };
    let content = unsafe { content.as_ref() }.ok_or("The HUD window has no content view")?;
    let web = web_view(content).ok_or("The HUD window has no web view")?;
    let parent: *mut NSView = unsafe { msg_send![&*web, superview] };
    let parent = unsafe { parent.as_ref() }.ok_or("The HUD web view has no parent")?;
    let label = window.label().to_owned();
    GLASS.with(|glass| place(glass, &label, class, parent, &web, shapes, dark))
}

fn place(
    glass: &RefCell<HashMap<String, Retained<NSView>>>,
    label: &str,
    class: &AnyClass,
    parent: &NSView,
    web: &NSView,
    shapes: &[GlassShape],
    dark: bool,
) -> Result<(), String> {
    let mut views = glass.borrow_mut();
    let current = views
        .get(label)
        .filter(|view| {
            let view: &NSView = view;
            let owner: *mut NSView = unsafe { msg_send![view, superview] };
            std::ptr::eq(owner, parent)
        })
        .cloned();
    let view = match current {
        Some(view) => view,
        None => {
            if shapes.is_empty() {
                return Ok(());
            }
            if let Some(old) = views.remove(label) {
                old.removeFromSuperview();
            }
            let view: Retained<NSView> = unsafe { msg_send![class, new] };
            view.setWantsLayer(true);
            view.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            // Clear: the page's own ground over it gives the look its tone,
            // and Regular would flip light and dark by its size.
            let _: () = unsafe { msg_send![&*view, setStyle: 1isize] };
            parent.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, Some(web));
            views.insert(label.to_owned(), view.clone());
            view
        }
    };
    without_animation(|| {
        if shapes.is_empty() {
            view.setHidden(true);
            return;
        }
        let frame = web.frame();
        view.setFrame(frame);
        set_appearance(&view, dark);
        let path = outline(shapes, frame.size.height);
        let _: () = unsafe { msg_send![&*view, _setPath: path.cast_const()] };
        unsafe { CGPathRelease(path) };
        view.setHidden(false);
    });
    Ok(())
}

/// The outlines as one path, turned from the page's top-down points into the
/// view's bottom-up ones.
fn outline(shapes: &[GlassShape], height: f64) -> *mut CGPath {
    let none = std::ptr::null();
    unsafe {
        let path = CGPathCreateMutable();
        for shape in shapes {
            for command in &shape.0 {
                let y = |value: f64| height - value;
                match command[0] as u8 {
                    0 => CGPathMoveToPoint(path, none, command[1], y(command[2])),
                    1 => CGPathAddLineToPoint(path, none, command[1], y(command[2])),
                    2 => CGPathAddQuadCurveToPoint(
                        path,
                        none,
                        command[1],
                        y(command[2]),
                        command[3],
                        y(command[4]),
                    ),
                    3 => CGPathAddCurveToPoint(
                        path,
                        none,
                        command[1],
                        y(command[2]),
                        command[3],
                        y(command[4]),
                        command[5],
                        y(command[6]),
                    ),
                    _ => CGPathCloseSubpath(path),
                }
            }
        }
        path
    }
}

fn set_appearance(view: &NSView, dark: bool) {
    let Some(class) = AnyClass::get(c"NSAppearance") else {
        return;
    };
    let name = NSString::from_str(if dark {
        "NSAppearanceNameDarkAqua"
    } else {
        "NSAppearanceNameAqua"
    });
    let appearance: Option<Retained<AnyObject>> =
        unsafe { msg_send![class, appearanceNamed: &*name] };
    let _: () = unsafe { msg_send![view, setAppearance: appearance.as_deref()] };
}

/// Moves the glass with the page's frame, not after it with Core Animation's
/// own easing.
fn without_animation(change: impl FnOnce()) {
    let Some(transaction) = AnyClass::get(c"CATransaction") else {
        change();
        return;
    };
    unsafe {
        let _: () = msg_send![transaction, begin];
        let _: () = msg_send![transaction, setDisableActions: true];
    }
    change();
    let _: () = unsafe { msg_send![transaction, commit] };
}

fn web_view(view: &NSView) -> Option<Retained<NSView>> {
    if view.isKindOfClass(objc2::class!(WKWebView)) {
        return Some(view.retain());
    }
    view.subviews().iter().find_map(|child| web_view(&child))
}
