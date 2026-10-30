//! Agent HUD's data folder, named after the app's identifier on each system.
//! Every edition of the app uses the same one.
//!
//! Builds before the app took those identifiers kept their data in
//! `app.agenthud.next`. The app moves that folder once at launch (`adopt`),
//! while it holds the lock every copy that owns client hooks takes. Until
//! then `resolve` keeps naming the earlier folder, so no copy reads half of
//! each.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// The folder's name on each system: the app's identifier there, as the
/// desktop app's Tauri configuration gives it. A build uses its own system's
/// name; the test below checks all three on every system.
#[allow(dead_code)]
pub(crate) const MAC: &str = "app.agenthud.mac";
#[allow(dead_code)]
pub(crate) const WINDOWS: &str = "app.agenthud.windows";
#[allow(dead_code)]
pub(crate) const LINUX: &str = "app.agenthud.linux";
/// The name earlier builds used on every system.
const EARLIER: &str = "app.agenthud.next";
/// The file every collector locks in the folder it uses. Once it is in this
/// build's folder, that folder is the one in use.
pub(crate) const LOCK: &str = "instance.lock";

/// The folder in use under `parent`: this build's once a collector has used
/// it, else an earlier build's while that one is still there.
pub(crate) fn resolve(parent: &Path, name: &str) -> PathBuf {
    let current = parent.join(name);
    let earlier = parent.join(EARLIER);
    if current.join(LOCK).exists() || !earlier.is_dir() {
        current
    } else {
        earlier
    }
}

/// An earlier build's folder under `parent`, while it is there.
pub(crate) fn earlier(parent: &Path) -> Option<PathBuf> {
    Some(parent.join(EARLIER)).filter(|path| path.is_dir())
}

/// Moves an earlier build's folder under `parent` to `name`.
///
/// What is already at `name` before the move is usually the web view's data
/// (Windows keeps it in the folder named after the app). It joins the earlier
/// folder, which then moves whole, so the data never sits in two places. A
/// folder an earlier build wrote after the move adds only what this build's
/// folder lacks, and is removed once nothing of it is left.
pub(crate) fn adopt(parent: &Path, name: &str) -> io::Result<()> {
    let earlier = parent.join(EARLIER);
    if !earlier.is_dir() {
        return Ok(());
    }
    let current = parent.join(name);
    if current.join(LOCK).exists() {
        return merge(&earlier, &current);
    }
    if current.is_dir() {
        merge(&current, &earlier)?;
    }
    fs::rename(&earlier, &current)
}

/// Moves each entry of `from` that `into` lacks, and the entries inside the
/// folders both have. A file both have stays as `into` has it. `from` is
/// removed when that leaves it empty.
fn merge(from: &Path, into: &Path) -> io::Result<()> {
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = into.join(entry.file_name());
        match fs::symlink_metadata(&target) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::rename(entry.path(), &target)?
            }
            Ok(existing) if existing.is_dir() && entry.file_type()?.is_dir() => {
                merge(&entry.path(), &target)?
            }
            _ => {}
        }
    }
    // Only an empty folder goes; one with a file `into` kept stays.
    let _ = fs::remove_dir(from);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn each_name_is_the_apps_identifier_on_its_system() {
        let tauri = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src-tauri");
        for (file, name) in [
            ("tauri.conf.json", MAC),
            ("tauri.windows.conf.json", WINDOWS),
            ("tauri.linux.conf.json", LINUX),
        ] {
            let config: serde_json::Value = serde_json::from_str(&read(&tauri.join(file))).unwrap();
            assert_eq!(config["identifier"], name, "{file}");
        }
    }

    #[test]
    fn the_earlier_folder_stays_in_use_until_this_one_has_been_used() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("data");
        assert_eq!(resolve(&parent, MAC), parent.join(MAC));
        fs::create_dir_all(parent.join(EARLIER)).unwrap();
        assert_eq!(resolve(&parent, MAC), parent.join(EARLIER));
        // The web view's folder alone does not make it this build's.
        fs::create_dir_all(parent.join(MAC).join("EBWebView")).unwrap();
        assert_eq!(resolve(&parent, MAC), parent.join(EARLIER));
        write(&parent.join(MAC).join(LOCK), "");
        assert_eq!(resolve(&parent, MAC), parent.join(MAC));
    }

    #[test]
    fn the_earlier_folder_moves_whole() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("data");
        let earlier = parent.join(EARLIER);
        write(&earlier.join("ledger.sqlite"), "ledger");
        write(&earlier.join("hooks/inbox/event.json"), "event");
        write(&earlier.join(LOCK), "");
        adopt(&parent, MAC).unwrap();
        let current = parent.join(MAC);
        assert!(!earlier.exists());
        assert_eq!(read(&current.join("ledger.sqlite")), "ledger");
        assert_eq!(read(&current.join("hooks/inbox/event.json")), "event");
        assert_eq!(resolve(&parent, MAC), current);
        assert_eq!(super::earlier(&parent), None);
    }

    #[test]
    fn the_web_views_folder_joins_the_moved_data() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("data");
        write(&parent.join(EARLIER).join("ledger.sqlite"), "ledger");
        write(&parent.join(EARLIER).join(LOCK), "");
        write(&parent.join(WINDOWS).join("EBWebView/Local State"), "web");
        adopt(&parent, WINDOWS).unwrap();
        let current = parent.join(WINDOWS);
        assert!(!parent.join(EARLIER).exists());
        assert_eq!(read(&current.join("ledger.sqlite")), "ledger");
        assert_eq!(read(&current.join("EBWebView/Local State")), "web");
    }

    #[test]
    fn a_folder_written_after_the_move_adds_only_what_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("data");
        let current = parent.join(LINUX);
        write(&current.join(LOCK), "");
        write(&current.join("ledger.sqlite"), "kept");
        write(&current.join("hooks/inbox/first.json"), "first");
        let earlier = parent.join(EARLIER);
        write(&earlier.join("hooks/inbox/late.json"), "late");
        adopt(&parent, LINUX).unwrap();
        assert_eq!(read(&current.join("hooks/inbox/late.json")), "late");
        assert_eq!(read(&current.join("hooks/inbox/first.json")), "first");
        assert!(!earlier.exists());
        // A file both have stays as this build's folder has it, and the
        // earlier folder keeps its copy rather than losing it.
        write(&earlier.join("ledger.sqlite"), "older");
        adopt(&parent, LINUX).unwrap();
        assert_eq!(read(&current.join("ledger.sqlite")), "kept");
        assert_eq!(read(&earlier.join("ledger.sqlite")), "older");
    }
}
