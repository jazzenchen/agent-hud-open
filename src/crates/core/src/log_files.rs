//! LogFiles.swift's listing ownership: event paths update files/subtrees; a dropped
//! event, absent watcher, initial read or 300-second deadline reconciles every root.
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

/// How often every root is walked again, whatever the watcher said.
const RECONCILE_SECS: u64 = 300;

#[cfg(windows)]
#[path = "log_files/windows.rs"]
mod path_adapter;
#[cfg(not(windows))]
#[path = "log_files/unix.rs"]
mod path_adapter;
pub(super) use path_adapter::native_path;
mod source_paths;
mod stamp;
mod walk;
use source_paths::accepts;
pub(super) use source_paths::accepts_source;
pub(crate) use stamp::stamp;
use stamp::{stamp_cached, stamp_readonly};
use walk::{owned_path, walk, walk_root};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct FileStamp {
    pub source: &'static str,
    pub length: u64,
    pub modified: Option<SystemTime>,
}

pub(super) type ArchiveCache = HashMap<PathBuf, (u64, Option<SystemTime>, u64)>;

#[derive(Default)]
pub(super) struct LogFiles {
    pub files: HashMap<PathBuf, FileStamp>,
    archive_lengths: ArchiveCache,
    scanned_at: Option<Instant>,
    /// None means the current batch cannot establish which paths changed.
    changes: Option<HashSet<PathBuf>>,
    external_changes: bool,
    gaps: Vec<String>,
    holders: HashSet<PathBuf>,
    pub changed: HashSet<PathBuf>,
    pub removed: HashSet<PathBuf>,
    pub reconciled: bool,
}

impl LogFiles {
    pub fn note_changes(&mut self, paths: Option<HashSet<PathBuf>>) {
        if !self.external_changes {
            self.changes = paths;
            self.external_changes = true;
        } else if let (Some(pending), Some(paths)) = (&mut self.changes, paths) {
            pending.extend(paths);
        } else {
            self.changes = None;
        }
    }

    /// When the next full walk of every root is due: the reconcile that
    /// catches what the file watcher missed.
    pub fn next_reconcile(&self) -> Option<Instant> {
        self.scanned_at
            .map(|at| at + Duration::from_secs(RECONCILE_SECS))
    }

    /// A stat-only look at the logs a client holds open, whose writes can
    /// reach the watcher late: true when one of them no longer matches the
    /// listing. Reads nothing and changes nothing; the next pass rechecks
    /// them and reads what changed.
    pub fn stale(&self, roots: &[(PathBuf, &'static str)], active: &[PathBuf]) -> bool {
        active.iter().any(|path| {
            let Some((path, source)) = owned_path(path, roots) else {
                return false;
            };
            let metadata = if roots.iter().any(|(root, _)| root == &path) {
                fs::metadata(&path)
            } else {
                fs::symlink_metadata(&path)
            };
            match metadata {
                Ok(meta) if meta.is_file() && accepts(&path, roots) => {
                    self.files.get(&path)
                        != Some(&stamp_readonly(&path, source, &meta, &self.archive_lengths))
                }
                Ok(_) => self.files.contains_key(&path),
                Err(error) => {
                    error.kind() == std::io::ErrorKind::NotFound && self.files.contains_key(&path)
                }
            }
        })
    }

    pub fn refresh(
        &mut self,
        roots: &[(PathBuf, &'static str)],
        active: &[PathBuf],
    ) -> Vec<String> {
        self.changed.clear();
        self.removed.clear();
        self.reconciled = false;
        let changes = if self.external_changes {
            self.changes.take()
        } else {
            None
        };
        self.changes = Some(HashSet::new());
        if self
            .scanned_at
            .is_none_or(|last| last.elapsed() >= Duration::from_secs(RECONCILE_SECS))
            || changes.is_none()
        {
            self.reconciled = true;
            let mut files = HashMap::new();
            let mut errors = Vec::new();
            for (root, source) in roots {
                walk_root(
                    root,
                    source,
                    &mut files,
                    &mut self.archive_lengths,
                    &mut errors,
                );
            }
            // A failed subtree read cannot prove that cached files disappeared.
            if !errors.is_empty() {
                for (path, file) in &self.files {
                    files.entry(path.clone()).or_insert(*file);
                }
            }
            self.changed.extend(
                files
                    .iter()
                    .filter(|(path, file)| self.files.get(*path) != Some(*file))
                    .map(|(path, _)| path.clone()),
            );
            self.removed.extend(
                self.files
                    .keys()
                    .filter(|path| !files.contains_key(*path))
                    .cloned(),
            );
            self.holders.clear();
            for path in files.keys() {
                note_holders(&mut self.holders, path);
            }
            self.archive_lengths
                .retain(|path, _| files.contains_key(path));
            self.files = files;
            self.scanned_at = Some(Instant::now());
            self.gaps = errors;
            return self.gaps.clone();
        }
        let changed = changes
            .unwrap()
            .into_iter()
            .filter_map(|path| owned_path(&path, roots))
            .collect::<HashSet<_>>();
        let mut directories = changed
            .iter()
            .filter(|(path, _)| {
                fs::metadata(path)
                    .map(|metadata| metadata.is_dir())
                    .unwrap_or_else(|_| self.holders.contains(path))
            })
            .map(|(path, source)| (path.clone(), *source))
            .collect::<Vec<_>>();
        directories.sort_by_key(|(path, _)| path.components().count());
        let mut outer: Vec<(PathBuf, &'static str)> = Vec::new();
        for (path, source) in directories {
            if !outer.iter().any(|(parent, _)| path.starts_with(parent)) {
                outer.push((path, source));
            }
        }
        let mut errors = self.gaps.clone();
        for (path, source) in &outer {
            let mut files = HashMap::new();
            let mut subtree_errors = Vec::new();
            if roots.iter().any(|(root, _)| root == path) {
                walk_root(
                    path,
                    source,
                    &mut files,
                    &mut self.archive_lengths,
                    &mut subtree_errors,
                );
            } else {
                walk(
                    path,
                    source,
                    &mut files,
                    &mut self.archive_lengths,
                    &mut subtree_errors,
                );
            }
            if subtree_errors.is_empty() {
                self.removed.extend(
                    self.files
                        .keys()
                        .filter(|known| known.starts_with(path) && !files.contains_key(*known))
                        .cloned(),
                );
                self.files.retain(|known, _| !known.starts_with(path));
                self.archive_lengths
                    .retain(|known, _| !known.starts_with(path) || files.contains_key(known));
            }
            self.changed.extend(
                files
                    .iter()
                    .filter(|(path, file)| self.files.get(*path) != Some(*file))
                    .map(|(path, _)| path.clone()),
            );
            for path in files.keys() {
                note_holders(&mut self.holders, path);
            }
            self.files.extend(files);
            errors.extend(subtree_errors);
        }
        let mut recheck = changed
            .into_iter()
            .filter(|(path, _)| !outer.iter().any(|(parent, _)| path.starts_with(parent)))
            .collect::<HashMap<_, _>>();
        for path in active {
            if let Some((path, source)) = owned_path(path, roots) {
                recheck.insert(path, source);
            }
        }
        for (path, source) in recheck {
            let metadata = if roots.iter().any(|(root, _)| root == &path) {
                fs::metadata(&path)
            } else {
                fs::symlink_metadata(&path)
            };
            match metadata {
                Ok(meta) if meta.is_file() && accepts(&path, roots) => {
                    let next = stamp_cached(&path, source, &meta, &mut self.archive_lengths);
                    if self.files.get(&path) != Some(&next) {
                        self.changed.insert(path.clone());
                        note_holders(&mut self.holders, &path);
                        self.files.insert(path, next);
                    }
                }
                Ok(_) => {
                    self.archive_lengths.remove(&path);
                    if self.files.remove(&path).is_some() {
                        self.removed.insert(path);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.archive_lengths.remove(&path);
                    if self.files.remove(&path).is_some() {
                        self.removed.insert(path);
                    }
                }
                Err(error) => errors.push(format!("Could not inspect a {source} log: {error}")),
            }
        }
        errors
    }
}

fn note_holders(holders: &mut HashSet<PathBuf>, path: &Path) {
    let mut parent = path.parent();
    while let Some(directory) = parent {
        if !holders.insert(directory.into()) {
            break;
        }
        parent = directory.parent();
    }
}

#[cfg(test)]
mod tests;
