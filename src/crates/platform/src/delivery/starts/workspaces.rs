//! DeepSeek Harness's own workspaces, the folders its sidebar lists
//! (`$DSH_HOME/storages/workspace.json`), offered to start in beside its
//! recent sessions' folders: a workspace left a day or more stays offered.
use super::*;

impl SessionStarts {
    /// Harness's workspaces in its own order, absolute paths only.
    pub(super) fn deepseek_workspaces(&self) -> Vec<PathBuf> {
        let home =
            crate::hud_tools::Places::new(&self.config.home, &self.client_environment()).deepseek;
        let file = read_json(&home.join("storages").join("workspace.json"));
        let table = &file["tables"]["workspaces"];
        file["global"]["workspaceIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|id| table[id.as_str()?]["path"].as_str())
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .collect()
    }
}
