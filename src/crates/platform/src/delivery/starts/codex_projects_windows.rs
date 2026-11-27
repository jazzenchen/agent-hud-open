//! Project lookup and start offers for Windows Codex Desktop.
use super::*;

type CatalogResult = Result<Vec<codex::Project>>;

#[derive(Default)]
pub(super) struct ProjectCatalog {
    cache: Mutex<Option<(std::time::Instant, CatalogResult)>>,
    #[cfg(test)]
    fixture: Option<CatalogResult>,
}

impl ProjectCatalog {
    #[cfg(test)]
    pub(super) fn set_fixture(&mut self, result: CatalogResult) {
        self.fixture = Some(result);
    }

    /// Offers can reuse a recent catalog; a start reads it again before
    /// creating a thread so an old offer cannot assign the wrong project.
    fn read(&self, config: &StartConfig, fresh: bool) -> CatalogResult {
        #[cfg(test)]
        if let Some(result) = &self.fixture {
            return result.clone();
        }
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((read, result)) = cache.as_ref() {
            if !fresh && read.elapsed() < Duration::from_secs(30) {
                return result.clone();
            }
        }
        let result = codex::projects(config).map_err(|error| {
            Failure::Unavailable(format!("Could not list Codex Desktop projects: {error}"))
        });
        *cache = Some((std::time::Instant::now(), result.clone()));
        result
    }

    pub(super) fn offer_folders(
        &self,
        starts: &SessionStarts,
        recent: impl Iterator<Item = PathBuf>,
    ) -> Vec<String> {
        let projects = self.read(&starts.config, false).unwrap_or_default();
        let covered = |folder: &Path| {
            codex::project_for_folder(&projects, folder).is_ok_and(|id| id.is_some())
        };
        let mut folders = starts.recent_folders(recent.filter(|folder| covered(folder)));
        for root in projects.iter().flat_map(codex::Project::roots) {
            let written = starts.written(root);
            if !root.is_dir() || folders.contains(&written) {
                continue;
            }
            if starts
                .project_folder(&written)
                .is_some_and(|folder| covered(&folder))
            {
                folders.push(written);
            }
        }
        folders
    }
}

impl SessionStarts {
    pub(super) fn can_start_codex(&self) -> bool {
        self.desktop(Provider::Codex) && self.executable(Provider::Codex).is_some_and(Path::is_file)
    }

    pub(super) fn codex_start_project(
        &self,
        folder: &Path,
        request: &StartRequest,
    ) -> Result<Option<String>> {
        if request.model.is_some() {
            return Err(Failure::Unsupported(
                "Codex Desktop runs the first queued message in its own model".into(),
            ));
        }
        if !self.can_start_codex() {
            return Err(Failure::Unavailable(
                "Starting Codex on Windows requires Codex Desktop and its executable".into(),
            ));
        }
        if request.folder.is_empty() {
            return Ok(None);
        }
        let projects = self.codex_projects.read(&self.config, true)?;
        let id = codex::project_for_folder(&projects, folder)?.ok_or_else(|| {
            Failure::Unavailable(format!(
                "No Codex Desktop project covers {}",
                folder.display()
            ))
        })?;
        Ok(Some(id.to_owned()))
    }
}
