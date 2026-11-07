//! The folders each client offers to start in: the latest ten of its own
//! sessions' projects, and a task with no project.
use super::*;

impl SessionStarts {
    pub fn update_offers(&mut self, sessions: &[Value]) -> Vec<StartOffer> {
        let offers = self.offers_for(sessions);
        self.offers = offers.clone();
        offers
    }
    /// What `sessions` offer to start in, without keeping it as the phone's
    /// offers: the latest ten folders per client, and its own task folder.
    pub fn offers_for(&self, sessions: &[Value]) -> Vec<StartOffer> {
        let mut rows = sessions
            .iter()
            .filter(|session| session["parentId"].is_null())
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| {
            b["updatedAt"]
                .as_str()
                .cmp(&a["updatedAt"].as_str())
                .then_with(|| b["id"].as_str().cmp(&a["id"].as_str()))
        });
        let mut offers = Vec::new();
        for provider in [Provider::Claude, Provider::Codex, Provider::Antigravity] {
            if self.executable(provider).is_none()
                && !(provider == Provider::Antigravity && self.desktop(Provider::Antigravity))
            {
                continue;
            }
            let name = provider.as_str();
            let mut folders = Vec::new();
            for session in &rows {
                if session["source"] != name {
                    continue;
                }
                let Some(folder) = session["cwd"]
                    .as_str()
                    .and_then(|path| self.project_folder(path))
                else {
                    continue;
                };
                if !folder.is_dir() || !self.accepts(provider, &folder) {
                    continue;
                }
                let written = self.written(&folder);
                if !folders.contains(&written) {
                    folders.push(written);
                }
                if folders.len() == 10 {
                    break;
                }
            }
            // Each client also offers a task with no project, in a folder of
            // its own under the task folder. Claude Code starts only in a
            // folder it trusts, so that start trusts the task folder first.
            offers.push(StartOffer {
                provider,
                app: if self.desktop(provider) {
                    "desktop"
                } else {
                    "cli"
                }
                .into(),
                folders,
                without_folder: true,
            });
        }
        offers
    }
    /// The folder `cwd` names as an offer would write it: a worktree's
    /// repository, under `~` when inside home. None for Codex's own worktrees.
    pub fn offered_folder(&self, cwd: &str) -> Option<String> {
        self.project_folder(cwd).map(|folder| self.written(&folder))
    }
    pub fn update_offers_ref(&self) -> &[StartOffer] {
        &self.offers
    }
    pub(super) fn project_folder(&self, path: &str) -> Option<PathBuf> {
        let expanded = if path == "~" {
            self.config.home.clone()
        } else if let Some(tail) = path.strip_prefix("~/") {
            self.config.home.join(tail)
        } else {
            PathBuf::from(path)
        };
        if !expanded.is_absolute() {
            return None;
        }
        if expanded.starts_with(self.config.home.join(".codex/worktrees")) {
            return None;
        }
        let project = expanded
            .ancestors()
            .find(|path| {
                path.file_name().is_some_and(|name| name == "worktrees")
                    && path
                        .parent()
                        .and_then(Path::file_name)
                        .is_some_and(|name| name == ".claude")
            })
            .and_then(Path::parent)
            .and_then(Path::parent)
            .map(Path::to_path_buf);
        Some(project.unwrap_or(expanded))
    }
    pub(super) fn written(&self, path: &Path) -> String {
        if path == self.config.home {
            "~".into()
        } else if let Ok(tail) = path.strip_prefix(&self.config.home) {
            format!(
                "~/{}",
                tail.iter()
                    .map(|part| part.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            )
        } else {
            path.to_string_lossy().into()
        }
    }
}
