//! The command line the Claude desktop app keeps off PATH:
//! `<data>\claude-code\<version>\<hash>\claude.exe`.
use super::{claude_desktop_bases, Environment};
use std::path::{Path, PathBuf};

/// Its copies, newest first in each of the desktop app's data folders.
pub(super) fn engines(home: &Path, env: &Environment) -> Vec<PathBuf> {
    claude_desktop_bases(home, env)
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

    #[test]
    fn the_folder_the_app_moved_its_data_to_is_one_of_its_data_folders() {
        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("Local");
        let roaming = temp.path().join("Roaming");
        let env = Environment::from([
            ("LOCALAPPDATA".into(), local.to_string_lossy().into_owned()),
            ("APPDATA".into(), roaming.to_string_lossy().into_owned()),
        ]);
        let left = roaming.join("Claude");
        let moved = local.join("Claude-Data/claude_pzs8sxrjxfjjc");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&moved).unwrap();
        std::fs::write(left.join(".user-data-left.claude_pzs8sxrjxfjjc"), b"").unwrap();
        std::fs::write(moved.join("config.json"), b"{}").unwrap();
        let bases = claude_desktop_bases(temp.path(), &env);
        assert!(bases.contains(&left) && bases.contains(&moved), "{bases:?}");
    }
}
