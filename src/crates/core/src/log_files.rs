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
mod walk;
use source_paths::accepts;
pub(super) use source_paths::accepts_source;
use walk::{owned_path, walk, walk_root};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct FileStamp {
    pub source: &'static str,
    pub length: u64,
    pub modified: Option<SystemTime>,
}

#[derive(Default)]
pub(super) struct LogFiles {
    pub files: HashMap<PathBuf, FileStamp>,
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
                    self.files.get(&path) != Some(&stamp(&path, source, &meta))
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
                walk_root(root, source, &mut files, &mut errors);
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
                walk_root(path, source, &mut files, &mut subtree_errors);
            } else {
                walk(path, source, &mut files, &mut subtree_errors);
            }
            if subtree_errors.is_empty() {
                self.removed.extend(
                    self.files
                        .keys()
                        .filter(|known| known.starts_with(path) && !files.contains_key(*known))
                        .cloned(),
                );
                self.files.retain(|known, _| !known.starts_with(path));
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
                    let next = stamp(&path, source, &meta);
                    if self.files.get(&path) != Some(&next) {
                        self.changed.insert(path.clone());
                        note_holders(&mut self.holders, &path);
                        self.files.insert(path, next);
                    }
                }
                Ok(_) => {
                    if self.files.remove(&path).is_some() {
                        self.removed.insert(path);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
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

pub(super) fn stamp(path: &Path, source: &'static str, metadata: &fs::Metadata) -> FileStamp {
    let related = if source == "grok-bot" {
        path.parent()
            .map(|dir| super::grok_cache::path(dir, super::grok_cache::ACCOUNT_KEY))
    } else if source == "antigravity" {
        path.parent().and_then(Path::parent).map(|dir| {
            dir.join("annotations")
                .join(path.file_stem().unwrap_or_default())
                .with_extension("pbtxt")
        })
    } else {
        agent_hud_agents::transcript::kept_names::kept_name_file(path, source)
    };
    let related_meta = related.and_then(|p| fs::metadata(p).ok());
    FileStamp {
        source,
        length: if super::source_db::accepts(path, source) {
            metadata.len().saturating_add(
                fs::metadata(format!("{}-wal", path.to_string_lossy()))
                    .map(|m| m.len())
                    .unwrap_or(0),
            )
        } else if source == "codex" && super::compressed::is_archive(path) {
            super::compressed::text_length(path).unwrap_or(metadata.len())
        } else {
            metadata.len()
        },
        modified: metadata
            .modified()
            .ok()
            .into_iter()
            .chain(if super::source_db::accepts(path, source) {
                fs::metadata(format!("{}-wal", path.to_string_lossy()))
                    .ok()
                    .and_then(|m| m.modified().ok())
            } else {
                None
            })
            .chain(related_meta.and_then(|m| m.modified().ok()))
            .max(),
    }
}

#[cfg(test)]
mod tests;
