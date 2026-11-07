//! What sending and starting need from macOS: Unix's private files and
//! command lines, the folders apps are installed in, and `open` for links.
pub use super::unix::private_dir;
pub(crate) use super::unix::{command_folders, is_path_key, new_file_options};
use super::Result;
use std::path::{Path, PathBuf};

/// Where desktop apps are installed: `/Applications` and `~/Applications`.
pub(crate) fn applications(home: &Path) -> Vec<PathBuf> {
    vec![PathBuf::from("/Applications"), home.join("Applications")]
}

/// The command that opens a client link, in the background.
pub(crate) fn link_opener(uri: &str) -> Result<(PathBuf, Vec<String>)> {
    Ok((
        PathBuf::from("/usr/bin/open"),
        vec!["-g".to_string(), uri.to_string()],
    ))
}

/// Where tests make their folders: `/private/tmp`, whose short paths keep
/// test sockets within macOS's 104-byte socket path limit.
#[cfg(any(test, feature = "fixtures"))]
pub(crate) fn fixture_root() -> PathBuf {
    PathBuf::from("/private/tmp")
}
