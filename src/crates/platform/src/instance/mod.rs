//! Exclusive ownership of the collector's data and, separately, client hooks.
//!
//! Every collector holds its data directory's lock. A hook owner also holds
//! the shared hook lock, even when its data lives elsewhere. The locks are
//! advisory whole-file locks (`flock` on macOS and Linux, `LockFileEx` on
//! Windows), the same kind agent-hud-native-open's `InstanceLock` takes, so the two
//! apps see each other's. The system lets go of it when the process ends,
//! also when it crashes, so a copy that is gone never keeps another from
//! starting. Hook commands, the MCP relay and demos take no lock.
//!
//! The copy that holds the lock also listens beside it (`endpoint`), so a
//! second launch of the same app can ask it to come forward. Only the holder
//! ever listens there, so two launches never race for the endpoint.
use std::{
    fs::{File, TryLockError},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

// A Unix socket beside the lock, or a named pipe on Windows.
#[cfg(unix)]
#[path = "unix.rs"]
mod endpoint;
#[cfg(windows)]
#[path = "windows.rs"]
mod endpoint;

pub type OnShow = Arc<dyn Fn() + Send + Sync>;

pub enum Claim {
    /// This process holds the lock for as long as it keeps the value.
    Acquired(InstanceLock),
    /// Another process holds it: the executable it runs, if the lock names it.
    Held(Option<PathBuf>),
    /// No lock could be taken here; collection must not start.
    Unavailable(String),
}

/// A collector always owns its data; hooks are an additional shared resource.
pub struct CollectorLocks {
    data: InstanceLock,
    hooks: Option<InstanceLock>,
}

#[derive(Debug)]
pub enum CollectorError {
    Held {
        path: PathBuf,
        executable: Option<PathBuf>,
    },
    Unavailable(String),
}

impl std::fmt::Display for CollectorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Held { path, executable } => {
                write!(formatter, "Agent HUD already owns {}", path.display())?;
                if let Some(executable) = executable {
                    write!(formatter, " ({})", application(executable).display())?;
                }
                Ok(())
            }
            Self::Unavailable(error) => {
                write!(formatter, "Agent HUD could not take its lock: {error}")
            }
        }
    }
}

impl std::error::Error for CollectorError {}

/// The data owner and, when present, hook owner, with their hand-over endpoints.
pub struct Collecting {
    _data: Running,
    _hooks: Option<Running>,
}

impl CollectorLocks {
    pub fn listen(self, on_show: OnShow) -> Collecting {
        Collecting {
            _data: self.data.listen(Arc::clone(&on_show)),
            _hooks: self.hooks.map(|lock| lock.listen(on_show)),
        }
    }
}

/// Claim every resource before a Runtime opens its database. Failure to claim
/// hooks drops the already-acquired data lock, so another collector may use it.
pub fn claim_collector(
    data: &Path,
    hooks: Option<&Path>,
    executable: Option<&Path>,
) -> Result<CollectorLocks, CollectorError> {
    let take = |path: &Path| match claim(path, executable) {
        Claim::Acquired(lock) => Ok(lock),
        Claim::Held(executable) => Err(CollectorError::Held {
            path: path.to_owned(),
            executable,
        }),
        Claim::Unavailable(error) => Err(CollectorError::Unavailable(error)),
    };
    let data = take(data)?;
    let hooks = hooks.map(take).transpose()?;
    Ok(CollectorLocks { data, hooks })
}

pub struct InstanceLock {
    path: PathBuf,
    _file: File,
}

/// The lock of the copy that runs, and the endpoint where a second launch
/// asks it to come forward.
pub struct Running {
    _lock: InstanceLock,
    _endpoint: Option<endpoint::Endpoint>,
}

impl InstanceLock {
    /// Listens for a second launch's `hand_over` and calls `on_show` for each.
    /// Without an endpoint the lock still holds; a second launch then says
    /// which copy runs instead of bringing it forward.
    pub fn listen(self, on_show: OnShow) -> Running {
        let endpoint = endpoint::listen(&self.path, on_show)
            .inspect_err(|error| {
                eprintln!(
                    "No hand-over endpoint beside {}: {error}",
                    self.path.display()
                )
            })
            .ok();
        Running {
            _lock: self,
            _endpoint: endpoint,
        }
    }
}

/// Asks the copy that holds the lock at `path` to come forward; true when it
/// answered.
pub fn hand_over(path: &Path) -> bool {
    endpoint::hand_over(path)
}

/// Takes the lock at `path` and writes `executable` into it, so that a launch
/// it turns away can say which copy runs.
pub fn claim(path: &Path, executable: Option<&Path>) -> Claim {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        if let Err(error) = crate::os::private_directory(parent) {
            return Claim::Unavailable(format!("{}: {error}", parent.display()));
        }
    }
    let mut file = match crate::os::private_file(path, false) {
        Ok(file) => file,
        Err(error) => return Claim::Unavailable(format!("{}: {error}", path.display())),
    };
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Claim::Held(holder(path)),
        Err(TryLockError::Error(error)) => {
            return Claim::Unavailable(format!("{}: {error}", path.display()))
        }
    }
    let name = executable
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let written = file.set_len(0).is_ok()
        && file.seek(SeekFrom::Start(0)).is_ok()
        && file.write_all(name.as_bytes()).is_ok();
    if !written {
        eprintln!(
            "Could not name this copy in the instance lock {}",
            path.display()
        );
    }
    Claim::Acquired(InstanceLock {
        path: path.to_owned(),
        _file: file,
    })
}

/// The executable the holder of `path` wrote; None when the holder has not
/// written it yet, or the file cannot be read while it is locked (Windows).
pub fn holder(path: &Path) -> Option<PathBuf> {
    let mut text = String::new();
    File::open(path)
        .ok()?
        .take(4096)
        .read_to_string(&mut text)
        .ok()?;
    let executable = PathBuf::from(text.trim());
    executable.is_absolute().then_some(executable)
}

/// The application an executable belongs to, as a person would find it:
/// `…/Name.app` for one inside a macOS bundle, else the executable itself.
pub fn application(executable: &Path) -> PathBuf {
    let folder = executable.parent();
    let contents = folder.and_then(Path::parent);
    let bundle = contents.and_then(Path::parent);
    let in_bundle = folder
        .and_then(Path::file_name)
        .is_some_and(|name| name == "MacOS")
        && contents
            .and_then(Path::file_name)
            .is_some_and(|name| name == "Contents")
        && bundle
            .and_then(Path::extension)
            .is_some_and(|extension| extension == "app");
    match bundle {
        Some(bundle) if in_bundle => bundle.to_path_buf(),
        _ => executable.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_held_lock_turns_the_next_claim_away_and_names_the_holder() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("app.agenthud/instance.lock");
        let running = temp.path().join("Agent HUD.app/Contents/MacOS/Agent HUD");
        let next = temp.path().join("other/agent-hud");
        let Claim::Acquired(first) = claim(&path, Some(&running)) else {
            panic!("the first claim takes the lock");
        };
        match claim(&path, Some(&next)) {
            Claim::Held(Some(holder)) => assert_eq!(holder, running),
            // Windows keeps a locked file from being read by anyone else.
            Claim::Held(None) => {}
            _ => panic!("a second claim is turned away"),
        }
        drop(first);
        let Claim::Acquired(second) = claim(&path, Some(&next)) else {
            panic!("the lock is free once its holder lets go");
        };
        drop(second);
        assert_eq!(
            holder(&path).as_deref(),
            Some(next.as_path()),
            "the new holder replaces the old name"
        );
    }

    #[test]
    fn a_second_launch_brings_the_holder_forward() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("data/instance.lock");
        assert!(!hand_over(&path), "nobody answers before a copy runs");
        let Claim::Acquired(lock) = claim(&path, None) else {
            panic!("the first claim takes the lock");
        };
        let directory = path.parent().unwrap();
        assert!(crate::os::owner_only(
            directory,
            &fs::metadata(directory).unwrap()
        ));
        let shown = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = shown.clone();
        let _running = lock.listen(Arc::new(move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }));
        assert!(hand_over(&path));
        assert!(hand_over(&path));
        assert_eq!(shown.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn an_executable_inside_a_macos_bundle_names_the_application() {
        let tool = Path::new("Applications/Agent HUD.app/Contents/MacOS/Agent HUD");
        assert_eq!(application(tool), Path::new("Applications/Agent HUD.app"));
    }

    #[test]
    fn executables_outside_a_bundle_name_themselves() {
        let tool = Path::new("/usr/local/bin/agent-hud");
        assert_eq!(application(tool), tool);
        let loose = Path::new("/tmp/MacOS/agent-hud");
        assert_eq!(application(loose), loose);
    }

    #[test]
    fn every_collector_claims_its_data_with_or_without_hooks() {
        for first_hooks in [false, true] {
            for second_hooks in [false, true] {
                let temp = tempfile::tempdir().unwrap();
                let data = temp.path().join("data/instance.lock");
                let hooks = temp.path().join("hooks/instance.lock");
                let _first =
                    claim_collector(&data, first_hooks.then_some(hooks.as_path()), None).unwrap();
                assert!(matches!(
                    claim_collector(&data, second_hooks.then_some(hooks.as_path()), None),
                    Err(CollectorError::Held { path, .. }) if path == data
                ));
            }
        }
    }

    #[test]
    fn separate_data_collectors_coexist_but_share_one_hook_owner() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first/instance.lock");
        let second = temp.path().join("second/instance.lock");
        let hooks = temp.path().join("hooks/instance.lock");
        let _first = claim_collector(&first, Some(&hooks), None).unwrap();
        let second_owner = claim_collector(&second, None, None).unwrap();
        drop(second_owner);
        assert!(matches!(
            claim_collector(&second, Some(&hooks), None),
            Err(CollectorError::Held { path, .. }) if path == hooks
        ));
        // A failed hook claim must release the data lock it took first.
        let _second = claim_collector(&second, None, None).unwrap();
    }

    #[test]
    fn an_unavailable_resource_never_leaves_a_partial_owner() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data/instance.lock");
        let file = temp.path().join("file");
        fs::write(&file, "").unwrap();
        let unavailable = file.join("instance.lock");
        assert!(matches!(
            claim_collector(&unavailable, None, None),
            Err(CollectorError::Unavailable(_))
        ));
        assert!(matches!(
            claim_collector(&data, Some(&unavailable), None),
            Err(CollectorError::Unavailable(_))
        ));
        let _owner = claim_collector(&data, None, None).unwrap();
    }

    #[test]
    fn both_owned_resources_hand_over_to_the_same_app() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data/instance.lock");
        let hooks = temp.path().join("hooks/instance.lock");
        let owner = claim_collector(&data, Some(&hooks), None).unwrap();
        let shown = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = Arc::clone(&shown);
        let _running = owner.listen(Arc::new(move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }));
        assert!(hand_over(&data));
        assert!(hand_over(&hooks));
        assert_eq!(shown.load(std::sync::atomic::Ordering::SeqCst), 2);
    }
}
