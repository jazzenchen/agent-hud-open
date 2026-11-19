//! The Chromium browsers: each one's folder in Application Support and its
//! bundle identifier.
use std::path::{Path, PathBuf};

const BROWSERS: &[(&str, &str)] = &[
    ("Google/Chrome", "com.google.Chrome"),
    ("Google/Chrome Beta", "com.google.Chrome.beta"),
    ("Google/Chrome Dev", "com.google.Chrome.dev"),
    ("Google/Chrome Canary", "com.google.Chrome.canary"),
    ("Chromium", "org.chromium.Chromium"),
    ("Microsoft Edge", "com.microsoft.edgemac"),
    ("BraveSoftware/Brave-Browser", "com.brave.Browser"),
    ("Arc/User Data", "company.thebrowser.Browser"),
    ("Vivaldi", "com.vivaldi.Vivaldi"),
];

pub(super) fn folders(home: &Path) -> Vec<PathBuf> {
    let browsers = BROWSERS
        .iter()
        .map(|(folder, _)| *folder)
        .collect::<Vec<_>>();
    super::existing(&home.join("Library/Application Support"), &browsers)
}

pub(super) fn browsers() -> Vec<&'static str> {
    BROWSERS.iter().map(|(_, id)| *id).collect()
}
