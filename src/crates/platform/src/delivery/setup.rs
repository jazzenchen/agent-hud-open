//! The clients as a host that sends into sessions and starts them finds them
//! on this computer: their folders and command lines.
use super::*;
use std::{collections::BTreeMap, path::Path};

impl StartConfig {
    /// For the user whose home is `home`: Claude Code's and Codex's folders
    /// (`CLAUDE_CONFIG_DIR` and `CODEX_HOME`, else in the home folder), the
    /// clients' command lines, and the folder for tasks with no project,
    /// `~/.agenthud/sessions/<client>/<request>`.
    pub fn find(home: &Path) -> Self {
        Self {
            home: home.to_path_buf(),
            claude_config: crate::claude_plugin::claude_config_directory(home),
            codex_data: std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .unwrap_or_else(|| home.join(".codex")),
            claude_executable: crate::client_executable("claude"),
            codex_executable: crate::client_executable("codex"),
            antigravity_executable: crate::client_executable("agy"),
            claude_desktop: false,
            codex_desktop: false,
            task_root: home.join(".agenthud").join("sessions"),
            environment: BTreeMap::new(),
        }
    }
}
