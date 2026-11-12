//! The application shell's place in the operating system. Each host file
//! provides the same items, and callers never branch on the operating system:
//!   install(app)              at launch: global shortcut and initial Dock state
//!   window_opened(app)        Settings or Statistics was shown
//!   windows_changed(app)      Settings or Statistics was closed or hidden
//!   reopened(event)           the user asked for the app again (Dock click)
//!   statistics_chrome(b)      the Statistics window's title bar
//!   system_prefers_chinese()  the system's language, before any setting is read
//!   alert(title, body, ok)    a blocking message, before the app starts
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
    alert, install, prepare, reopened, statistics_chrome, system_prefers_chinese, window_opened,
    windows_changed,
};
