//! What sending and starting need from Linux: Unix's private files and
//! command lines, and `xdg-open` for links.
pub use super::unix::private_dir;
pub(crate) use super::unix::{command_folders, is_path_key, new_file_options};
use super::Result;
use std::path::{Path, PathBuf};

/// Desktop apps are not looked for on Linux yet, so offers and starts go by
/// the command lines alone; an adapter would read the apps' installed
/// `.desktop` entries.
pub(crate) fn applications(_home: &Path) -> Vec<PathBuf> {
    Vec::new()
}

/// The command that opens a client link.
pub(crate) fn link_opener(uri: &str) -> Result<(PathBuf, Vec<String>)> {
    Ok((PathBuf::from("/usr/bin/xdg-open"), vec![uri.to_string()]))
}

/// Where tests make their folders: the system's temporary folder.
#[cfg(any(test, feature = "fixtures"))]
pub(crate) fn fixture_root() -> PathBuf {
    std::env::temp_dir()
}
