//! Chrome's note of the native messaging host: a small JSON file, in each
//! Chromium browser's folder, that names this app as the host the browser
//! extension may talk to. Written at each launch, so it follows the app
//! wherever it runs from. Each host file gives the folders:
//!   folders(home)   the browsers' host-manifest folders, where they exist
use serde_json::json;
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
#[path = "hosts/macos.rs"]
mod host;
#[cfg(target_os = "windows")]
#[path = "hosts/windows.rs"]
mod host;
#[cfg(target_os = "linux")]
#[path = "hosts/linux.rs"]
mod host;

/// The host's name, as the extension asks for it.
pub(crate) const NAME: &str = "app.agenthud.inspect";
/// The extension's id, fixed by the public key in its manifest.
const EXTENSION_ID: &str = "iihocfnohlhbepcckimekpoioandkpkl";

/// Writes the note into every browser folder there is, where it differs.
pub(super) fn install(executable: &Path) {
    let Ok(home) = agent_hud_runtime::home_directory() else {
        return;
    };
    let note = json!({
        "name": NAME,
        "description": "Agent HUD: sends what the browser extension picks to an agent session",
        "path": executable,
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")],
    });
    let bytes = serde_json::to_vec_pretty(&note).unwrap_or_default();
    for folder in host::folders(&home) {
        let file = folder.join(format!("{NAME}.json"));
        if std::fs::read(&file).is_ok_and(|current| current == bytes) {
            continue;
        }
        if std::fs::create_dir_all(&folder).is_ok() {
            let _ = std::fs::write(&file, &bytes);
        }
    }
}

/// The browser folders under `root` that exist, each with its host-manifest
/// folder inside.
fn existing(root: &Path, browsers: &[&str]) -> Vec<PathBuf> {
    browsers
        .iter()
        .map(|browser| root.join(browser))
        .filter(|folder| folder.is_dir())
        .map(|folder| folder.join("NativeMessagingHosts"))
        .collect()
}
