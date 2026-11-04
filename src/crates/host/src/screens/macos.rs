//! NSApplicationDidChangeScreenParametersNotification wakes the HUD window planner.
use block2::RcBlock;
use dispatch2::MainThreadBound;
use objc2::{
    rc::Retained,
    runtime::{NSObjectProtocol, ProtocolObject},
    MainThreadMarker,
};
use objc2_app_kit::NSApplicationDidChangeScreenParametersNotification;
use objc2_foundation::{NSNotification, NSNotificationCenter};
use std::{
    ptr::NonNull,
    sync::{Mutex, PoisonError},
    time::Duration,
};
use tauri::Manager;

/// AppKit tells every change; nothing is measured on a timer.
pub const POLL: Option<Duration> = None;

struct Observer {
    center: Retained<NSNotificationCenter>,
    token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
}

impl Drop for Observer {
    fn drop(&mut self) {
        // MainThreadBound keeps registration, unregistration and token release
        // on the AppKit thread without claiming the opaque token is Send/Sync.
        unsafe { self.center.removeObserver((*self.token).as_ref()) };
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
    let center = NSNotificationCenter::defaultCenter();
    // AppKit posts this public notification after the system screen configuration
    // changes, matching IslandController.swift's application-level observer.
    let token = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSApplicationDidChangeScreenParametersNotification),
            None,
            None,
            &callback,
        )
    };
    app.manage(ScreenNotifications(Mutex::new(Some(MainThreadBound::new(
        Observer { center, token },
        marker,
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
