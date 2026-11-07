//! How a client's command line runs: in the configured environment, with the
//! folders command lines are installed in on its `PATH`.
use super::super::process::CommandPlan;
use super::*;

impl SessionStarts {
    pub(super) fn plan(
        &self,
        executable: &Path,
        arguments: Vec<String>,
        directory: Option<&Path>,
        seconds: u64,
    ) -> CommandPlan {
        plan(&self.config, executable, arguments, directory, seconds)
    }
}

/// The environment a client starts in: the configured one, with the places
/// command lines are installed in on its `PATH`, which a GUI app lacks.
pub(super) fn environment(config: &StartConfig) -> BTreeMap<String, String> {
    let mut env = config.environment.clone();
    let path_key = env.keys().find(|key| host::is_path_key(key)).cloned();
    let base = path_key
        .and_then(|key| env.remove(&key))
        .map(std::ffi::OsString::from)
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    let mut folders = vec![config.home.join(".local/bin"), config.home.join(".bun/bin")];
    folders.extend(host::command_folders());
    folders.extend(std::env::split_paths(&base));
    if let Ok(path) = std::env::join_paths(folders) {
        env.insert("PATH".into(), path.to_string_lossy().into_owned());
    }
    env
}
pub(super) fn plan(
    config: &StartConfig,
    executable: &Path,
    arguments: Vec<String>,
    directory: Option<&Path>,
    seconds: u64,
) -> CommandPlan {
    CommandPlan {
        executable: executable.into(),
        arguments,
        directory: directory.map(Path::to_path_buf),
        environment: environment(config),
        timeout: Duration::from_secs(seconds),
    }
}
