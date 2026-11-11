//! Where each client keeps its settings and skills on this machine.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Where the clients keep their settings on this machine.
#[derive(Clone, Debug)]
pub struct Places {
    pub home: PathBuf,
    /// Claude Code's folder: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
    pub claude: PathBuf,
    /// DeepSeek Harness's folder: `$DSH_HOME`, else `~/.dsh`.
    pub deepseek: PathBuf,
    pub(super) environment: HashMap<String, String>,
}

impl Places {
    pub fn new(home: &Path, environment: &HashMap<String, String>) -> Self {
        let claude = environment
            .get("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.join(".claude"));
        let deepseek = match environment
            .get("DSH_HOME")
            .map(String::as_str)
            .filter(|path| !path.trim().is_empty())
        {
            None => home.join(".dsh"),
            Some("~") => home.to_path_buf(),
            Some(path) => match path.strip_prefix("~/") {
                Some(tail) => home.join(tail),
                None => PathBuf::from(path),
            },
        };
        Self {
            home: home.to_path_buf(),
            claude,
            deepseek,
            environment: environment.clone(),
        }
    }

    /// From this process's environment.
    pub fn current(home: &Path) -> Self {
        Self::new(home, &std::env::vars().collect())
    }
}

pub(super) fn antigravity(places: &Places) -> PathBuf {
    places.home.join(".gemini").join("config")
}

/// Pi's agent folder: `$PI_CODING_AGENT_DIR`, else `~/.pi/agent`. Its MCP
/// servers are in `mcp.json` there (Pi's docs/mcp.md).
pub(super) fn pi(places: &Places) -> PathBuf {
    places
        .environment
        .get("PI_CODING_AGENT_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| places.home.join(".pi").join("agent"))
}

/// OpenCode's configuration folder: `opencode` in `$XDG_CONFIG_HOME`, else
/// in `~/.config`. Its MCP servers are under `mcp` in `opencode.json`.
pub(super) fn opencode(places: &Places) -> PathBuf {
    places
        .environment
        .get("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| places.home.join(".config"))
        .join("opencode")
}

pub(super) fn qoder(places: &Places) -> PathBuf {
    places
        .environment
        .get("QODER_CONFIG_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| places.home.join(".qoder"))
}

/// The MCP servers TRAE reads for the user: `User/mcp.json` in the build's
/// app data folder, beside its `settings.json` (TRAE's workbench reads
/// `userSettingsResource/../mcp.json`). A `.trae/mcp.json` is a project's
/// own, read only in a workspace. TRAE names its China build's folder
/// `Trae CN`, the international one's `Trae`.
pub(super) fn trae_mcp(places: &Places, folder: &str) -> PathBuf {
    crate::os::app_support(&places.home, &places.environment)
        .join(folder)
        .join("User")
        .join("mcp.json")
}
