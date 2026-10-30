//! The original Mac application's declared ledger; no app launch or auth reads.
//!
//! The installed bundle's `AgentHUDDataDirectory` names the folder. An in-place
//! update replaces that bundle with the app built from this repository, so
//! that app must declare the same key, or this lookup falls back to the
//! Swift app's own folder.
use std::path::{Path, PathBuf};
pub(super) fn quota_database(home: &Path) -> Option<PathBuf> {
    use std::{
        fs::File,
        io::{Cursor, Read},
    };
    let support = home.join("Library/Application Support");
    for bundle in [
        PathBuf::from("/Applications/Agent HUD.app"),
        home.join("Applications/Agent HUD.app"),
    ] {
        let Some(file) = File::open(bundle.join("Contents/Info.plist")).ok() else {
            continue;
        };
        let mut bytes = Vec::new();
        if file.take(1024 * 1024 + 1).read_to_end(&mut bytes).is_err() || bytes.len() > 1024 * 1024
        {
            continue;
        }
        let Some(value) = plist::Value::from_reader(Cursor::new(bytes)).ok() else {
            continue;
        };
        let Some(name) = value
            .as_dictionary()
            .and_then(|value| value.get("AgentHUDDataDirectory"))
            .and_then(plist::Value::as_string)
        else {
            continue;
        };
        // AppSupport.directoryName is a single application-support child.
        let mut parts = Path::new(name).components();
        if !matches!(parts.next(), Some(std::path::Component::Normal(_))) || parts.next().is_some()
        {
            continue;
        }
        let path = support.join(name).join("usage-ledger.sqlite");
        if path.is_file() {
            return Some(path);
        }
    }
    // The source app's documented default outside an installed bundle.
    let path = support.join("Agent HUD Open/usage-ledger.sqlite");
    path.is_file().then_some(path)
}
