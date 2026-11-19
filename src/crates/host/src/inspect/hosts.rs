//! Chrome's note of the native messaging host: a small JSON file, in each
//! Chromium browser's folder, that names this app as the host the browser
//! extension may talk to. Written at each launch, so it follows the app
//! wherever it runs from. Each host file gives the folders and the browsers:
//!   folders(home)   the browsers' host-manifest folders, where they exist
//!   browsers()      their bundle identifiers, Chrome first, where the host
//!                   can open a page in a named one
use serde_json::{json, Value};
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
/// The extension's id when loaded from its folder, fixed by the public key
/// in its manifest.
const EXTENSION_ID: &str = "iihocfnohlhbepcckimekpoioandkpkl";
/// Its id in the Chrome Web Store, which the store gave it on the first
/// upload (2026-11-18).
const STORE_ID: Option<&str> = Some("eaihfcfnoaafgcggajlbegjnfjinkckk");

/// The extension's page in the Chrome Web Store, once it is there.
pub(crate) fn store_url() -> Option<String> {
    STORE_ID.map(|id| format!("https://chromewebstore.google.com/detail/{id}"))
}

/// Opens the store's page in a browser that can add the extension: the
/// system's browser when it is one of these, else Chrome or another one
/// installed. Safari or Firefox can show the page but not add it.
pub(crate) fn open_store() -> Result<Value, String> {
    let url = store_url().ok_or("The extension is not in the Chrome Web Store yet")?;
    let opened = agent_hud_runtime::open_link_in(&url, &host::browsers())?;
    Ok(json!({ "opened": opened }))
}

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
        "allowed_origins": std::iter::once(EXTENSION_ID)
            .chain(STORE_ID)
            .map(|id| format!("chrome-extension://{id}/"))
            .collect::<Vec<_>>(),
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
/// folder inside. Windows names them in the registry instead.
#[allow(dead_code)]
fn existing(root: &Path, browsers: &[&str]) -> Vec<PathBuf> {
    browsers
        .iter()
        .map(|browser| root.join(browser))
        .filter(|folder| folder.is_dir())
        .map(|folder| folder.join("NativeMessagingHosts"))
        .collect()
}
