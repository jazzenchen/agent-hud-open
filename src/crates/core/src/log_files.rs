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
#[path = "log_files_windows.rs"]
mod path_adapter;
#[cfg(not(windows))]
#[path = "log_files_unix.rs"]
mod path_adapter;
pub(super) use path_adapter::native_path;

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

fn walk_root(
    path: &Path,
    source: &'static str,
    files: &mut HashMap<PathBuf, FileStamp>,
    errors: &mut Vec<String>,
) {
    if fs::symlink_metadata(path).is_ok_and(|meta| meta.is_symlink()) {
        if let Ok(real) = path.canonicalize() {
            let mut resolved = HashMap::new();
            walk(&real, source, &mut resolved, errors);
            for (file, stamp) in resolved {
                if let Ok(tail) = file.strip_prefix(&real) {
                    files.insert(path.join(tail), stamp);
                }
            }
        } else {
            errors.push(format!("Could not resolve a {source} log root"));
        }
    } else {
        walk(path, source, files, errors);
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
fn accepts(path: &Path, roots: &[(PathBuf, &'static str)]) -> bool {
    roots
        .iter()
        .find(|(root, _)| path.starts_with(root))
        .is_some_and(|(root, source)| {
            accepts_source(path, source)
                && !(matches!(*source, "claude" | "codex") && hidden_below(path, root))
        })
}
/// Whether a file or a folder it lies in, below its root, is hidden.
fn hidden_below(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root).is_ok_and(|rest| {
        rest.components()
            .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
    })
}
pub(super) fn accepts_source(path: &Path, source: &str) -> bool {
    if super::source_db::accepts(path, source) {
        return true;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    match source {
        // A session, or a subagent's transcript. A subagents folder also holds the
        // Workflow tool's journal.jsonl, a run log with no turns: read as a session it
        // would never report an event and leave the Claude mark faded.
        "claude" => {
            name.ends_with(".jsonl")
                && (name.starts_with("agent-")
                    || !path.components().any(|p| p.as_os_str() == "subagents"))
        }
        // A rollout, plain or compressed (agent-hud-native-open CodexRollouts.accepts).
        "codex" => {
            name.starts_with("rollout-")
                && (name.ends_with(".jsonl") || name.ends_with(".jsonl.zst"))
        }
        "deepseek" => {
            let plain = name.strip_suffix(".zstd").unwrap_or(&name);
            plain == "session.jsonl"
                || (plain.starts_with("session.v")
                    && plain.ends_with(".jsonl")
                    && plain[9..plain.len() - 6]
                        .chars()
                        .all(|c| c.is_ascii_digit()))
        }
        "qwen" => {
            name.ends_with(".jsonl")
                && !name.ends_with(".ledger.jsonl")
                && path.components().any(|p| p.as_os_str() == "chats")
                && !path.components().any(|p| p.as_os_str() == "subagents")
        }
        "codebuddy" | "workbuddy" => {
            name.ends_with(".jsonl") && !path.components().any(|p| p.as_os_str() == "tool-results")
        }
        "kimi" => name == "wire.jsonl",
        "openclaw" => {
            path.parent()
                .and_then(Path::file_name)
                .is_some_and(|s| s == "sessions")
                && path
                    .parent()
                    .and_then(Path::parent)
                    .and_then(Path::parent)
                    .and_then(Path::file_name)
                    .is_some_and(|s| s == "agents")
                && (name.ends_with(".jsonl")
                    || ((name.contains(".jsonl.reset.") || name.contains(".jsonl.deleted."))
                        && !name.ends_with(".zst")
                        && !name.ends_with(".tmp")))
        }
        "hermes" | "zcode" | "devin" | "opencode" | "grok-bot" | "cursor" => false,
        "antigravity" => {
            name == "transcript.jsonl" && path.components().any(|p| p.as_os_str() == "brain")
        }
        "grok-cli" => name == "updates.jsonl" || name == "unified.jsonl",
        "copilot" => name == "events.jsonl",
        _ => name.ends_with(".jsonl"),
    }
}
fn walk(
    path: &Path,
    source: &'static str,
    files: &mut HashMap<PathBuf, FileStamp>,
    errors: &mut Vec<String>,
) {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            errors.push(format!("Could not inspect {source} logs: {error}"));
            return;
        }
    };
    if meta.is_symlink() {
        return;
    }
    if meta.is_file() {
        if accepts_source(path, source) {
            files.insert(path.into(), stamp(path, source, &meta));
        }
        return;
    }
    let name = path.file_name().unwrap_or_default();
    if source == "grok-bot"
        && path.parent().is_some_and(|p| {
            p.file_name()
                .is_some_and(|s| s == "sand-client-persistence")
        })
    {
        return;
    }
    if (source == "qwen" && name == "subagents")
        || (matches!(source, "codebuddy" | "workbuddy") && name == "tool-results")
    {
        return;
    }
    if source == "hermes"
        && !(path.join("state.db").is_file()
            || path.join("profiles").is_dir()
            || name == "profiles"
            || path
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == "profiles"))
    {
        return;
    }
    if source == "opencode"
        && !path.join("opencode.db").is_file()
        && name != "storage"
        && !path.components().any(|c| c.as_os_str() == "message")
    {
        return;
    }
    if matches!(source, "zcode" | "devin")
        && !path
            .join(match source {
                "zcode" => "db.sqlite",
                "devin" => "sessions.db",
                _ => "opencode.db",
            })
            .is_file()
    {
        return;
    }
    if source == "antigravity" {
        walk_antigravity(path, source, files);
        return;
    }
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(format!("Could not list {source} logs: {error}"));
            return;
        }
    };
    for entry in entries {
        match entry {
            // Claude Code's and Codex's hidden files and folders are never
            // listed (agent-hud-native-open LogFiles); other readers keep their own
            // rules.
            Ok(entry)
                if matches!(source, "claude" | "codex")
                    && entry.file_name().to_string_lossy().starts_with('.') => {}
            Ok(entry) => walk(&entry.path(), source, files, errors),
            Err(error) => errors.push(format!("Could not list {source} logs: {error}")),
        }
    }
}

fn walk_antigravity(path: &Path, source: &'static str, files: &mut HashMap<PathBuf, FileStamp>) {
    fn insert_if_accepted(
        candidate: PathBuf,
        source: &'static str,
        files: &mut HashMap<PathBuf, FileStamp>,
    ) {
        if let Ok(meta) = fs::symlink_metadata(&candidate) {
            if !meta.is_symlink() && meta.is_file() && accepts_source(&candidate, source) {
                files.insert(candidate.clone(), stamp(&candidate, source, &meta));
            }
        }
    }
    fn scan_flat(dir: &Path, source: &'static str, files: &mut HashMap<PathBuf, FileStamp>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                insert_if_accepted(entry.path(), source, files);
            }
        }
    }
    fn scan_brain(brain_dir: &Path, source: &'static str, files: &mut HashMap<PathBuf, FileStamp>) {
        if let Ok(entries) = fs::read_dir(brain_dir) {
            for entry in entries.flatten() {
                insert_if_accepted(
                    entry.path().join(".system_generated/logs/transcript.jsonl"),
                    source,
                    files,
                );
            }
        }
    }
    scan_flat(path, source, files);
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if name == "brain" {
        scan_brain(path, source, files);
        return;
    }
    if path.components().any(|c| c.as_os_str() == "brain") {
        for rel in [
            ".system_generated/logs/transcript.jsonl",
            "logs/transcript.jsonl",
            "transcript.jsonl",
        ] {
            insert_if_accepted(path.join(rel), source, files);
        }
        return;
    }
    if name != "conversations" {
        let conv = path.join("conversations");
        if conv.is_dir() {
            scan_flat(&conv, source, files);
        }
        let brain = path.join("brain");
        if brain.is_dir() {
            scan_brain(&brain, source, files);
        }
    }
}

/// Native watches can report canonical paths while configured roots contain a symlink.
/// Keep one configured name as the cache key, including after a reported file vanished.
fn owned_path(path: &Path, roots: &[(PathBuf, &'static str)]) -> Option<(PathBuf, &'static str)> {
    if let Some(raw) = path
        .to_str()
        .and_then(|s| s.strip_suffix("-wal").or_else(|| s.strip_suffix("-shm")))
    {
        let original = Path::new(raw);
        if let Some((root, source)) = roots.iter().find(|(root, source)| {
            original.starts_with(root) && super::source_db::accepts(original, source)
        }) {
            return Some((
                if root == original {
                    root.clone()
                } else {
                    original.into()
                },
                *source,
            ));
        }
    }
    for (root, source) in roots {
        if path == root || path.starts_with(root) {
            if *source == "grok-bot"
                && path == super::grok_cache::path(root, super::grok_cache::ACCOUNT_KEY)
            {
                return Some((root.clone(), *source));
            }
            return Some((path.into(), *source));
        }
        let canonical = real_name(root);
        if path == canonical {
            return Some((root.clone(), *source));
        }
        if let Ok(tail) = path.strip_prefix(&canonical) {
            return Some((root.join(tail), *source));
        }
    }
    None
}
fn real_name(path: &Path) -> PathBuf {
    if let Ok(real) = path.canonicalize() {
        return real;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => real_name(parent).join(name),
        _ => path.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stat-only look between passes reads no file and changes nothing:
    /// it says whether a held log no longer matches the listing.
    #[test]
    fn a_stale_look_sees_a_grown_or_gone_log_and_leaves_the_listing_alone() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".claude/projects");
        let path = root.join("p/s.jsonl");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{}\n").unwrap();
        let roots = vec![(root, "claude")];
        let mut files = LogFiles::default();
        files.refresh(&roots, &[]);
        let active = vec![path.clone()];
        assert!(!files.stale(&roots, &active));
        assert!(files.next_reconcile().is_some());
        fs::write(&path, "{}\n{}\n").unwrap();
        assert!(files.stale(&roots, &active));
        assert!(files.stale(&roots, &active), "the look changed nothing");
        files.refresh(&roots, &active);
        assert!(files.changed.contains(&path));
        assert!(!files.stale(&roots, &active));
        fs::remove_file(&path).unwrap();
        assert!(files.stale(&roots, &active));
        assert!(
            !files.stale(&roots, &[]),
            "only the held logs are looked at"
        );
    }

    #[test]
    fn claude_and_codex_list_no_hidden_files_and_codex_only_rollouts() {
        let roots = vec![
            (PathBuf::from("/h/.claude/projects"), "claude"),
            (PathBuf::from("/h/.codex/sessions"), "codex"),
        ];
        assert!(accepts(Path::new("/h/.claude/projects/p/s.jsonl"), &roots));
        assert!(!accepts(
            Path::new("/h/.claude/projects/p/.s.jsonl"),
            &roots
        ));
        assert!(!accepts(
            Path::new("/h/.claude/projects/.trash/s.jsonl"),
            &roots
        ));
        assert!(accepts(
            Path::new("/h/.codex/sessions/2026/10/21/rollout-a.jsonl"),
            &roots
        ));
        assert!(accepts(
            Path::new("/h/.codex/sessions/2026/10/21/rollout-a.jsonl.zst"),
            &roots
        ));
        assert!(!accepts(
            Path::new("/h/.codex/sessions/2026/10/21/notes.jsonl"),
            &roots
        ));
    }

    #[test]
    fn claude_lists_sessions_and_subagents_but_not_workflow_journals() {
        let roots = vec![(PathBuf::from("/h/.claude/projects"), "claude")];
        let session = "/h/.claude/projects/p/a13ae0b7-3ba4-4e35-af3f-80f8fb300c67";
        assert!(accepts(Path::new(&format!("{session}.jsonl")), &roots));
        assert!(accepts(
            Path::new(&format!("{session}/subagents/agent-a1.jsonl")),
            &roots
        ));
        assert!(accepts(
            Path::new(&format!(
                "{session}/subagents/workflows/wf_93f53f2f-2ce/agent-a2.jsonl"
            )),
            &roots
        ));
        assert!(!accepts(
            Path::new(&format!(
                "{session}/subagents/workflows/wf_93f53f2f-2ce/journal.jsonl"
            )),
            &roots
        ));
    }
}
