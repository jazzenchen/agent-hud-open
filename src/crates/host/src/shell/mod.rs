//! The application shell's place in the operating system. Each host file
//! provides the same items, and callers never branch on the operating system:
//!   install(app)              at launch: global shortcut and initial Dock state
//!   window_opened(window)     Settings, Statistics or a sign-in window is about to show
//!   windows_changed(app)      Settings or Statistics was closed or hidden
//!   reopened(event)           the user asked for the app again (Dock click)
//!   toolbar_chrome(b)         a toolbar in the title bar (Statistics, markup)
//!   pointer_monitor(app)      the screen under the pointer
//!   system_languages()       the system's languages, before any setting is read
//!   alert(title, body, ok)    a blocking message, before the app starts
//!   clip_shortcut(app, key)   holds the clipboard window's shortcut, or none
//!   CLIP_SHORTCUT             whether this system can hold that shortcut
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod host;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod host;
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod host;
pub(crate) use host::{
    alert, clip_shortcut, install, pointer_monitor, prepare, reopened, system_languages,
    toolbar_chrome, window_opened, windows_changed, CLIP_SHORTCUT,
};
