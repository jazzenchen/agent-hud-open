//! Display changes: not observed yet on this host, so the HUD window
//! planner measures the screens every few seconds. An adapter would listen
//! for WM_DISPLAYCHANGE and call `crate::coordinator::screens_changed`.
use std::time::Duration;

pub const POLL: Option<Duration> = Some(Duration::from_secs(3));

pub fn install(_app: &tauri::App) -> Result<(), String> {
    Ok(())
}

pub fn shutdown(_app: &tauri::AppHandle) {}

/// Not read on this host yet: no HUD hides for a full-screen app.
pub fn full_screen() -> super::FullScreen {
    super::FullScreen::default()
}
