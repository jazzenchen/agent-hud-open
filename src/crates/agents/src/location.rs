//! Where a client keeps its files, as data the platform resolves per OS:
//! its session logs and databases (`Source`), its hook settings, its caches.
use crate::{PerOs, Support};
use std::path::{Path, PathBuf};

/// A folder the platform resolves on each operating system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    /// The user's home folder.
    Home,
    /// Application data: Application Support on macOS, the roaming AppData
    /// folder on Windows, the XDG config folder on Linux.
    AppData,
    /// The XDG data folder, `XDG_DATA_HOME` or `~/.local/share`, which the
    /// command-line clients use on every OS.
    DataHome,
    /// The XDG config folder, `XDG_CONFIG_HOME` or `~/.config`, on every OS.
    ConfigHome,
}

/// The few questions resolving a location asks of the file system.
pub trait Places {
    /// Whether something is at `path`.
    fn exists(&self, path: &Path) -> bool;
    /// Whether two paths name the same thing, links resolved.
    fn same(&self, a: &Path, b: &Path) -> bool;
    /// The names in a folder.
    fn names(&self, folder: &Path) -> Vec<String>;
}

/// A client's folder or file: `folder/path` under `base`, unless one of the
/// `env` variables names the folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location {
    pub base: Base,
    /// Variables that name the folder instead, the first one set winning. A
    /// value is used as written, without the whitespace around it, as the
    /// clients read it; an empty one is ignored.
    pub env: &'static [&'static str],
    /// The folder's names under the base, the first that exists winning and
    /// the last when none does. Empty when only a variable names it.
    pub folders: &'static [&'static str],
    /// The client's profiles, each a folder of its own that holds the same
    /// path: also every profile's.
    pub profiles: Option<Profiles>,
    /// The file or folder inside the folder; empty for the folder itself.
    pub path: &'static str,
    /// A folder under the base that a variable may name for another client:
    /// the variable is then ignored, as CodeBuddy's is when it names
    /// WorkBuddy's folder.
    pub not_if: Option<&'static str>,
}

/// Where a client keeps its profiles' folders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profiles {
    /// Beside the folder: every folder under the base whose name is this
    /// prefix and a profile name (OpenClaw's `~/.openclaw-<name>`), unless a
    /// variable names the folder.
    Beside(&'static str),
    /// Inside the folder: every folder in this folder of it whose name is a
    /// profile name and that holds the path (Hermes's
    /// `~/.hermes/profiles/<name>/config.yaml`), whichever folder a variable
    /// names. A profile that lacks the path is left out, so nothing is
    /// made in a profile the client has not set up.
    Inside(&'static str),
}

const fn plain(
    base: Base,
    env: &'static [&'static str],
    folders: &'static [&'static str],
    path: &'static str,
) -> Location {
    Location {
        base,
        env,
        folders,
        profiles: None,
        path,
        not_if: None,
    }
}

impl Location {
    /// `folder/path` under the home folder, or under the folder the first set
    /// variable of `env` names.
    pub const fn home(
        env: &'static [&'static str],
        folder: &'static [&'static str],
        path: &'static str,
    ) -> Self {
        plain(Base::Home, env, folder, path)
    }

    /// `folder/path` under the application data folder.
    pub const fn app_data(folder: &'static [&'static str], path: &'static str) -> Self {
        plain(Base::AppData, &[], folder, path)
    }

    /// `folder/path` under the XDG data folder.
    pub const fn data_home(folder: &'static [&'static str], path: &'static str) -> Self {
        plain(Base::DataHome, &[], folder, path)
    }

    /// `folder/path` under the XDG config folder.
    pub const fn config_home(folder: &'static [&'static str], path: &'static str) -> Self {
        plain(Base::ConfigHome, &[], folder, path)
    }

    /// The path where only the first set variable of `env` names the folder.
    pub const fn named_by(env: &'static [&'static str], path: &'static str) -> Self {
        plain(Base::Home, env, &[], path)
    }

    /// The default path relative to the base, `folder/path`.
    pub fn relative(&self) -> String {
        let folder = self.folders.first().copied().unwrap_or("");
        match (folder.is_empty(), self.path.is_empty()) {
            (true, _) => self.path.into(),
            (false, true) => folder.into(),
            (false, false) => format!("{folder}/{}", self.path),
        }
    }

    /// Every path this location names on this machine, given its base
    /// folder, the environment and the file system.
    pub fn resolve(
        &self,
        base: &Path,
        env: &dyn Fn(&str) -> Option<String>,
        places: &dyn Places,
    ) -> Vec<PathBuf> {
        let named = self
            .env
            .iter()
            .find_map(|key| {
                env(key)
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty())
            })
            .map(PathBuf::from)
            .filter(|folder| {
                !self.not_if.is_some_and(|other| {
                    let other = base.join(other);
                    *folder == other || places.same(folder, &other)
                })
            });
        let folders = match named {
            Some(folder) => vec![folder],
            None => {
                let Some(last) = self.folders.last() else {
                    return Vec::new();
                };
                let first = if self.folders.len() == 1 {
                    base.join(last)
                } else {
                    self.folders
                        .iter()
                        .map(|folder| base.join(folder))
                        .find(|folder| places.exists(folder))
                        .unwrap_or_else(|| base.join(last))
                };
                let mut folders = vec![first];
                if let Some(Profiles::Beside(prefix)) = self.profiles {
                    folders.extend(
                        places
                            .names(base)
                            .into_iter()
                            .filter(|name| name.strip_prefix(prefix).is_some_and(profile))
                            .map(|name| base.join(name)),
                    );
                    folders.sort();
                    folders.dedup();
                }
                folders
            }
        };
        // The profiles a folder holds, each by its name.
        let held = |folder: &Path| -> Vec<PathBuf> {
            let Some(Profiles::Inside(subfolder)) = self.profiles else {
                return Vec::new();
            };
            let profiles = folder.join(subfolder);
            let mut paths = places
                .names(&profiles)
                .into_iter()
                .filter(|name| profile(name))
                .map(|name| self.at(&profiles.join(name)))
                .filter(|path| places.exists(path))
                .collect::<Vec<_>>();
            paths.sort();
            paths
        };
        folders
            .iter()
            .flat_map(|folder| std::iter::once(self.at(folder)).chain(held(folder)))
            .collect()
    }

    /// The path in `folder`.
    fn at(&self, folder: &Path) -> PathBuf {
        if self.path.is_empty() {
            folder.to_owned()
        } else {
            folder.join(self.path)
        }
    }
}

/// A profile name: a letter or digit, then letters, digits, `_` and `-`,
/// at most 64 in all.
fn profile(name: &str) -> bool {
    name.len() <= 64
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A place the engine reads a client's sessions from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Source {
    /// The source the engine reads it as: the client's id, or a part of the
    /// client (Codex's session index reads as `codex-index`).
    pub reads_as: &'static str,
    pub location: Location,
}

/// The same sources on every OS: verified on macOS, read but unverified on
/// Windows and Linux.
pub const fn everywhere(sources: &'static [Source]) -> PerOs<&'static [Source]> {
    PerOs {
        macos: Support::Verified(sources),
        windows: Support::Unverified(sources),
        linux: Support::Unverified(sources),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    struct Disk {
        present: HashSet<PathBuf>,
    }
    impl Places for Disk {
        fn exists(&self, path: &Path) -> bool {
            self.present.contains(path)
        }
        fn same(&self, a: &Path, b: &Path) -> bool {
            a == b
        }
        fn names(&self, folder: &Path) -> Vec<String> {
            self.present
                .iter()
                .filter(|path| path.parent() == Some(folder))
                .filter_map(|path| Some(path.file_name()?.to_str()?.to_owned()))
                .collect()
        }
    }

    fn resolve(location: &Location, env: &[(&str, &str)], present: &[&str]) -> Vec<PathBuf> {
        let env: HashMap<String, String> = env
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect();
        let disk = Disk {
            present: present.iter().map(PathBuf::from).collect(),
        };
        location.resolve(Path::new("/h"), &|key| env.get(key).cloned(), &disk)
    }

    #[test]
    fn the_first_set_variable_names_the_folder() {
        let qwen = Location::home(&["QWEN_RUNTIME_DIR", "QWEN_HOME"], &[".qwen"], "projects");
        assert_eq!(
            resolve(&qwen, &[], &[]),
            [PathBuf::from("/h/.qwen/projects")]
        );
        assert_eq!(
            resolve(&qwen, &[("QWEN_HOME", " /q ")], &[]),
            [PathBuf::from("/q/projects")]
        );
        assert_eq!(
            resolve(
                &qwen,
                &[("QWEN_RUNTIME_DIR", "/r"), ("QWEN_HOME", "/q")],
                &[]
            ),
            [PathBuf::from("/r/projects")]
        );
        assert_eq!(
            resolve(&qwen, &[("QWEN_RUNTIME_DIR", " ")], &[]),
            [PathBuf::from("/h/.qwen/projects")]
        );
    }

    #[test]
    fn a_variable_naming_another_clients_folder_is_ignored() {
        let codebuddy = Location {
            not_if: Some(".workbuddy"),
            ..Location::home(&["CODEBUDDY_CONFIG_DIR"], &[".codebuddy"], "projects")
        };
        assert_eq!(
            resolve(
                &codebuddy,
                &[("CODEBUDDY_CONFIG_DIR", "/h/.workbuddy")],
                &[]
            ),
            [PathBuf::from("/h/.codebuddy/projects")]
        );
        assert_eq!(
            resolve(&codebuddy, &[("CODEBUDDY_CONFIG_DIR", "/c")], &[]),
            [PathBuf::from("/c/projects")]
        );
    }

    #[test]
    fn a_location_only_a_variable_names_is_absent_without_it() {
        let claude = Location::named_by(&["CLAUDE_CONFIG_DIR"], "projects");
        assert!(resolve(&claude, &[], &[]).is_empty());
        assert_eq!(
            resolve(&claude, &[("CLAUDE_CONFIG_DIR", "/c")], &[]),
            [PathBuf::from("/c/projects")]
        );
    }

    #[test]
    fn alternatives_and_profiles() {
        let claw = Location {
            profiles: Some(Profiles::Beside(".openclaw-")),
            ..Location::home(
                &["OPENCLAW_STATE_DIR"],
                &[".openclaw", ".clawdbot"],
                "agents",
            )
        };
        assert_eq!(
            resolve(&claw, &[], &[]),
            [PathBuf::from("/h/.clawdbot/agents")]
        );
        assert_eq!(
            resolve(
                &claw,
                &[],
                &[
                    "/h/.openclaw",
                    "/h/.openclaw-work",
                    "/h/.openclaw-",
                    "/h/.openclaw-a b"
                ]
            ),
            [
                PathBuf::from("/h/.openclaw/agents"),
                PathBuf::from("/h/.openclaw-work/agents")
            ]
        );
        assert_eq!(
            resolve(
                &claw,
                &[("OPENCLAW_STATE_DIR", "/s")],
                &["/h/.openclaw-work"]
            ),
            [PathBuf::from("/s/agents")]
        );
    }

    #[test]
    fn profiles_inside_the_folder_that_hold_the_path() {
        let hermes = Location {
            profiles: Some(Profiles::Inside("profiles")),
            ..Location::home(&["HERMES_HOME"], &[".hermes"], "config.yaml")
        };
        let resolve = |named: Option<&str>, present: &[&str]| {
            let disk = Disk {
                present: present.iter().map(PathBuf::from).collect(),
            };
            let env = |key: &str| named.filter(|_| key == "HERMES_HOME").map(str::to_owned);
            hermes.resolve(Path::new("h"), &env, &disk)
        };
        let paths = |list: &[&str]| list.iter().map(PathBuf::from).collect::<Vec<_>>();
        assert_eq!(resolve(None, &[]), paths(&["h/.hermes/config.yaml"]));
        let present = [
            "h/.hermes/profiles/work",
            "h/.hermes/profiles/work/config.yaml",
            "h/.hermes/profiles/coder",
            "h/.hermes/profiles/coder/config.yaml",
            // Not set up yet: nothing is made in it.
            "h/.hermes/profiles/fresh",
            "h/.hermes/profiles/a b",
            "h/.hermes/profiles/a b/config.yaml",
            "h/.hermes/profiles/.cache",
        ];
        assert_eq!(
            resolve(None, &present),
            paths(&[
                "h/.hermes/config.yaml",
                "h/.hermes/profiles/coder/config.yaml",
                "h/.hermes/profiles/work/config.yaml",
            ])
        );
        // The folder a variable names holds its own profiles.
        assert_eq!(
            resolve(
                Some("d"),
                &["d/profiles/ops", "d/profiles/ops/config.yaml", present[1]]
            ),
            paths(&["d/config.yaml", "d/profiles/ops/config.yaml"])
        );
    }
}
