//! Where the app's data and its shared lock are, and moving earlier builds'
//! folders there at launch (`home_folder`, `data_folder`).
use crate::{data_folder, directory, home_folder, instance, os, NativePlatform, DATA_DIR_VARIABLE};
use std::path::{Path, PathBuf};

impl NativePlatform {
    /// The data folder: the one `AGENTHUD_DATA_DIR` names, else the app's
    /// own (`data_folder`): on Windows in `~/.agenthud`, or where an earlier
    /// build kept it until the app has moved it (`home_folder`).
    pub fn data_dir(&self) -> PathBuf {
        if let Some(path) = directory(DATA_DIR_VARIABLE) {
            return path;
        }
        if let Some(before) = self.unmoved_data() {
            return before;
        }
        data_folder::resolve(&os::data_parent(&self.home), os::DATA_FOLDER)
    }
    /// The data folder an earlier build kept outside `~/.agenthud`, while it
    /// is still there. Only a folder a collector used counts: one with the
    /// web view's files alone is not the data.
    fn unmoved_data(&self) -> Option<PathBuf> {
        let parent = os::legacy_data_parent(&self.home)?;
        let before = data_folder::resolve(&parent, data_folder::WINDOWS);
        (home_folder::unmoved(&before) && before.join(data_folder::LOCK).is_file())
            .then_some(before)
    }
    /// The data folders earlier builds kept outside `~/.agenthud`, each with
    /// where it goes. None when `AGENTHUD_DATA_DIR` names the folder.
    fn data_moves(&self) -> Vec<(PathBuf, PathBuf)> {
        let Some(parent) = os::legacy_data_parent(&self.home) else {
            return Vec::new();
        };
        if directory(DATA_DIR_VARIABLE).is_some() {
            return Vec::new();
        }
        let home = os::data_parent(&self.home);
        vec![
            (
                parent.join(data_folder::WINDOWS),
                home.join(os::DATA_FOLDER),
            ),
            (
                parent.join(data_folder::EARLIER),
                home.join(data_folder::EARLIER),
            ),
        ]
    }
    /// Whether any folder is still to move into `~/.agenthud`: the app's
    /// own, its lock's, or one in `others` (old place, new place), which an
    /// edition adds for folders of its own.
    fn home_moves_pending(&self, others: &[(PathBuf, PathBuf)]) -> bool {
        self.data_moves()
            .iter()
            .chain(others)
            .map(|(old, _)| old)
            .chain(os::legacy_lock_folder(&self.home).as_ref())
            .any(|old| home_folder::unmoved(old))
    }
    /// Moves the data folders into `~/.agenthud`, with `others`. Only a copy
    /// holding `shared_instance_lock` may call it, before it claims its data.
    fn move_data_home(&self, others: &[(PathBuf, PathBuf)]) -> Result<(), String> {
        let errors: Vec<String> = self
            .data_moves()
            .iter()
            .chain(others)
            .filter_map(|(old, new)| {
                home_folder::relocate(old, new)
                    .err()
                    .map(|error| format!("{} → {}: {error}", old.display(), new.display()))
            })
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
    /// Moves the shared lock's folder into `~/.agenthud`, once the copy that
    /// moved the data has let the lock go.
    fn move_lock_home(&self) -> Result<(), String> {
        let Some(old) = os::legacy_lock_folder(&self.home) else {
            return Ok(());
        };
        home_folder::relocate(&old, &os::shared_lock_folder(&self.home))
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    /// Moves the data folders earlier builds kept elsewhere into
    /// `~/.agenthud` (Windows), with `others` (old place, new place) an
    /// edition adds for folders of its own, and an earlier build's folder to
    /// the app's own name. An edition calls it at launch before it reads any
    /// of them; the collector's claim calls it again, then with nothing left
    /// to do. Nothing moves while another copy holds the shared lock: it may
    /// be an earlier build still writing there. Failures are only logged; the
    /// folders then stay in use where they are.
    pub fn move_data(&self, others: &[(PathBuf, PathBuf)], executable: Option<&Path>) {
        if !self.home_moves_pending(others) && self.earlier_data_dir().is_none() {
            return;
        }
        {
            let instance::Claim::Acquired(_lock) =
                instance::claim(&self.shared_instance_lock(), executable)
            else {
                return;
            };
            // A copy collecting into the old data folder (an earlier build,
            // or an engine it started) holds that folder's own lock.
            let free = self.unmoved_data().is_none_or(|old| {
                matches!(
                    instance::claim(&old.join(data_folder::LOCK), executable),
                    instance::Claim::Acquired(_)
                )
            });
            if !free {
                return;
            }
            if let Err(error) = self.move_data_home(others) {
                eprintln!("Data folders could not be moved into the home folder: {error}");
            }
            if let Err(error) = self.adopt_earlier_data() {
                eprintln!("The earlier data folder could not be moved: {error}");
            }
        }
        // The lock's own folder moves once it is let go.
        if let Err(error) = self.move_lock_home() {
            eprintln!("The lock's folder could not be moved into the home folder: {error}");
        }
    }
    /// An earlier build's data folder that has not been moved yet. None when
    /// `AGENTHUD_DATA_DIR` names the folder: then nothing is moved.
    pub fn earlier_data_dir(&self) -> Option<PathBuf> {
        if directory(DATA_DIR_VARIABLE).is_some() {
            return None;
        }
        data_folder::earlier(&os::data_parent(&self.home))
    }
    /// Moves an earlier build's data folder to the app's own. Only a copy
    /// holding `shared_instance_lock` may call it, before it claims its data.
    pub fn adopt_earlier_data(&self) -> Result<(), String> {
        if directory(DATA_DIR_VARIABLE).is_some() {
            return Ok(());
        }
        data_folder::adopt(&os::data_parent(&self.home), os::DATA_FOLDER)
            .map_err(|error| error.to_string())
    }
    /// The lock every Agent HUD that owns client hooks claims, whatever its
    /// data directory: agent-hud-native-open's `~/Library/Caches/app.agenthud/instance.lock`
    /// on macOS, so this app and the Swift app never run side by side; on
    /// Windows in `~/.agenthud`, or where earlier builds took it until it
    /// has moved, so a copy never runs beside an earlier one.
    pub fn shared_instance_lock(&self) -> PathBuf {
        let folder = os::shared_lock_folder(&self.home);
        os::legacy_lock_folder(&self.home)
            .map_or(folder.clone(), |old| home_folder::in_use(&old, &folder))
            .join("instance.lock")
    }
    /// The lock that keeps two copies from collecting into one data directory.
    pub fn instance_lock(&self) -> PathBuf {
        self.data_dir().join(data_folder::LOCK)
    }
}
