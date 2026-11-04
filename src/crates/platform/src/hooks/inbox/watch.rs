//! The inbox as the app watches it: change notices name the records to read
//! again, and each drain reads them, one after another.
use super::super::{now_ms, Wake};
use super::{is_record, prepare, read, valid, ACCOUNT_LOCK};
use agent_hud_agents::HookEvent;
use notify::{
    event::{CreateKind, ModifyKind, RemoveKind},
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

pub(crate) struct Inbox {
    directory: PathBuf,
    // None requests an initial/full scan; Some(empty) is a quiet inbox.
    pending: Arc<Mutex<Option<HashSet<PathBuf>>>>,
    seen: HashMap<PathBuf, Vec<u8>>,
    _watcher: RecommendedWatcher,
}
impl Inbox {
    pub fn new(data: &Path, wake: Wake) -> Result<Self, String> {
        prepare(&data.join("hooks"))?;
        let directory = data.join("hooks/inbox");
        prepare(&directory)?;
        let directory = directory
            .canonicalize()
            .map_err(|_| "Hook inbox unavailable")?;
        let pending = Arc::new(Mutex::new(None::<HashSet<PathBuf>>));
        let changes = pending.clone();
        let watched = directory.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Some(change) = changed_paths(&watched, event) else {
                    return;
                };
                {
                    let mut pending = changes.lock().unwrap_or_else(PoisonError::into_inner);
                    match change {
                        Change::Full => *pending = None,
                        Change::Paths(paths) => {
                            if let Some(known) = pending.as_mut() {
                                known.extend(paths);
                            }
                        }
                    }
                }
                wake();
            })
            .map_err(|_| "Hook inbox watcher unavailable")?;
        watcher
            .watch(&directory, RecursiveMode::NonRecursive)
            .map_err(|_| "Hook inbox watcher unavailable")?;
        Ok(Self {
            directory,
            pending,
            seen: HashMap::new(),
            _watcher: watcher,
        })
    }
    pub fn drain(&mut self) -> Vec<HookEvent> {
        let changed = {
            let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
            (*pending).replace(HashSet::new())
        };
        if changed.as_ref().is_some_and(HashSet::is_empty) {
            return Vec::new();
        }
        let mut events = Vec::new();
        let seen = &mut self.seen;
        let now = now_ms();
        let full = changed.is_none();
        let paths = if let Some(paths) = changed {
            paths
        } else {
            let Ok(entries) = fs::read_dir(&self.directory) else {
                *self.pending.lock().unwrap_or_else(PoisonError::into_inner) = None;
                return events;
            };
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| is_record(path))
                .collect::<HashSet<_>>()
        };
        if full {
            seen.retain(|path, _| paths.contains(path));
        }
        for path in paths {
            if let Some((event, bytes)) = read(&path) {
                if !valid(&event, now) {
                    let _ = fs::remove_file(&path);
                    seen.remove(&path);
                    continue;
                }
                let digest = Sha256::digest(bytes).to_vec();
                if seen.get(&path) != Some(&digest) {
                    seen.insert(path, digest);
                    events.push(event);
                }
            } else {
                seen.remove(&path);
            }
        }
        events.sort_by_key(|event| event.at_ms);
        events
    }
}

pub(super) enum Change {
    Full,
    Paths(HashSet<PathBuf>),
}
fn temporary(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("tmp")
}
/// Notifications only identify paths; all reads remain in the serial drain.
/// FSEvents may coalesce to a directory or request a rescan. Those cases must
/// keep the full-scan path instead of assuming a missing filename means quiet.
pub(super) fn changed_paths(directory: &Path, event: notify::Result<Event>) -> Option<Change> {
    let Ok(event) = event else {
        return Some(Change::Full);
    };
    if event.need_rescan() {
        return Some(Change::Full);
    }
    if matches!(event.kind, EventKind::Access(_)) {
        return None;
    }
    if event.paths.is_empty() {
        return Some(Change::Full);
    }
    let mut paths = HashSet::new();
    for path in event.paths {
        if path == directory || directory.starts_with(&path) {
            return Some(Change::Full);
        }
        if !path.starts_with(directory) {
            continue;
        }
        if path.file_name().is_some_and(|name| name == ACCOUNT_LOCK) {
            continue;
        }
        if matches!(
            event.kind,
            EventKind::Create(CreateKind::Folder) | EventKind::Remove(RemoveKind::Folder)
        ) {
            return Some(Change::Full);
        }
        if path.parent() != Some(directory) {
            continue;
        }
        if is_record(&path) {
            paths.insert(path);
        } else if !temporary(&path)
            && matches!(
                event.kind,
                EventKind::Any | EventKind::Other | EventKind::Modify(ModifyKind::Name(_))
            )
        {
            return Some(Change::Full);
        }
    }
    (!paths.is_empty()).then_some(Change::Paths(paths))
}
