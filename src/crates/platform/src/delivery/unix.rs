//! Private files and command lines on macOS and Linux.
use super::Result;
use std::{
    fs,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

/// Makes `path` and the folders above it, and closes it to everyone but this
/// user (0700).
pub fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

/// Options that create a new file to write, readable only by this user (0600).
pub(crate) fn new_file_options() -> fs::OpenOptions {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    options.mode(0o600);
    options
}

/// Whether an environment key names the command search path: `PATH`, as
/// written.
pub(crate) fn is_path_key(key: &str) -> bool {
    key == "PATH"
}

/// Where command lines are installed that a GUI app's `PATH` lacks: Homebrew's
/// folder and `/usr/local/bin`.
pub(crate) fn command_folders() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]
}
