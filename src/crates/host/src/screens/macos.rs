//! NSApplicationDidChangeScreenParametersNotification wakes the HUD window
//! planner, and so does NSWorkspace's active-Space change, which comes as an
//! app or video goes full screen on a display or leaves it.
use block2::RcBlock;
use dispatch2::MainThreadBound;
use objc2::{
    rc::Retained,
    runtime::{AnyObject, NSObjectProtocol, ProtocolObject},
    MainThreadMarker,
};
use objc2_app_kit::{
    NSApplicationDidChangeScreenParametersNotification, NSWorkspace,
    NSWorkspaceActiveSpaceDidChangeNotification,
};
use objc2_foundation::{
    ns_string, NSArray, NSDictionary, NSNotification, NSNotificationCenter, NSNotificationName,
    NSNumber, NSString,
};
use std::{
    ffi::{c_char, c_void},
    ptr::NonNull,
    sync::{Mutex, PoisonError},
    time::Duration,
};
use tauri::Manager;

/// AppKit tells every change; nothing is measured on a timer.
pub const POLL: Option<Duration> = None;

type Registration = (
    Retained<NSNotificationCenter>,
    Retained<ProtocolObject<dyn NSObjectProtocol>>,
);

struct Observer(Vec<Registration>);

impl Drop for Observer {
    fn drop(&mut self) {
        // MainThreadBound keeps registration, unregistration and token release
        // on the AppKit thread without claiming the opaque token is Send/Sync.
        for (center, token) in &self.0 {
            unsafe { center.removeObserver((**token).as_ref()) };
        }
    }
}

struct ScreenNotifications(Mutex<Option<MainThreadBound<Observer>>>);

pub fn install(app: &tauri::App) -> Result<(), String> {
    if app.try_state::<ScreenNotifications>().is_some() {
        return Ok(());
    }
    let marker = MainThreadMarker::new()
        .ok_or("Display notifications must be installed on the AppKit thread")?;
    let handle = app.handle().clone();
    let callback = RcBlock::new(move |_: NonNull<NSNotification>| {
        // No AppKit queries or synchronous work in the notification callback.
        crate::coordinator::screens_changed(&handle);
    });
    let observe = |center: Retained<NSNotificationCenter>, name: &NSNotificationName| {
        let token = unsafe {
            center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &callback)
        };
        (center, token)
    };
    let observer = Observer(vec![
        // AppKit posts this public notification after the system screen configuration
        // changes, matching IslandController.swift's application-level observer.
        observe(NSNotificationCenter::defaultCenter(), unsafe {
            NSApplicationDidChangeScreenParametersNotification
        }),
        // A display shows another desktop: a screen whose HUD hides in full
        // screen may lose or regain it (Open #117).
        observe(
            NSWorkspace::sharedWorkspace().notificationCenter(),
            unsafe { NSWorkspaceActiveSpaceDidChangeNotification },
        ),
    ]);
    app.manage(ScreenNotifications(Mutex::new(Some(MainThreadBound::new(
        observer, marker,
    )))));
    Ok(())
}

pub fn shutdown(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<ScreenNotifications>() {
        let observer = state
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        // Release the state lock before running the main-thread destructor.
        drop(observer);
    }
}

unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}
/// `RTLD_DEFAULT`: every image the app has loaded.
const LOADED: *mut c_void = -2isize as *mut c_void;
type ConnectionId = unsafe extern "C" fn() -> i32;
type CopySpaces = unsafe extern "C" fn(i32) -> *mut NSArray<NSDictionary<NSString, AnyObject>>;

/// The displays showing a full-screen app's desktop now. Only CoreGraphics'
/// undocumented list of each display's desktops tells it, so both calls are
/// looked up as the app runs: a system without them hides no HUD, rather
/// than keeping the app from starting.
pub fn full_screen() -> super::FullScreen {
    let (connection, copy) = unsafe {
        (
            dlsym(LOADED, c"CGSMainConnectionID".as_ptr()),
            dlsym(LOADED, c"CGSCopyManagedDisplaySpaces".as_ptr()),
        )
    };
    if connection.is_null() || copy.is_null() {
        return super::FullScreen::default();
    }
    let (connection, copy) = unsafe {
        (
            std::mem::transmute::<*mut c_void, ConnectionId>(connection),
            std::mem::transmute::<*mut c_void, CopySpaces>(copy),
        )
    };
    // A Copy call: the array is the app's to release.
    let Some(displays) = (unsafe { Retained::from_raw(copy(connection())) }) else {
        return super::FullScreen::default();
    };
    let spaces: Vec<(String, i64)> = displays
        .iter()
        .filter_map(|display| {
            let id = display.objectForKey(ns_string!("Display Identifier"))?;
            let current = display.objectForKey(ns_string!("Current Space"))?;
            let current = current.downcast_ref::<NSDictionary>()?;
            let kind = current.objectForKey(ns_string!("type"))?;
            Some((
                id.downcast_ref::<NSString>()?.to_string(),
                kind.downcast_ref::<NSNumber>()?.integerValue() as i64,
            ))
        })
        .collect();
    held(spaces.iter().map(|(id, kind)| (id.as_str(), *kind)))
}

/// From each display's identifier and the type of the desktop it shows now,
/// as macOS lists them: type 4 is a full-screen app's own desktop, and
/// "Main" stands for every display while they share their desktops.
fn held<'a>(spaces: impl IntoIterator<Item = (&'a str, i64)>) -> super::FullScreen {
    let mut full = super::FullScreen::default();
    for (display, _) in spaces.into_iter().filter(|(_, kind)| *kind == 4) {
        if display == "Main" {
            full.all = true;
        } else {
            full.ids.insert(display.to_owned());
        }
    }
    full
}

#[cfg(test)]
mod tests {
    use super::{super::FullScreen, held};

    #[test]
    fn only_a_display_showing_a_full_screen_desktop_is_held() {
        let full = held([("37D8-A", 4), ("A6A1-B", 0)]);
        assert!(full.holds("37D8-A"));
        assert!(!full.holds("A6A1-B"));
        assert!(!full.holds("display:3"));
        assert_eq!(held([("37D8-A", 0)]), FullScreen::default());
        // Displays that share their desktops are listed once, as "Main".
        let shared = held([("Main", 4)]);
        assert!(shared.holds("37D8-A") && shared.holds("A6A1-B"));
    }
}
