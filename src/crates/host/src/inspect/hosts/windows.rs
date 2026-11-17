//! Not implemented on Windows: the browser extension finds no host there.
//! Chrome reads the manifest's path from the registry, under
//! HKCU\Software\Google\Chrome\NativeMessagingHosts\<name> (and the same
//! for Edge and Brave); an adapter would write the file in the app's data
//! folder and that key with windows-registry.
use std::path::{Path, PathBuf};

pub(super) fn folders(_home: &Path) -> Vec<PathBuf> {
    Vec::new()
}
