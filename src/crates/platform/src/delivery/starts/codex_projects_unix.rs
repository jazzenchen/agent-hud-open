//! Unix Codex starts offer recent folders and use the existing start route.
use super::*;

#[derive(Default)]
pub(super) struct ProjectCatalog;

impl ProjectCatalog {
    pub(super) fn offer_folders(
        &self,
        starts: &SessionStarts,
        recent: impl Iterator<Item = PathBuf>,
    ) -> Vec<String> {
        starts.recent_folders(recent)
    }
}

impl SessionStarts {
    pub(super) fn can_start_codex(&self) -> bool {
        self.executable(Provider::Codex).is_some()
    }

    pub(super) fn codex_start_project(
        &self,
        _folder: &Path,
        _request: &StartRequest,
    ) -> Result<Option<String>> {
        Ok(None)
    }
}
