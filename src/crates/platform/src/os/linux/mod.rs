pub(crate) use super::unix::{
    environment_value, hook_configuration, hook_text, is_shell, owned_by_user, owner_only,
    private_append, private_directory, private_file, read_stdin,
};
use super::{config_home, data_home, joined_path, path_entries, Environment, ProcessInfo};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
mod apps;
mod process;
mod secrets;
pub(crate) use apps::{activate_app, app_running, launch_app, ACTIVATES_APPS};
pub(crate) use process::{
    codex_cli_on_port, parent_processes, process_runs, process_start, terminal_io,
};
/// No application bundles here: an MCP call is matched by its own parent.
pub(crate) fn app_ancestors(_pid: u32) -> Vec<u32> {
    Vec::new()
}
pub(crate) use secrets::{app_cookie, stored_secret, unseal_safe_storage};
pub(crate) const READS_PROCESS_ENVIRONMENT: bool = false;
/// Claude has no desktop app on this host to take a background job in.
pub(crate) const MOVES_CLAUDE_JOBS: bool = false;
/// Claude has no desktop app on this host to open a Project thread in.
pub(crate) const OPENS_CLAUDE_PROJECTS: bool = false;
pub(crate) const VALIDATION: &str = "unverified";
pub(crate) const PORTABLE_VALIDATION: &str = "unverified";
/// The operating system whose facts the agents' descriptors give here.
pub(crate) const OS: agent_hud_agents::Os = agent_hud_agents::Os::Linux;
pub(crate) const LEGACY_MAC_HISTORY: bool = false;
/// An AppImage runs from a new temporary mount (`$APPDIR`) at every launch;
/// the image file (`$APPIMAGE`) is the program that stays put.
pub(crate) fn launched_executable() -> std::io::Result<PathBuf> {
    let current = std::env::current_exe()?;
    Ok(appimage(&current, &|key| std::env::var_os(key)).unwrap_or(current))
}
fn appimage(
    current: &Path,
    variable: &dyn Fn(&str) -> Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let image = variable("APPIMAGE")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_file())?;
    let mount = variable("APPDIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())?;
    current.starts_with(&mount).then_some(image)
}
pub(crate) fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "User home directory unavailable".into())
}
/// The folder Agent HUD's data folder is in, and that folder's name.
pub(crate) fn data_parent(home: &Path) -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"))
}
pub(crate) const DATA_FOLDER: &str = crate::data_folder::LINUX;
pub(crate) fn caches_dir(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".cache"))
}
pub(crate) fn app_support(home: &Path, env: &Environment) -> PathBuf {
    config_home(home, env)
}
/// Where a Claude desktop build keeps its session metadata: the config folder.
pub(crate) fn claude_desktop_bases(home: &Path, env: &Environment) -> Vec<PathBuf> {
    vec![app_support(home, env).join("Claude")]
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
        home.join(".kimi-code/bin"),
        home.join(".zcode/bin"),
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
pub(crate) fn spawn(command: &mut Command) -> std::io::Result<std::process::Child> {
    command.spawn()
}
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

/// Not implemented on this host: callers fall back to the client's link.
pub(crate) fn open_folder_in_app(_bundle_id: &str, _folder: &std::path::Path) -> bool {
    false
}

/// Not implemented on this host: nothing opens a link in a named app here.
pub(crate) fn open_url_in_app(_bundle_id: &str, _url: &str) -> bool {
    false
}

pub(crate) fn open_session_url(url: &str) -> Result<(), String> {
    super::unix::spawn_script(
        include_str!("../../../scripts/linux/open-session-url.sh"),
        &[std::ffi::OsStr::new(url)],
    )
    .map_err(|_| "The native client could not open this session".into())
}

/// Swift UserDefaults has no native source on this host.
pub(crate) fn legacy_preferences() -> Option<serde_json::Value> {
    None
}

pub(crate) fn prepare_socket_address(_address: &mut libc::sockaddr_un) {}
/// Linux suppresses SIGPIPE per send with MSG_NOSIGNAL instead.
pub(crate) fn suppress_sigpipe(_fd: libc::c_int) {}
pub(crate) const SOCKET_SEND_FLAGS: libc::c_int = libc::MSG_NOSIGNAL;

#[cfg(test)]
mod tests {
    #[test]
    fn an_appimage_names_its_image_file_not_its_mount() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("Agent-HUD.AppImage");
        std::fs::write(&image, b"fixture only").unwrap();
        let mount = directory.path().join(".mount_AgentH1");
        let current = mount.join("usr/bin/Agent HUD");
        let variables = |image: std::path::PathBuf, mount: std::path::PathBuf| {
            move |key: &str| match key {
                "APPIMAGE" => Some(image.clone().into_os_string()),
                "APPDIR" => Some(mount.clone().into_os_string()),
                _ => None,
            }
        };
        let found = super::appimage(&current, &variables(image.clone(), mount.clone()));
        assert_eq!(found, Some(image.clone()));
        // Not run from that mount, a missing image, or relative paths: the
        // executable itself stays the program.
        let elsewhere = directory.path().join("bin/Agent HUD");
        assert_eq!(
            super::appimage(&elsewhere, &variables(image.clone(), mount.clone())),
            None
        );
        let missing = directory.path().join("gone.AppImage");
        assert_eq!(
            super::appimage(&current, &variables(missing, mount.clone())),
            None
        );
        assert_eq!(
            super::appimage(&current, &variables("Agent-HUD.AppImage".into(), mount)),
            None
        );
        assert_eq!(super::appimage(&current, &|_: &str| None), None);
    }
}
