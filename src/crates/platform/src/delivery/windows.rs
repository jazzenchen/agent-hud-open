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

/// Installed Windows desktop apps (Claude, Codex, Antigravity, DeepSeek
/// Harness), found from running processes, Start menu / desktop shortcuts,
/// standard install folders and Store packages.
pub(crate) fn applications(home: &Path) -> Vec<PathBuf> {
    crate::os::desktop_applications(home)
}

/// Whether `provider`'s desktop app is installed on Windows. Checked per
/// provider so asking about Claude, Antigravity or DeepSeek never runs Codex's
/// Store package lookup.
pub(crate) fn desktop_installed(home: &Path, provider: super::Provider) -> bool {
    let bundle = match provider {
        super::Provider::Claude => "com.anthropic.claudefordesktop",
        super::Provider::Codex => "com.openai.codex",
        super::Provider::Antigravity => "com.google.antigravity",
        super::Provider::DeepSeek => "com.deepseek.dsh",
        _ => return false,
    };
    !crate::os::desktop_for(home, bundle).is_empty()
}

/// The Codex desktop app's programs (`Codex.exe`, `ChatGPT.exe` or the Store
/// package's), so a queued message can wake its thread as on macOS.
pub(crate) fn codex_applications(_home: &Path) -> Vec<PathBuf> {
    crate::os::codex_applications()
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
