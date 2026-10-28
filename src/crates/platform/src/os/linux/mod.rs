pub(crate) use super::unix::{
    environment_value, hook_configuration, is_shell, owned_by_user, owner_only, private_append,
    private_directory, private_file, read_stdin,
};
use super::{config_home, data_home, joined_path, path_entries, Environment, ProcessInfo};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
mod apps;
mod process;
mod secrets;
pub(crate) use apps::{activate_app, app_running, ACTIVATES_APPS};
pub(crate) use process::{codex_cli_on_port, parent_processes};
pub(crate) use secrets::stored_secret;
pub(crate) const READS_PROCESS_ENVIRONMENT: bool = false;
/// Claude has no desktop app on this host to take a background job in.
pub(crate) const MOVES_CLAUDE_JOBS: bool = false;
pub(crate) const VALIDATION: &str = "unverified";
pub(crate) const PORTABLE_VALIDATION: &str = "unverified";
/// The operating system whose facts the agents' descriptors give here.
pub(crate) const OS: agent_hud_agents::Os = agent_hud_agents::Os::Linux;
pub(crate) const LEGACY_MAC_HISTORY: bool = false;
pub(crate) fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "User home directory unavailable".into())
}
pub(crate) fn data_dir(home: &Path) -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"))
        .join("app.agenthud.next")
}
pub(crate) fn caches_dir(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".cache"))
}
pub(crate) fn app_support(home: &Path, env: &Environment) -> PathBuf {
    config_home(home, env)
}
pub(crate) fn application_installed(home: &Path, env: &Environment, name: &str) -> bool {
    let file = format!("{}.desktop", name.to_lowercase().replace(' ', "-"));
    [
        data_home(home, env).join("applications"),
        PathBuf::from("/usr/share/applications"),
    ]
    .iter()
    .any(|dir| dir.join(&file).is_file())
}
pub(crate) fn registered_bundle(_id: &str) -> bool {
    false
}
/// URL-handler lookup through XDG associations is not implemented.
pub(crate) fn registered_url(_url: &str) -> bool {
    false
}
pub(crate) fn executable_dirs(home: &Path, env: &Environment) -> Vec<PathBuf> {
    let mut dirs = path_entries(env);
    dirs.extend([
        home.join(".local/bin"),
        home.join(".bun/bin"),
        home.join(".npm-global/bin"),
        "/usr/local/bin".into(),
        "/usr/bin".into(),
        "/bin".into(),
    ]);
    dirs
}
pub(crate) fn discovered_executable(path: &Path) -> bool {
    super::unix::executable(path)
}
pub(crate) fn engine_executable(path: &Path) -> bool {
    super::unix::executable(path)
}
pub(crate) fn configure_command(_command: &mut Command) {}
pub(crate) fn engine_candidates(home: &Path, env: &Environment, name: &str) -> Vec<PathBuf> {
    let mut paths = vec![
        home.join(".local/bin").join(name),
        home.join(".bun/bin").join(name),
        home.join(".claude/local").join(name),
        PathBuf::from("/usr/local/bin").join(name),
    ];
    if name == "claude" {
        paths.extend(super::unix::newest(
            &home.join(".local/share/claude/versions"),
        ));
    }
    paths.extend(path_entries(env).into_iter().map(|dir| dir.join(name)));
    paths
}
pub(crate) fn engine_path(home: &Path, env: &Environment) -> String {
    let mut dirs = vec![
        home.join(".bun/bin"),
        home.join(".local/bin"),
        "/usr/local/bin".into(),
        "/usr/bin".into(),
        "/bin".into(),
    ];
    dirs.extend(path_entries(env));
    joined_path(dirs)
}
pub(crate) fn reveal_transcript(path: &Path) -> Result<(), String> {
    super::unix::spawn_script(
        include_str!("../../../scripts/linux/reveal-transcript.sh"),
        &[path.as_os_str()],
    )
}
pub(crate) fn legacy_quota_database(_home: &Path) -> Option<PathBuf> {
    None
}
pub(crate) fn processes() -> Result<Vec<ProcessInfo>, String> {
    let mut processes = super::unix::processes(
        include_str!("../../../scripts/linux/process-names.sh"),
        include_str!("../../../scripts/linux/process-arguments.sh"),
    )?;
    // Linux ps `comm` is a truncated basename. /proc gives the executable path,
    // preventing an argument string from masquerading as the executable.
    for process in &mut processes {
        process.executable = std::fs::read_link(format!("/proc/{}/exe", process.pid))
            .ok()
            .and_then(|p| p.into_os_string().into_string().ok())
            .unwrap_or_default();
    }
    processes.retain(|process| !process.executable.is_empty());
    Ok(processes)
}
pub(crate) fn listening_ports(pid: u32) -> Result<Vec<u16>, String> {
    super::unix::listening_ports(
        include_str!("../../../scripts/linux/listening-ports.sh"),
        pid,
    )
}

pub(crate) fn source_roots(home: &Path) -> Vec<agent_hud_core::SourceRoot> {
    super::portable_cli_roots(home)
}

pub(crate) fn open_session_url(url: &str) -> Result<(), String> {
    super::unix::spawn_script(
        include_str!("../../../scripts/linux/open-session-url.sh"),
        &[std::ffi::OsStr::new(url)],
    )
    .map_err(|_| "The native client could not open this session".into())
}

/// Swift UserDefaults has no native source on this host.
pub(crate) fn legacy_preferences() -> Result<Option<serde_json::Value>, String> {
    Ok(None)
}

pub(crate) fn prepare_socket_address(_address: &mut libc::sockaddr_un) {}
/// Linux suppresses SIGPIPE per send with MSG_NOSIGNAL instead.
pub(crate) fn suppress_sigpipe(_fd: libc::c_int) {}
pub(crate) const SOCKET_SEND_FLAGS: libc::c_int = libc::MSG_NOSIGNAL;

#[cfg(test)]
mod tests {
    #[test]
    fn url_handler_discovery_is_unavailable() {
        assert!(!super::registered_url("dsh://open"));
    }
}
