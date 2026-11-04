//! Native filesystem events only signal the serial collector, as FileChangeMonitor
//! does in the Swift app. No callback reads logs or mutates session state.
use agent_hud_core::SourceRoot;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

/// A watch that failed is tried again after this, doubling up to RETRY_MAX.
const RETRY_MIN: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(300);

type Waker = Arc<dyn Fn() + Send + Sync>;
/// Each watched folder, and whether the folders under it are watched too.
type Targets = HashMap<PathBuf, bool>;
pub struct FileChanges {
    watcher: Option<RecommendedWatcher>,
    roots: Vec<PathBuf>,
    wanted: Targets,
    watched: Targets,
    retry: Option<(Instant, Duration)>,
    changes: Arc<Mutex<Option<HashSet<PathBuf>>>>,
    names: Arc<Mutex<Vec<PathBuf>>>,
    waker: Arc<Mutex<Option<Waker>>>,
}
impl FileChanges {
    pub fn new(roots: &[SourceRoot]) -> Self {
        let changes: Arc<Mutex<Option<HashSet<PathBuf>>>> = Arc::new(Mutex::new(None));
        let waker: Arc<Mutex<Option<Waker>>> = Arc::new(Mutex::new(None));
        let names = Arc::new(Mutex::new(root_names(roots.iter().map(|root| &root.path))));
        let pending = Arc::clone(&changes);
        let notify_wake = Arc::clone(&waker);
        let source_names = Arc::clone(&names);
        let watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            if matches!(&event,Ok(event) if matches!(event.kind,EventKind::Access(_)) && !event.need_rescan()) {
                return;
            }
            // A file root is watched through its parent. In particular, the Codex
            // index must not wake the collector for every unrelated .codex write.
            let changed = match event {
                Ok(event) if !event.need_rescan() && !event.paths.is_empty() => {
                    let roots = source_names.lock().unwrap_or_else(PoisonError::into_inner);
                    let mut owned = HashSet::new();
                    for path in event.paths {
                        for root in roots.iter() {
                            if path.starts_with(root) { owned.insert(path.clone()); }
                            else if root.starts_with(&path) { owned.insert(root.clone()); }
                        }
                    }
                    if owned.is_empty() { return; }
                    Some(owned)
                }
                _ => None,
            };
            {
                let mut paths = pending.lock().unwrap_or_else(PoisonError::into_inner);
                match changed {
                    Some(changed) => {
                        if let Some(known) = paths.as_mut() {
                            known.extend(changed);
                            if known.len() > 10_000 {
                                *paths = None;
                            }
                        }
                    }
                    _ => *paths = None,
                }
            }
            let wake = notify_wake.lock().unwrap_or_else(PoisonError::into_inner).clone();
            if let Some(wake) = wake {
                wake();
            }
        })
        .ok();
        let mut this = Self {
            watcher,
            roots: roots.iter().map(|root| root.path.clone()).collect(),
            wanted: Targets::new(),
            watched: Targets::new(),
            retry: None,
            changes,
            names,
            waker,
        };
        this.update();
        this
    }
    pub fn set_waker(&mut self, wake: Waker) {
        *self.waker.lock().unwrap_or_else(PoisonError::into_inner) = Some(wake);
    }
    fn update(&mut self) {
        let wanted = targets(&self.roots);
        if wanted != self.wanted {
            *self.names.lock().unwrap_or_else(PoisonError::into_inner) =
                root_names(self.roots.iter());
            if let Some(watcher) = &mut self.watcher {
                for (path, recursive) in &self.watched {
                    if wanted.get(path) != Some(recursive) {
                        let _ = watcher.unwatch(path);
                    }
                }
            }
            self.watched
                .retain(|path, recursive| wanted.get(path) == Some(recursive));
            self.wanted = wanted;
            self.retry = None;
            // What changed while the watched set moved is unknown.
            *self.changes.lock().unwrap_or_else(PoisonError::into_inner) = None;
        } else if self.watched.len() == self.wanted.len()
            || self.retry.is_some_and(|(at, _)| Instant::now() < at)
        {
            return;
        }
        let Some(watcher) = &mut self.watcher else {
            return;
        };
        for (path, recursive) in &self.wanted {
            if self.watched.contains_key(path) {
                continue;
            }
            let mode = if *recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            if watcher.watch(path, mode).is_ok() {
                self.watched.insert(path.clone(), *recursive);
            } else {
                // A recursive watch can fail part of the way down.
                let _ = watcher.unwatch(path);
            }
        }
        self.retry = (self.watched.len() < self.wanted.len()).then(|| {
            let wait = self
                .retry
                .map_or(RETRY_MIN, |(_, wait)| (wait * 2).min(RETRY_MAX));
            (Instant::now() + wait, wait)
        });
    }
    pub fn consume(&mut self) -> Option<HashSet<PathBuf>> {
        self.update();
        if self.watcher.is_none() || self.wanted.is_empty() {
            return None;
        }
        let mut changed = self
            .changes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(HashSet::new())?;
        // A root no watch covers is read again on every pass, not every root.
        changed.extend(unwatched(&self.roots, &self.wanted, &self.watched));
        Some(changed)
    }
}

/// A folder root is watched with everything under it. A file root is watched
/// only through its own folder, so the Codex index does not watch all of .codex.
fn targets(roots: &[PathBuf]) -> Targets {
    let mut all = Targets::new();
    for root in roots.iter().filter(|path| path.exists()) {
        let (path, recursive) = if root.is_file() {
            (root.parent().unwrap_or(root), false)
        } else {
            (root.as_path(), true)
        };
        *all.entry(path.to_owned()).or_default() |= recursive;
    }
    let recursive = all
        .iter()
        .filter(|(_, recursive)| **recursive)
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    all.retain(|path, _| {
        !recursive
            .iter()
            .any(|parent| parent != path && path.starts_with(parent))
    });
    all
}

/// The roots that a watch which failed leaves unobserved.
fn unwatched(roots: &[PathBuf], wanted: &Targets, watched: &Targets) -> Vec<PathBuf> {
    let failed = wanted
        .iter()
        .filter(|(path, _)| !watched.contains_key(*path))
        .collect::<Vec<_>>();
    roots
        .iter()
        .filter(|root| {
            failed.iter().any(|(path, recursive)| {
                if **recursive {
                    root.starts_with(path)
                } else {
                    root.parent() == Some(path.as_path()) && root.is_file()
                }
            })
        })
        .cloned()
        .collect()
}

/// Each root under every name it has. A database root (Cursor's
/// `state.vscdb`) also changes through its write-ahead log beside it, which
/// the engine reads as the database (`log_files::owned_path`).
fn root_names<'a>(roots: impl Iterator<Item = &'a PathBuf>) -> Vec<PathBuf> {
    let mut names = HashSet::new();
    for root in roots {
        let mut both = vec![root.clone()];
        if let Ok(real) = root.canonicalize() {
            both.push(real);
        }
        for name in both {
            let mut wal = name.clone().into_os_string();
            wal.push("-wal");
            names.insert(PathBuf::from(wal));
            names.insert(name);
        }
    }
    names.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};

    #[test]
    fn a_database_root_is_watched_with_its_write_ahead_log() {
        let names = root_names([PathBuf::from("/nowhere/User/globalStorage/state.vscdb")].iter());
        assert!(names.contains(&PathBuf::from("/nowhere/User/globalStorage/state.vscdb")));
        assert!(names.contains(&PathBuf::from(
            "/nowhere/User/globalStorage/state.vscdb-wal"
        )));
        let wal = Path::new("/nowhere/User/globalStorage/state.vscdb-wal");
        assert!(names.iter().any(|root| wal.starts_with(root)));
        let other = Path::new("/nowhere/User/globalStorage/storage.json");
        assert!(!names.iter().any(|root| other.starts_with(root)));
    }

    /// A home with the Codex layout: an index file beside the session folders
    /// and folders the collector never reads.
    fn codex_home() -> (tempfile::TempDir, PathBuf, Vec<PathBuf>) {
        let home = tempfile::tempdir().unwrap();
        // macOS reports the real path of a temporary folder (/private/var/...).
        let codex = home.path().canonicalize().unwrap().join(".codex");
        fs::create_dir_all(codex.join("sessions/2026/10")).unwrap();
        fs::create_dir_all(codex.join(".tmp/plugins/a/b")).unwrap();
        fs::write(codex.join("session_index.jsonl"), "").unwrap();
        let roots = vec![
            codex.join("session_index.jsonl"),
            codex.join("sessions"),
            codex.join("archived_sessions"),
        ];
        (home, codex, roots)
    }

    #[test]
    fn a_file_root_watches_only_its_own_folder() {
        let (_home, codex, roots) = codex_home();
        assert_eq!(
            targets(&roots),
            Targets::from([(codex.clone(), false), (codex.join("sessions"), true)])
        );
        let sources = roots
            .iter()
            .map(|path| SourceRoot {
                path: path.clone(),
                provider: "codex",
            })
            .collect::<Vec<_>>();
        let mut changes = FileChanges::new(&sources);
        assert_eq!(changes.watched, changes.wanted);
        // The first pass cannot know what changed; later ones can.
        assert_eq!(changes.consume(), None);
        assert_eq!(changes.consume(), Some(HashSet::new()));
        // The folder watch still sees the index, and the session folder's own
        // watch sees a new session below it.
        let session = codex.join("sessions/2026/10/rollout.jsonl");
        fs::write(codex.join("session_index.jsonl"), "{}\n").unwrap();
        fs::write(&session, "{}\n").unwrap();
        fs::write(codex.join(".tmp/plugins/a/b/cache"), "").unwrap();
        let mut seen = HashSet::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline && !(seen.contains(&roots[0]) && seen.contains(&session)) {
            seen.extend(changes.consume().expect("the watch is working"));
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(seen.contains(&roots[0]), "{seen:?}");
        assert!(seen.contains(&session), "{seen:?}");
        assert!(!seen.iter().any(|path| path.starts_with(codex.join(".tmp"))));
    }

    #[test]
    fn a_recursive_root_covers_the_roots_inside_it() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("agent");
        fs::create_dir_all(root.join("sessions")).unwrap();
        fs::write(root.join("index.jsonl"), "").unwrap();
        let roots = vec![
            root.clone(),
            root.join("sessions"),
            root.join("index.jsonl"),
        ];
        assert_eq!(targets(&roots), Targets::from([(root, true)]));
    }

    #[test]
    fn a_failed_watch_rereads_only_the_roots_it_covers() {
        let (_home, codex, roots) = codex_home();
        let wanted = targets(&roots);
        let sessions = Targets::from([(codex.join("sessions"), true)]);
        assert_eq!(
            unwatched(&roots, &wanted, &sessions),
            vec![codex.join("session_index.jsonl")]
        );
        let index = Targets::from([(codex.clone(), false)]);
        assert_eq!(
            unwatched(&roots, &wanted, &index),
            vec![codex.join("sessions")]
        );
        assert!(unwatched(&roots, &wanted, &wanted).is_empty());
    }
}
