//! Agent HUD's folder in the home folder, `~/.agenthud`, and moving data
//! folders into it.
//!
//! On Windows everything the app keeps lives there, never in AppData: a
//! client packaged as MSIX (the Claude desktop app) has its writes under
//! AppData redirected into its own package, so a folder it shares with the
//! app there splits in two. Builds before this kept their folders in
//! `%LOCALAPPDATA%`; the app moves each one at launch while it holds the lock
//! every copy that owns client hooks takes (`relocate`).
//!
//! A moved folder leaves a directory junction behind pointing at its new
//! place. This build never uses the old path: a junction there means moved
//! (`unmoved`). The junction is for what still runs or starts from the old
//! path: a tools relay an agent started before the update (its pipe's name
//! comes from the folder's resolved path, which is the same through the
//! junction) and an older build put back.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// The folder's name in the home folder.
pub const NAME: &str = ".agenthud";

/// `~/.agenthud` under `home`.
pub fn directory(home: &Path) -> PathBuf {
    home.join(NAME)
}

/// Whether `path` is a real folder not moved yet: not a junction or link
/// left by a move, and not missing.
pub fn unmoved(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir() && !is_link(&metadata))
}

/// The folder in use of an old place and its new one: the old one until it
/// has been moved.
pub fn in_use(old: &Path, new: &Path) -> PathBuf {
    if unmoved(old) {
        old.to_owned()
    } else {
        new.to_owned()
    }
}

/// Moves the folder at `old` to `new` and leaves a junction at `old`.
/// Returns whether it moved anything. Only a copy that holds the shared
/// lock may call it, so no Agent HUD is writing into either folder.
///
/// The old folder moves whole, so it keeps its owner-only permissions,
/// which a folder made at `new` meanwhile (the web view's, by Tauri) lacks.
/// What is already at `new` joins it first: entries the old folder lacks
/// move in; any it has too stay behind and are set aside beside `new`, so
/// the old folder's own files win and nothing is lost. A move across
/// volumes copies, then removes the old folder.
pub fn relocate(old: &Path, new: &Path) -> io::Result<bool> {
    if !unmoved(old) {
        return Ok(false);
    }
    if let Some(parent) = new.parent() {
        fs::create_dir_all(parent)?;
    }
    if new.exists() {
        merge(new, old)?;
        if new.exists() {
            fs::rename(new, set_aside(new))?;
        }
    }
    if let Err(error) = fs::rename(old, new) {
        if !crate::os::other_volume(&error) {
            return Err(error);
        }
        copy(old, new)?;
        fs::remove_dir_all(old)?;
    }
    if let Err(error) = crate::os::link_folder(old, new) {
        eprintln!(
            "No link was left at {} for {}: {error}",
            old.display(),
            new.display()
        );
    }
    Ok(true)
}

fn is_link(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink() || crate::os::is_reparse_point(metadata)
}

/// Where what is left of the old folder goes, beside the new one, when
/// files both have stay in it.
fn set_aside(new: &Path) -> PathBuf {
    let name = new
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_secs())
        .unwrap_or_default();
    new.with_file_name(format!("{name}.left-{stamp}"))
}

/// Moves each entry of `from` that `into` lacks, and the entries inside the
/// folders both have. `from` is removed when that leaves it empty.
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
    let _ = fs::remove_dir(from);
    Ok(())
}

/// Copies the tree at `from` to `into`, which does not exist yet. Links in
/// it are not followed.
fn copy(from: &Path, into: &Path) -> io::Result<()> {
    fs::create_dir(into)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = into.join(entry.file_name());
        if kind.is_dir() {
            copy(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests_unix;
#[cfg(all(test, windows))]
mod tests_windows;
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
    fn a_folder_moves_whole_and_the_new_place_is_then_in_use() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("AppData/Local/app.agenthud.windows");
        let new = directory(&temp.path().join("home")).join("data");
        write(&old.join("ingestion.sqlite"), "ledger");
        write(&old.join("hooks/inbox/event.json"), "event");
        assert_eq!(in_use(&old, &new), old);
        assert!(relocate(&old, &new).unwrap());
        assert_eq!(read(&new.join("ingestion.sqlite")), "ledger");
        assert_eq!(read(&new.join("hooks/inbox/event.json")), "event");
        assert_eq!(in_use(&old, &new), new);
        assert!(!relocate(&old, &new).unwrap());
    }

    #[test]
    fn nothing_moves_when_there_is_no_old_folder() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        assert!(!relocate(&old, &new).unwrap());
        assert!(!new.exists());
        assert_eq!(in_use(&old, &new), new);
    }

    #[test]
    fn the_old_folders_files_win_and_the_new_places_others_join() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        write(&new.join("EBWebView/Local State"), "fresh web view");
        write(&new.join("settings.json"), "made later");
        write(&old.join("settings.json"), "the user's");
        write(&old.join("ingestion.sqlite"), "ledger");
        assert!(relocate(&old, &new).unwrap());
        assert_eq!(read(&new.join("settings.json")), "the user's");
        assert_eq!(read(&new.join("ingestion.sqlite")), "ledger");
        assert_eq!(read(&new.join("EBWebView/Local State")), "fresh web view");
        let aside: Vec<_> = fs::read_dir(temp.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("new.left-"))
            .collect();
        assert_eq!(aside.len(), 1);
        assert_eq!(read(&aside[0].path().join("settings.json")), "made later");
        assert!(!unmoved(&old));
    }
}
