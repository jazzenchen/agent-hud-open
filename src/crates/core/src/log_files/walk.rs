//! Walking a source root for the logs it holds, and naming a reported path
//! the way its configured root does.
use super::source_paths::{accepts_source, hides_files};
use super::{stamp, FileStamp};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

pub(super) fn walk_root(
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

pub(super) fn walk(
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
    if let Some(store) = agent_hud_agents::opencode::store(source) {
        if !path.join(store.database).is_file()
            && !(store.legacy
                && (name == "storage" || path.components().any(|c| c.as_os_str() == "message")))
        {
            return;
        }
    }
    if matches!(source, "zcode" | "devin" | "goose")
        && !path
            .join(match source {
                "zcode" => "db.sqlite",
                "devin" | "goose" => "sessions.db",
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
    // Hidden files and folders are never listed where `hides_files` says so;
    // other readers keep their own rules.
    let skips = hides_files(source);
    for entry in entries {
        match entry {
            Ok(entry) if skips && entry.file_name().to_string_lossy().starts_with('.') => {}
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
pub(super) fn owned_path(
    path: &Path,
    roots: &[(PathBuf, &'static str)],
) -> Option<(PathBuf, &'static str)> {
    if let Some(raw) = path
        .to_str()
        .and_then(|s| s.strip_suffix("-wal").or_else(|| s.strip_suffix("-shm")))
    {
        let original = Path::new(raw);
        if let Some((root, source)) = roots.iter().find(|(root, source)| {
            original.starts_with(root) && crate::source_db::accepts(original, source)
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
                && path == crate::grok_cache::path(root, crate::grok_cache::ACCOUNT_KEY)
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
