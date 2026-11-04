//! A session's project (Open's SessionProject): the Git repository its
//! folder is in, which its worktrees share, or the folder itself outside Git,
//! written with `~` as Open writes the id. Git's own pointers decide (`.git`,
//! `gitdir:`, `commondir`), never a folder's name or a worktree's location.
use std::{
    cmp::Ordering,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock, PoisonError},
};

/// The id of the sessions of no folder.
pub const UNASSIGNED: &str = "unassigned";

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// The project of a session's folder, by its id; none for a session of no folder.
pub fn project_id(cwd: Option<&str>) -> Option<String> {
    let cwd = cwd.filter(|cwd| !cwd.is_empty())?;
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(id) = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(cwd)
        .cloned()
    {
        return Some(id);
    }
    let id = abbreviate(&repository(&expand(cwd)));
    cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(cwd.to_owned(), id.clone());
    Some(id)
}

/// A filter's entry as a project id: `unassigned` as it is, a folder (with
/// `~` or without) as the project it is in, so a layout written with raw
/// folders still matches.
pub fn normalize(entry: &str) -> String {
    if entry == UNASSIGNED {
        return entry.into();
    }
    project_id(Some(entry)).unwrap_or_else(|| UNASSIGNED.into())
}

/// Projects as Finder orders their ids, the sessions of no folder last.
pub fn compare(a: &Option<String>, b: &Option<String>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, _) => Ordering::Greater,
        (_, None) => Ordering::Less,
        (Some(a), Some(b)) => natural(a, b),
    }
}

fn expand(path: &str) -> PathBuf {
    match (path.strip_prefix('~'), home()) {
        (Some(rest), Some(home))
            if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') =>
        {
            home.join(rest.trim_start_matches(['/', '\\']))
        }
        _ => PathBuf::from(path),
    }
}

fn abbreviate(path: &Path) -> String {
    if let Some(home) = home() {
        if let Ok(rest) = path.strip_prefix(&home) {
            let rest = rest.to_string_lossy();
            return if rest.is_empty() {
                "~".into()
            } else {
                format!("~/{rest}")
            };
        }
    }
    path.to_string_lossy().into_owned()
}

/// The repository a folder is in, through its worktree's `commondir`; a
/// submodule or a separate Git directory, which have none, stays its own
/// project; a folder outside Git is its own.
fn repository(path: &Path) -> PathBuf {
    let start = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut directory = start.clone();
    loop {
        let marker = directory.join(".git");
        if marker.is_dir() {
            return directory;
        }
        if marker.is_file() {
            let Some(pointer) = std::fs::read_to_string(&marker).ok().and_then(|text| {
                text.trim()
                    .strip_prefix("gitdir:")
                    .map(|rest| rest.trim().to_owned())
            }) else {
                return start;
            };
            let git = resolve(&pointer, &directory);
            let Ok(common) = std::fs::read_to_string(git.join("commondir")) else {
                return directory;
            };
            let common = resolve(common.trim(), &git);
            return if common.file_name().is_some_and(|name| name == ".git") {
                common.parent().map(Path::to_path_buf).unwrap_or(common)
            } else {
                common
            };
        }
        match directory.parent() {
            Some(parent) if parent != directory => directory = parent.to_path_buf(),
            _ => return start,
        }
    }
}

fn resolve(pointer: &str, relative_to: &Path) -> PathBuf {
    let path = relative_to.join(pointer);
    std::fs::canonicalize(&path).unwrap_or(path)
}

/// Finder's order of names: case-insensitive, runs of digits by their value.
fn natural(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let digits = |chars: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut run = String::new();
                    while let Some(c) = chars.peek().copied().filter(char::is_ascii_digit) {
                        run.push(c);
                        chars.next();
                    }
                    run
                };
                let (x, y) = (digits(&mut a), digits(&mut b));
                let (tx, ty) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let order = tx.len().cmp(&ty.len()).then_with(|| tx.cmp(ty));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.to_lowercase().cmp(y.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                a.next();
                b.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktrees_share_their_repository_and_projects_sort_as_finder_does() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("app");
        std::fs::create_dir_all(repo.join(".git/worktrees/feature")).unwrap();
        std::fs::create_dir_all(repo.join("src/deep")).unwrap();
        let tree = root.path().join("app-feature");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(
            tree.join(".git"),
            format!(
                "gitdir: {}\n",
                repo.join(".git/worktrees/feature").display()
            ),
        )
        .unwrap();
        std::fs::write(repo.join(".git/worktrees/feature/commondir"), "../..\n").unwrap();
        let module = root.path().join("module");
        std::fs::create_dir_all(root.path().join("separate-git")).unwrap();
        std::fs::create_dir_all(&module).unwrap();
        std::fs::write(module.join(".git"), "gitdir: ../separate-git\n").unwrap();
        let id = |path: &Path| project_id(Some(path.to_str().unwrap())).unwrap();
        let canonical = |path: &Path| abbreviate(&std::fs::canonicalize(path).unwrap());
        assert_eq!(id(&repo.join("src/deep")), canonical(&repo));
        assert_eq!(id(&tree), canonical(&repo));
        assert_eq!(id(&module), canonical(&module));
        assert_eq!(
            id(&root.path().join("app-feature")),
            normalize(&canonical(&tree))
        );
        assert_eq!(project_id(None), None);
        let mut ids = vec![
            None,
            Some("~/b10".to_owned()),
            Some("~/B2".to_owned()),
            Some("~/a".to_owned()),
        ];
        ids.sort_by(compare);
        assert_eq!(
            ids,
            [
                Some("~/a".to_owned()),
                Some("~/B2".to_owned()),
                Some("~/b10".to_owned()),
                None
            ]
        );
    }
}
