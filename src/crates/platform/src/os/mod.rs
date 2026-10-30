//! Compile-time host boundary. Provider schemas and account/usage decisions stay
//! in the shared adapters; only paths, native application lookup and OS commands
//! are selected here. A successful portable read is data, not a cross-OS QA claim.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
pub(crate) type Environment = HashMap<String, String>;
#[cfg(target_os = "macos")]
#[path = "macos/mod.rs"]
mod target;
#[cfg(target_os = "windows")]
#[path = "windows/mod.rs"]
mod target;
#[cfg(target_os = "linux")]
#[path = "linux/mod.rs"]
mod target;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
compile_error!("agent-hud-platform supports macOS, Windows and Linux host builds only");
#[cfg(unix)]
mod unix;
pub(crate) use target::*;

/// The directory a client's variable moves its files to: the value without
/// the whitespace around it, used as written, as the clients themselves read
/// it (agent-hud-native-open ClientHome.variable); None when unset, empty or only
/// whitespace. `home` is kept for callers that resolve relative defaults.
pub(crate) fn variable(env: &Environment, key: &str, _home: &Path) -> Option<PathBuf> {
    environment_value(env, key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
/// A base folder of the agents' locations on this host.
pub(crate) fn folder(home: &Path, env: &Environment, base: agent_hud_agents::Base) -> PathBuf {
    match base {
        agent_hud_agents::Base::Home => home.to_path_buf(),
        agent_hud_agents::Base::AppData => app_support(home, env),
        agent_hud_agents::Base::DataHome => data_home(home, env),
        agent_hud_agents::Base::ConfigHome => config_home(home, env),
    }
}
/// The file system, for resolving the agents' locations.
pub(crate) struct Disk;
impl agent_hud_agents::Places for Disk {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn same(&self, a: &Path, b: &Path) -> bool {
        a.canonicalize()
            .ok()
            .zip(b.canonicalize().ok())
            .is_some_and(|(a, b)| a == b)
    }
    fn names(&self, folder: &Path) -> Vec<String> {
        std::fs::read_dir(folder)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
            .collect()
    }
}
/// Every path `location` names on this host.
pub(crate) fn resolve(
    home: &Path,
    env: &Environment,
    location: &agent_hud_agents::Location,
) -> Vec<PathBuf> {
    location.resolve(
        &folder(home, env, location.base),
        &|key| environment_value(env, key).map(str::to_owned),
        &Disk,
    )
}
/// Where the engine reads sessions on this host: every agent's sources, in
/// the agents' order, each path once.
pub(crate) fn agent_source_roots(
    home: &Path,
    env: &Environment,
) -> Vec<agent_hud_core::SourceRoot> {
    let mut roots: Vec<agent_hud_core::SourceRoot> = Vec::new();
    for agent in agent_hud_agents::ALL {
        for source in agent.sources.on(OS).value().copied().unwrap_or_default() {
            for path in resolve(home, env, &source.location) {
                if !roots
                    .iter()
                    .any(|root| root.path == path && root.provider == source.reads_as)
                {
                    roots.push(agent_hud_core::SourceRoot {
                        path,
                        provider: source.reads_as,
                    });
                }
            }
        }
    }
    roots
}
pub(crate) fn config_home(home: &Path, env: &Environment) -> PathBuf {
    variable(env, "XDG_CONFIG_HOME", home).unwrap_or_else(|| home.join(".config"))
}
pub(crate) fn data_home(home: &Path, env: &Environment) -> PathBuf {
    variable(env, "XDG_DATA_HOME", home).unwrap_or_else(|| home.join(".local/share"))
}
pub(crate) fn path_entries(env: &Environment) -> Vec<PathBuf> {
    environment_value(env, "PATH")
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .collect()
}
pub(crate) fn joined_path(paths: Vec<PathBuf>) -> String {
    std::env::join_paths(paths)
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
/// A parent process of this one, as the kernel reports it.
pub(crate) struct ProcessAncestor {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    /// The application bundle identifier, on hosts that have bundles.
    pub bundle: Option<String>,
}
#[derive(Clone)]
pub(crate) struct ProcessInfo {
    pub pid: u32,
    pub executable: String,
    pub command: String,
}

pub(crate) fn capabilities() -> serde_json::Value {
    serde_json::json!({"os":std::env::consts::OS,"validation":VALIDATION,"clientDiscovery":"implemented","portableAccountReaders":{"claude":PORTABLE_VALIDATION,"codex":PORTABLE_VALIDATION,"cursor":PORTABLE_VALIDATION,"deepseek":PORTABLE_VALIDATION},"antigravityLocalService":agent_hud_agents::antigravity::LOCAL_SERVICE.support.on(OS).label(),"legacyMacHistory":LEGACY_MAC_HISTORY,"scripts":"build-time-embedded"})
}

/// Where the engine reads sessions: the agents' sources on this host. Each
/// host selects this explicitly; WSL, Store and client-container roots need
/// adapters of their own.
pub(super) fn portable_cli_roots(home: &Path) -> Vec<agent_hud_core::SourceRoot> {
    agent_source_roots(home, &std::env::vars().collect::<Environment>())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sources every agent's descriptor names, under a home without
    /// variables set (Grok Bot's cache is left out: only macOS has it;
    /// Cursor's places differ by OS and are checked after).
    #[test]
    fn roots_follow_the_descriptors() {
        let home = Path::new("/h");
        let all = agent_source_roots(home, &Environment::new());
        let roots = all
            .iter()
            .filter(|root| !matches!(root.provider, "grok-bot" | "cursor"))
            .map(|root| {
                let path = root.path.strip_prefix(home).unwrap().to_owned();
                (root.provider, path)
            })
            .collect::<Vec<_>>();
        // Cursor: its database under the application data folder, then,
        // where its hooks run, the turn log in Agent HUD's data folder.
        let cursor = all
            .iter()
            .filter(|root| root.provider == "cursor")
            .map(|root| root.path.clone())
            .collect::<Vec<_>>();
        assert!(cursor[0].ends_with("Cursor/User/globalStorage/state.vscdb"));
        assert!(cursor.len() <= 2);
        assert!(cursor[1..]
            .iter()
            .all(|path| path.ends_with(Path::new(target::DATA_FOLDER).join("clients/cursor"))));
        let expected = [
            ("claude", ".claude/projects"),
            ("claude", ".config/claude/projects"),
            ("codex-index", ".codex/session_index.jsonl"),
            ("codex", ".codex/sessions"),
            ("codex", ".codex/archived_sessions"),
            ("deepseek", ".dsh/sessions"),
            ("antigravity", ".gemini/antigravity-cli"),
            ("antigravity", ".gemini/antigravity-cli/conversations"),
            ("antigravity", ".gemini/antigravity"),
            ("antigravity", ".gemini/antigravity/conversations"),
            ("antigravity", ".gemini/antigravity-ide"),
            ("grok-cli", ".grok/sessions"),
            ("grok-cli", ".grok/logs"),
            ("copilot", ".copilot/session-state"),
            ("copilot", ".copilot/otel"),
            ("openclaw", ".clawdbot/agents"),
            ("hermes", ".hermes"),
            ("zcode", ".zcode/cli/db"),
            ("codebuddy", ".codebuddy/projects"),
            ("workbuddy", ".workbuddy/projects"),
            ("qwen", ".qwen/projects"),
            ("opencode", ".local/share/opencode"),
            ("opencode", ".local/share/opencode/storage/message"),
            ("kimi", ".kimi-code/sessions"),
            ("kimi", ".kimi/sessions"),
            ("pi", ".pi/agent/sessions"),
            ("devin", ".local/share/devin/cli"),
        ];
        let expected = expected
            .iter()
            .map(|(provider, path)| (*provider, PathBuf::from(path)))
            .collect::<Vec<_>>();
        assert_eq!(roots, expected);
    }
}
