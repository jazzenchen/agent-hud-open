//! Display changes wake the app's HUD window planner, which measures the screens
//! again. A host without a change notification is measured every `POLL`.
//! `full_screen` says which displays an app or video holds full screen now.
use std::collections::HashSet;

#[cfg(target_os = "macos")]
#[path = "screens/macos.rs"]
mod host;
#[cfg(target_os = "windows")]
#[path = "screens/windows.rs"]
mod host;
#[cfg(target_os = "linux")]
#[path = "screens/linux.rs"]
mod host;
pub use host::{full_screen, install, shutdown, POLL};

/// The displays a full-screen app or video holds, by screen id (the
/// display's UUID); every display when they share one set of desktops.
#[derive(Debug, Default, PartialEq)]
pub struct FullScreen {
    all: bool,
    ids: HashSet<String>,
}

impl FullScreen {
    pub(crate) fn holds(&self, screen_id: &str) -> bool {
        self.all || self.ids.contains(screen_id)
    }
}
