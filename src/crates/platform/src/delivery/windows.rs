//! What sending and starting need from Windows: folders and files closed to
//! other users, npm's command folder, and `rundll32` for links.
use super::{Failure, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Makes `path` a folder only this user can open (`crate::os::private_directory`).
pub fn private_dir(path: &Path) -> Result<()> {
    crate::os::private_directory(path).map_err(Failure::Io)
}

/// Options that create a new file to write. It takes the protection of the
/// private folder it is made in.
pub(crate) fn new_file_options() -> fs::OpenOptions {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    options
}

/// Whether an environment key names the command search path: `PATH` in any
/// case, as Windows reads it.
pub(crate) fn is_path_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("PATH")
}

/// Where command lines are installed that a GUI app's `PATH` lacks: npm's
/// global folder under `%APPDATA%`.
pub(crate) fn command_folders() -> Vec<PathBuf> {
    std::env::var_os("APPDATA")
        .map(|appdata| PathBuf::from(appdata).join("npm"))
        .into_iter()
        .collect()
}

/// Desktop apps are not looked for on Windows yet, so offers and starts go
/// by the command lines alone; an adapter would look up each app's install
/// location (its Start menu or Store package entry).
pub(crate) fn applications(_home: &Path) -> Vec<PathBuf> {
    Vec::new()
}

/// The command that opens a client link: `url.dll`'s protocol handler.
pub(crate) fn link_opener(uri: &str) -> Result<(PathBuf, Vec<String>)> {
    Ok((
        std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| Failure::Unavailable("Windows system directory".into()))?
            .join("System32/rundll32.exe"),
        vec!["url.dll,FileProtocolHandler".to_string(), uri.to_string()],
    ))
}

/// Where tests make their folders: the system's temporary folder.
#[cfg(any(test, feature = "fixtures"))]
pub(crate) fn fixture_root() -> PathBuf {
    std::env::temp_dir()
}
