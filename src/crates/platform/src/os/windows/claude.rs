//! The command line the Claude desktop app keeps off PATH:
//! `<data>\claude-code\<version>\<hash>\claude.exe`.
use super::{claude_desktop_bases, local_app, Environment};
use std::path::{Path, PathBuf};

/// Its copies, newest first in each data folder: %APPDATA%\Claude, the Store
/// package's redirected copy of it, and %LOCALAPPDATA%\Claude-Data, which the
/// MSIX package keeps out of that redirection, at most one folder down
/// (agent-hud-open#74).
pub(super) fn engines(home: &Path, env: &Environment) -> Vec<PathBuf> {
    let data = local_app(home, env).join("Claude-Data");
    let mut roots = claude_desktop_bases(home, env);
    roots.extend(subfolders(&data));
    roots.push(data);
    roots
        .iter()
        .flat_map(|root| newest_nested(&root.join("claude-code"), "claude.exe"))
        .collect()
}

fn subfolders(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect()
}

/// `<root>\*\*\<file>`, most recently modified first.
fn newest_nested(root: &Path, file: &str) -> Vec<PathBuf> {
    let mut found: Vec<_> = subfolders(root)
        .iter()
        .flat_map(|version| subfolders(version))
        .map(|dir| dir.join(file))
        .filter(|path| path.is_file())
        .map(|path| {
            let modified = std::fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            (modified, path)
        })
        .collect();
    found.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    found.into_iter().map(|(_, path)| path).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_msix_apps_copy_under_claude_data_is_found() {
        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("Local");
        let env = Environment::from([
            ("LOCALAPPDATA".into(), local.to_string_lossy().into_owned()),
            (
                "APPDATA".into(),
                temp.path().join("Roaming").to_string_lossy().into_owned(),
            ),
        ]);
        let direct = local.join("Claude-Data/claude-code/2.1.321/a1/claude.exe");
        let nested = local.join("Claude-Data/profile/claude-code/2.1.320/b2/claude.exe");
        for engine in [&direct, &nested] {
            std::fs::create_dir_all(engine.parent().unwrap()).unwrap();
            std::fs::write(engine, b"MZ").unwrap();
        }
        let found = engines(temp.path(), &env);
        assert!(
            found.contains(&direct) && found.contains(&nested),
            "{found:?}"
        );
    }
}
