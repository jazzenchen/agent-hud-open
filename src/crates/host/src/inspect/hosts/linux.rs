//! The Chromium browsers' folders in ~/.config.
use std::path::{Path, PathBuf};

pub(super) fn folders(home: &Path) -> Vec<PathBuf> {
    super::existing(
        &home.join(".config"),
        &[
            "google-chrome",
            "google-chrome-beta",
            "chromium",
            "microsoft-edge",
            "BraveSoftware/Brave-Browser",
            "vivaldi",
        ],
    )
}

/// None named here: the store's page opens in the system's browser.
pub(super) fn browsers() -> Vec<&'static str> {
    Vec::new()
}
