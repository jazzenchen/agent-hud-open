//! The Chromium browsers' folders in Application Support.
use std::path::{Path, PathBuf};

pub(super) fn folders(home: &Path) -> Vec<PathBuf> {
    super::existing(
        &home.join("Library/Application Support"),
        &[
            "Google/Chrome",
            "Google/Chrome Beta",
            "Google/Chrome Dev",
            "Google/Chrome Canary",
            "Chromium",
            "Microsoft Edge",
            "BraveSoftware/Brave-Browser",
            "Arc/User Data",
            "Vivaldi",
        ],
    )
}
